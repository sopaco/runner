use super::*;
use crate::model::CodexSpeed;

#[test]
fn direct_spawn_records_effective_inputs_before_machine_settings() {
    let pool = pool_with_schema();
    let mut configured = role(
        "codex-configured",
        &["--search", "--sandbox", "workspace-write"],
    );
    configured.runtime = "codex".into();
    configured
        .env
        .insert("RUNNER_878_MARKER".into(), "before".into());
    configured.system_prompt = Some("original persona".into());
    configured.codex_speed = Some(CodexSpeed::Fast);
    insert_role_row(&pool.get().unwrap(), &configured);
    let fake = fake_runtime();
    let mgr = mgr_with_fake(None, fake.clone());
    let out = mgr
        .spawn_direct_with_speed(
            &configured,
            None,
            Some("selected-model"),
            Some("xhigh"),
            Some(CodexSpeed::Standard),
            None,
            Some(fixture_tmp_dir().to_str().unwrap()),
            None,
            None,
            fixture_tmp_dir(),
            pool.clone(),
            capture(),
            None,
        )
        .unwrap();
    let row = crate::repo::session::get_row(&pool.get().unwrap(), &out.id)
        .unwrap()
        .unwrap();
    let definition = row.launch_definition.unwrap();
    assert_eq!(row.agent_runtime.as_deref(), Some("codex"));
    assert_eq!(definition.command, "codex-configured");
    assert_eq!(
        definition.args,
        ["--search", "--sandbox", "workspace-write"]
    );
    assert_eq!(
        definition.env.get("RUNNER_878_MARKER").map(String::as_str),
        Some("before")
    );
    assert_eq!(definition.model.as_deref(), Some("selected-model"));
    assert_eq!(definition.effort.as_deref(), Some("xhigh"));
    assert_eq!(definition.codex_speed, Some(CodexSpeed::Standard));
    assert_eq!(definition.system_prompt, configured.system_prompt);
    assert_eq!(
        row.agent_command,
        Some(fake.last_spawn_spec().unwrap().command)
    );
    mgr.kill(&out.id).unwrap();
}

#[test]
fn direct_resume_keeps_every_recorded_input_and_agent_defaults_after_role_edit() {
    for runtime in ["codex", "pi", "antigravity"] {
        let pool = pool_with_schema();
        let home = tempfile::tempdir().unwrap();
        let mut configured = role("original-command", &["original-arg"]);
        configured.runtime = runtime.into();
        configured
            .env
            .insert("RUNNER_878_MARKER".into(), "before".into());
        configured.system_prompt = Some("ORIGINAL_PERSONA".into());
        insert_role_row(&pool.get().unwrap(), &configured);
        let fake = fake_runtime();
        let mgr = mgr_with_fake(None, fake.clone());
        let out = mgr
            .spawn_direct(
                &configured,
                None,
                None,
                None,
                None,
                Some(home.path().to_str().unwrap()),
                None,
                None,
                home.path(),
                pool.clone(),
                capture(),
                None,
            )
            .unwrap();
        let original = crate::repo::session::get_row(&pool.get().unwrap(), &out.id)
            .unwrap()
            .unwrap()
            .launch_definition
            .unwrap();
        mgr.kill(&out.id).unwrap();
        configured.runtime = "claude-code".into();
        configured.command = "edited-command".into();
        configured.args = vec!["edited-arg".into()];
        configured
            .env
            .insert("RUNNER_878_MARKER".into(), "after".into());
        configured.model = Some("edited-model".into());
        configured.effort = Some("high".into());
        configured.codex_speed = Some(CodexSpeed::Fast);
        configured.system_prompt = Some("EDITED_PERSONA".into());
        update_role_row(&pool.get().unwrap(), &configured);
        // Keyless Codex and agy use the fresh-conversation fallback; pi composes a system prompt on every launch.
        crate::runtimes::with_conversation_home(home.path(), || {
            mgr.resume(&out.id, None, None, home.path(), pool.clone(), capture())
        })
        .unwrap();
        let spec = fake.last_spawn_spec().unwrap();
        assert_eq!(spec.command, "original-command", "{runtime}");
        assert_eq!(
            spec.env.get("RUNNER_878_MARKER").map(String::as_str),
            Some("before")
        );
        assert!(spec.args.contains(&"original-arg".into()));
        assert!(!spec.args.iter().any(|arg| arg.contains("edited")));
        if runtime == "antigravity" {
            assert!(spec.args.iter().any(|arg| arg.contains("ORIGINAL_PERSONA")));
        }
        if runtime == "pi" {
            let prompt_path = spec
                .args
                .windows(2)
                .find(|args| args[0] == "--append-system-prompt")
                .unwrap();
            let body = std::fs::read_to_string(&prompt_path[1]).unwrap();
            assert!(body.contains("ORIGINAL_PERSONA"));
            assert!(!body.contains("EDITED_PERSONA"));
        }
        let row = crate::repo::session::get_row(&pool.get().unwrap(), &out.id)
            .unwrap()
            .unwrap();
        assert_eq!(row.launch_definition.as_ref(), Some(&original));
        assert_eq!(row.agent_runtime.as_deref(), Some(runtime));
        assert_eq!(row.agent_command.as_deref(), Some("original-command"));
        mgr.kill(&out.id).unwrap();
    }
}

#[test]
fn mission_resume_keeps_definition_and_restart_rebuilds_from_role_and_roster() {
    for lead in [false, true] {
        let (pool, home, id) = slot_respawn_fixture("codex", lead);
        let conn = pool.get().unwrap();
        conn.execute("UPDATE sessions SET agent_session_key = NULL", [])
            .unwrap();
        let mut row = crate::repo::session::get_row(&conn, &id).unwrap().unwrap();
        let mut original = row.launch_definition.take().unwrap();
        original.command = "original-command".into();
        original.args = vec!["original-arg".into()];
        original
            .env
            .insert("RUNNER_878_MARKER".into(), "before".into());
        original.codex_speed = Some(CodexSpeed::Standard);
        crate::repo::session::set_launch(&conn, &id, "codex", &original).unwrap();
        conn.execute("UPDATE slots SET model_override = 'slot-model', effort_override = 'xhigh', runtime_options_json = '{\"codex\":{\"speed\":\"standard\"}}' WHERE id = ?1", [row.slot_id.as_deref().unwrap()]).unwrap();
        let role_id = row.role_id.as_deref().unwrap();
        let mut edited = crate::ops::role::get(&conn, role_id).unwrap();
        edited.command = "edited-command".into();
        edited.args = vec!["edited-arg".into()];
        edited.model = Some("edited-model".into());
        edited.effort = Some("high".into());
        edited.codex_speed = Some(CodexSpeed::Fast);
        edited
            .env
            .insert("RUNNER_878_MARKER".into(), "after".into());
        edited.system_prompt = Some("EDITED_PERSONA".into());
        update_role_row(&conn, &edited);
        drop(conn);
        let fake = fake_runtime();
        let mgr = mgr_with_fake(None, fake.clone());
        mgr.resume(&id, None, None, home.path(), pool.clone(), capture())
            .unwrap();
        let spec = fake.last_spawn_spec().unwrap();
        assert_eq!(spec.command, "original-command");
        assert_eq!(
            spec.env.get("RUNNER_878_MARKER").map(String::as_str),
            Some("before")
        );
        assert!(spec.args.last().unwrap().contains("SLOT_BRIEF"));
        assert!(!spec.args.last().unwrap().contains("EDITED_PERSONA"));
        assert!(!spec.args.iter().any(|arg| arg.contains("edited-model")));
        mgr.restart(&id, None, None, home.path(), pool.clone(), capture())
            .unwrap();
        let spec = fake.last_spawn_spec().unwrap();
        assert_eq!(spec.command, "edited-command");
        assert_eq!(
            spec.env.get("RUNNER_878_MARKER").map(String::as_str),
            Some("after")
        );
        assert!(spec.args.last().unwrap().contains("EDITED_PERSONA"));
        let restarted = crate::repo::session::get_row(&pool.get().unwrap(), &id)
            .unwrap()
            .unwrap();
        edited.model = Some("slot-model".into());
        edited.effort = Some("xhigh".into());
        edited.codex_speed = Some(CodexSpeed::Standard);
        assert_eq!(
            restarted.launch_definition,
            Some(crate::session::launch_definition::from_effective(&edited))
        );
        edited.runtime = "pi".into();
        edited.codex_speed = None;
        update_role_row(&pool.get().unwrap(), &edited);
        mgr.restart(&id, None, None, home.path(), pool.clone(), capture())
            .unwrap();
        let restarted = crate::repo::session::get_row(&pool.get().unwrap(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(restarted.agent_runtime.as_deref(), Some("pi"));
        assert_eq!(restarted.launch_definition.unwrap().codex_speed, None);
        mgr.kill(&id).unwrap();
    }
}

#[test]
fn failed_runtime_changing_restart_keeps_definition_and_conversation_key_consistent() {
    let (pool, home, id) = slot_respawn_fixture("codex", false);
    let conn = pool.get().unwrap();
    let original = crate::repo::session::get_row(&conn, &id).unwrap().unwrap();
    conn.execute("UPDATE sessions SET cwd = '/fixture' WHERE id = ?1", [&id])
        .unwrap();
    let role_id = original.role_id.as_deref().unwrap();
    let mut edited = crate::ops::role::get(&conn, role_id).unwrap();
    edited.runtime = "claude-code".into();
    edited.command = "edited-command".into();
    edited.args = vec!["--debug".into()];
    edited.system_prompt = Some("EDITED_PERSONA".into());
    update_role_row(&conn, &edited);
    drop(conn);
    let expected = crate::session::launch_definition::from_effective(&edited);
    let fake = fake_runtime();
    *fake.spawn_failure_handle.lock().unwrap() = Some("slot".into());
    let mgr = mgr_with_fake(None, fake.clone());
    let error = mgr
        .restart(&id, None, None, home.path(), pool.clone(), capture())
        .unwrap_err();
    assert!(error.to_string().contains("injected spawn failure"));
    assert_eq!(fake.spawn_count(), 0);
    let failed = crate::repo::session::get_row(&pool.get().unwrap(), &id)
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, crate::model::SessionStatus::Stopped);
    assert_eq!(failed.agent_runtime.as_deref(), Some("claude-code"));
    assert_eq!(failed.launch_definition.as_ref(), Some(&expected));
    assert_ne!(failed.agent_session_key, original.agent_session_key);
    let key = failed.agent_session_key.as_deref().unwrap();
    uuid::Uuid::parse_str(key).unwrap();

    let transcripts = home.path().join(".claude/projects/-fixture");
    std::fs::create_dir_all(&transcripts).unwrap();
    std::fs::write(transcripts.join(format!("{key}.jsonl")), "{}\n").unwrap();
    *fake.spawn_failure_handle.lock().unwrap() = None;
    edited.runtime = "pi".into();
    edited.command = "later-command".into();
    edited.args = vec!["later-arg".into()];
    update_role_row(&pool.get().unwrap(), &edited);
    crate::runtimes::with_conversation_home(home.path(), || {
        mgr.resume(&id, None, None, home.path(), pool.clone(), capture())
    })
    .unwrap();
    let spec = fake.last_spawn_spec().unwrap();
    assert_eq!(spec.command, expected.command);
    assert!(spec.args.contains(&"--debug".into()));
    assert!(has_arg_pair(&spec.args, "--resume", key));
    assert!(!spec
        .args
        .contains(original.agent_session_key.as_ref().unwrap()));
    assert!(!spec.args.contains(&"later-arg".into()));
    let resumed = crate::repo::session::get_row(&pool.get().unwrap(), &id)
        .unwrap()
        .unwrap();
    assert_eq!(resumed.agent_runtime, failed.agent_runtime);
    assert_eq!(resumed.agent_session_key, failed.agent_session_key);
    assert_eq!(resumed.launch_definition, failed.launch_definition);
    mgr.kill(&id).unwrap();
}

#[test]
fn resume_without_a_definition_is_refused_by_name() {
    let pool = pool_with_schema();
    let mut row = crate::test_support::test_session_row(
        "missing-definition",
        crate::model::SessionStatus::Stopped,
    );
    row.agent_runtime = Some("codex".into());
    row.agent_command = Some("codex".into());
    crate::repo::session::insert(&pool.get().unwrap(), &row).unwrap();
    let fake = fake_runtime();
    let mgr = mgr_with_fake(None, fake.clone());
    let error = mgr
        .resume(&row.id, None, None, fixture_tmp_dir(), pool, capture())
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "session missing-definition has no launch definition"
    );
    assert!(fake.last_spawn_spec().is_none());
}
