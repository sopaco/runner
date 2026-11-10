use super::client::ChatStart;
use super::*;
use runner_app::pane_layout::PaneLayout;
use runner_core::companion::{ActionKind, ActionTarget};
use runner_core::protocol::{
    ClientError, DaemonClient, EventSubscription, Request, Response, Transport,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Fake {
    requests: Mutex<Vec<(String, Value)>>,
    local: Mutex<Vec<&'static str>>,
}

impl Transport for Fake {
    fn call(&self, request: Request) -> Result<Response, ClientError> {
        Ok(match request {
            Request::remote_request { computer, request } => {
                let value = serde_json::to_value(&request).unwrap();
                self.requests
                    .lock()
                    .unwrap()
                    .push((computer, value.clone()));
                let result = match value["type"].as_str().unwrap() {
                    "overview" => json!({"deliberately": "not an overview"}),
                    "startChat" | "startShell" | "resume" | "restart" | "fork" => {
                        json!({"session": "same-id"})
                    }
                    "resumeMission" => {
                        json!({"mission":value["mission"],"sessions":["resumed-member"]})
                    }
                    "layout" => json!({"target": value["target"]}),
                    "createProject" => json!({"project":"host-project"}),
                    "catalog" => {
                        json!({"roles":[],"runtimes":[],"accounts":[],"activeAccount":"host-account","crews":[],"projects":[],"recentFolders":[]})
                    }
                    "folders" => {
                        json!({"path": value["path"].as_str().unwrap_or("C:\\Users\\Jason"),"parent":null,"folders":[]})
                    }
                    _ => Value::Null,
                };
                Response::remote_request(Ok(runner_core::remote::RemoteResult {
                    result,
                    features: [
                        "catalog",
                        "folders",
                        "startChat",
                        "startShell",
                        "stop",
                        "resume",
                        "restart",
                        "fork",
                        "close",
                        "rename",
                        "archive",
                        "layout",
                        "pin",
                        "move",
                        "reorderPinned",
                        "createProject",
                        "deleteProject",
                        "stopMission",
                        "resumeMission",
                    ]
                    .map(str::to_owned)
                    .to_vec(),
                }))
            }
            Request::session_kill { .. } => {
                self.local.lock().unwrap().push("stop");
                Response::session_kill(Ok(()))
            }
            Request::session_close { .. } => {
                self.local.lock().unwrap().push("close");
                Response::session_close(Ok(()))
            }
            Request::session_rename { .. } => {
                self.local.lock().unwrap().push("rename");
                Response::session_rename(Ok(()))
            }
            Request::session_archive_direct { .. } => {
                self.local.lock().unwrap().push("archive");
                Response::session_archive_direct(Ok(()))
            }
            _ => panic!("Unexpected local request"),
        })
    }

    fn subscribe(&self) -> Box<dyn EventSubscription> {
        panic!("No subscriptions in dispatch tests")
    }
}

fn session() -> ActionTarget {
    ActionTarget {
        kind: ActionKind::Session,
        id: "same-id".into(),
    }
}

#[test]
fn local_actions_keep_the_existing_daemon_calls_and_order() {
    let fake = Arc::new(Fake::default());
    let client = FrontPageClient::local(DaemonClient::new(fake.clone()));
    client.stop("same-id").unwrap();
    client.rename(session(), "name".into()).unwrap();
    client.archive(session()).unwrap();
    client.close("shell").unwrap();
    assert_eq!(
        *fake.local.lock().unwrap(),
        ["stop", "rename", "archive", "close"]
    );
    assert!(fake.requests.lock().unwrap().is_empty());
}

#[derive(Default)]
struct LocalDispatch {
    calls: Mutex<Vec<Request>>,
}

impl Transport for LocalDispatch {
    fn call(&self, request: Request) -> Result<Response, ClientError> {
        self.calls.lock().unwrap().push(request);
        Err(ClientError::msg("unchanged daemon error"))
    }

    fn subscribe(&self) -> Box<dyn EventSubscription> {
        panic!("dispatch does not subscribe")
    }
}

#[test]
fn local_workspace_mutations_keep_daemon_requests_arguments_and_errors() {
    use runner_core::protocol::{mission::MissionStart, model::CodexSpeed, project::ProjectScope};
    let fake = Arc::new(LocalDispatch::default());
    let client = FrontPageClient::local(DaemonClient::new(fake.clone()));
    macro_rules! dispatch {
        ($operation:expr, $request:expr) => {{
            assert_eq!(
                $operation.err().unwrap().to_string(),
                "unchanged daemon error"
            );
            let calls = std::mem::take(&mut *fake.calls.lock().unwrap());
            assert_eq!(calls.len(), 1);
            assert_eq!(
                serde_json::to_value(&calls[0]).unwrap(),
                serde_json::to_value($request).unwrap()
            );
        }};
    }
    let scope = ProjectScope::or_root(Some("project".into()));
    let input = ChatStart {
        role: Some("role".into()),
        runtime: Some("codex".into()),
        model: Some("model".into()),
        effort: Some("low".into()),
        speed: Some("fast".into()),
        project: Some("project".into()),
        cwd: Some("/work".into()),
        size: (101, 37),
    };
    dispatch!(
        client.start_chat(input.clone()),
        Request::session_start_direct_with_speed {
            role_id: "role".into(),
            runtime: Some("codex".into()),
            model: Some("model".into()),
            effort: Some("low".into()),
            speed: Some(CodexSpeed::Fast),
            scope: scope.clone(),
            cwd: Some("/work".into()),
            cols: Some(101),
            rows: Some(37),
        }
    );
    dispatch!(
        client.start_chat(ChatStart {
            role: None,
            speed: None,
            project: None,
            ..input
        }),
        Request::session_start_runtime_with_speed {
            runtime: "codex".into(),
            model: Some("model".into()),
            effort: Some("low".into()),
            speed: None,
            scope: ProjectScope::Root,
            cwd: Some("/work".into()),
            cols: Some(101),
            rows: Some(37),
        }
    );
    dispatch!(
        client.start_shell(Some("project".into()), Some("/work".into()), (99, 31)),
        Request::session_start_shell_in {
            scope: scope.clone(),
            cwd: Some("/work".into()),
            cols: Some(99),
            rows: Some(31),
        }
    );
    dispatch!(
        client.resume("chat", (83, 29)),
        Request::session_resume {
            session_id: "chat".into(),
            cols: Some(83),
            rows: Some(29)
        }
    );
    dispatch!(
        client.restart("chat", (84, 30)),
        Request::session_restart {
            session_id: "chat".into(),
            cols: Some(84),
            rows: Some(30)
        }
    );
    dispatch!(
        client.resume_session("shell", (85, 31), true),
        Request::session_resume_on_launch {
            session_id: "shell".into(),
            cols: Some(85),
            rows: Some(31)
        }
    );
    dispatch!(
        client.fork("chat", Some("Fork title".into()), (86, 32)),
        Request::session_fork {
            source_session_id: "chat".into(),
            title: Some("Fork title".into()),
            cols: Some(86),
            rows: Some(32)
        }
    );
    dispatch!(
        client.rename_title(
            ActionTarget {
                kind: ActionKind::Session,
                id: "chat".into()
            },
            None
        ),
        Request::session_rename {
            session_id: "chat".into(),
            title: None
        }
    );
    dispatch!(
        client.pin_chat("chat", true),
        Request::session_pin {
            session_id: "chat".into(),
            pinned: true
        }
    );
    dispatch!(
        client.archive_chat("chat", false),
        Request::session_archive {
            session_id: "chat".into()
        }
    );
    dispatch!(
        client.start_mission(
            "".into(),
            "Mission".into(),
            Some("Goal".into()),
            Some("project".into()),
            Some("/work".into()),
            (91, 33)
        ),
        Request::mission_start_impl_with_size {
            input: MissionStart {
                crew_id: None,
                scope,
                title: "Mission".into(),
                goal_override: Some("Goal".into()),
                cwd: Some("/work".into())
            },
            initial_size: Some((91, 33)),
        }
    );
    dispatch!(
        client.stop_mission_state("mission".into()).map(drop),
        Request::mission_stop_impl {
            id: "mission".into()
        }
    );
    dispatch!(
        client.pin_mission_state("mission".into(), true),
        Request::mission_pin_impl {
            id: "mission".into(),
            pinned: true
        }
    );
    dispatch!(
        client.rename_mission_state("mission".into(), "Title".into()),
        Request::mission_rename_impl {
            id: "mission".into(),
            title: "Title".into()
        }
    );
    dispatch!(
        client.archive_mission_state("mission".into()),
        Request::mission_archive_impl {
            id: "mission".into()
        }
    );
    dispatch!(
        client.post(
            "mission".into(),
            vec!["coder".into(), "reviewer".into()],
            "Text".into()
        ),
        Request::mission_post_impl {
            input: runner_core::protocol::mission::PostMessageInput {
                mission_id: "mission".into(),
                from: None,
                text: "Text".into(),
                to: runner_core::model::Recipients::new(vec!["coder".into(), "reviewer".into()])
            },
        }
    );
    dispatch!(
        client.answer("mission".into(), "question".into(), "yes".into()),
        Request::mission_signal_impl {
            input: runner_core::protocol::mission::PostSignalInput {
                mission_id: "mission".into(),
                from: None,
                signal_type: "human_response".into(),
                payload: json!({"question_id":"question","choice":"yes"})
            },
        }
    );
    let layout = runner_app::pane_layout::MissionLayout::default();
    dispatch!(
        client.save_mission_presentation(Some("node"), &layout),
        Request::node_mission_layout_set {
            node_id: "node".into(),
            layout: layout.serialize().unwrap()
        }
    );
    dispatch!(
        client.start_role_chat("role".into(), Some("/role-work".into()), (80, 24)),
        Request::session_start_direct {
            role_id: "role".into(),
            runtime: None,
            model: None,
            effort: None,
            scope: ProjectScope::Root,
            cwd: Some("/role-work".into()),
            cols: Some(80),
            rows: Some(24)
        }
    );
    let node = |id: &str, kind, reference: Option<&str>| model::SidebarNodeDisplay {
        id: id.into(),
        parent_id: None,
        node_type: kind,
        ref_id: reference.map(str::to_owned),
        position: 0,
        pinned_position: None,
        created_at: None,
    };
    let tab = node("tab-node", runner_core::protocol::node::NodeType::Tab, None);
    let project = node(
        "project-node",
        runner_core::protocol::node::NodeType::Project,
        Some("project"),
    );
    dispatch!(
        client.pin(&tab, true),
        Request::node_set_pinned {
            id: "tab-node".into(),
            pinned: true
        }
    );
    dispatch!(
        client.move_item(&tab, Some(&project), std::slice::from_ref(&tab)),
        Request::node_move {
            id: "tab-node".into(),
            parent_id: Some("project-node".into()),
            ordered_ids: vec!["tab-node".into()]
        }
    );
    dispatch!(
        client.reorder_pinned(&[tab]),
        Request::node_reorder_pinned {
            ordered_ids: vec!["tab-node".into()]
        }
    );
    let layout = PaneLayout::single(Some("chat"), &[]);
    dispatch!(
        client.layout(&layout, Some("project".into())),
        Request::node_tab_upsert {
            input: layout.upsert_input().unwrap()
        }
    );
    dispatch!(
        client.rename(
            ActionTarget {
                kind: ActionKind::Tab,
                id: "tab-node".into()
            },
            "Tab".into()
        ),
        Request::node_rename {
            id: "tab-node".into(),
            name: "Tab".into()
        }
    );
    dispatch!(
        client.rename(
            ActionTarget {
                kind: ActionKind::Project,
                id: "project".into()
            },
            "Project".into()
        ),
        Request::project_rename {
            id: "project".into(),
            name: "Project".into()
        }
    );
    dispatch!(
        client.archive(ActionTarget {
            kind: ActionKind::Tab,
            id: "tab-node".into()
        }),
        Request::node_tab_delete {
            node_id: "tab-node".into()
        }
    );
    dispatch!(
        client.create_project("Project".into(), "/work".into()),
        Request::project_create {
            name: "Project".into(),
            cwd: "/work".into()
        }
    );
    dispatch!(
        client.delete_project("project".into(), true),
        Request::project_delete {
            id: "project".into()
        }
    );
    dispatch!(
        client.delete_project("project".into(), false),
        Request::project_delete_checked {
            id: "project".into(),
            force: false
        }
    );
    assert!(fake.calls.lock().unwrap().is_empty());
}

struct ScriptedLocal {
    steps: Mutex<std::collections::VecDeque<(Request, Response)>>,
}

impl Transport for ScriptedLocal {
    fn call(&self, request: Request) -> Result<Response, ClientError> {
        let (expected, response) = self
            .steps
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected daemon request");
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        Ok(response)
    }
    fn subscribe(&self) -> Box<dyn EventSubscription> {
        panic!("no dispatch subscription")
    }
}

#[test]
fn local_mission_resume_preserves_concurrent_refresh_partial_failure_and_dimensions() {
    use runner_core::protocol::{
        model::{Session, SessionStatus},
        session::{SessionRow, SpawnedSession},
    };
    let member = |id: &str, status| SessionRow {
        session: Session {
            id: id.into(),
            mission_id: Some("mission".into()),
            role_id: "role".into(),
            slot_id: None,
            cwd: None,
            status,
            pid: None,
            started_at: None,
            stopped_at: None,
        },
        live_title: None,
        handle: id.into(),
        role_handle: "role".into(),
        runtime: "codex".into(),
        lead: false,
        agent_session_key: None,
    };
    let list = || Request::session_list {
        mission_id: "mission".into(),
    };
    let resume = |id: &str| Request::session_resume {
        session_id: id.into(),
        cols: Some(91),
        rows: Some(33),
    };
    let initial = vec![
        member("running", SessionStatus::Running),
        member("concurrent", SessionStatus::Stopped),
        member("failed", SessionStatus::Stopped),
        member("last", SessionStatus::Stopped),
    ];
    let refreshed = vec![
        member("running", SessionStatus::Running),
        member("concurrent", SessionStatus::Running),
        member("failed", SessionStatus::Stopped),
        member("last", SessionStatus::Stopped),
    ];
    let final_rows = vec![
        member("running", SessionStatus::Running),
        member("concurrent", SessionStatus::Running),
        member("failed", SessionStatus::Stopped),
        member("last", SessionStatus::Running),
    ];
    let fake = Arc::new(ScriptedLocal {
        steps: Mutex::new(
            [
                (list(), Response::session_list(Ok(initial))),
                (
                    resume("concurrent"),
                    Response::session_resume(Err(ClientError::msg(
                        "session is already being resumed",
                    ))),
                ),
                (list(), Response::session_list(Ok(refreshed))),
                (
                    resume("failed"),
                    Response::session_resume(Err(ClientError::msg("original resume failure"))),
                ),
                (
                    resume("last"),
                    Response::session_resume(Ok(SpawnedSession {
                        id: "last".into(),
                        mission_id: Some("mission".into()),
                        role_id: Some("role".into()),
                        handle: "last".into(),
                        pid: Some(12),
                    })),
                ),
                (list(), Response::session_list(Ok(final_rows.clone()))),
            ]
            .into(),
        ),
    });
    let client = FrontPageClient::local(DaemonClient::new(fake.clone()));
    let result = client
        .resume_mission_state("mission".into(), (91, 33))
        .unwrap();
    assert_eq!(result.error.as_deref(), Some("original resume failure"));
    assert_eq!(
        serde_json::to_value(result.local_sessions).unwrap(),
        serde_json::to_value(final_rows).unwrap()
    );
    assert!(fake.steps.lock().unwrap().is_empty());
}

struct EntryTrace {
    base: runner_daemon::daemon::InProcessTransport,
    calls: Mutex<Vec<Value>>,
}

impl Transport for EntryTrace {
    fn call(&self, request: Request) -> Result<Response, ClientError> {
        let value = serde_json::to_value(&request).unwrap();
        self.calls.lock().unwrap().push(value.clone());
        if [
            "session_kill",
            "session_resume",
            "session_fork",
            "session_archive",
        ]
        .iter()
        .any(|method| value.get(method).is_some())
        {
            return Err(ClientError::msg("entry dispatch failure"));
        }
        self.base.call(request)
    }
    fn subscribe(&self) -> Box<dyn EventSubscription> {
        self.base.subscribe()
    }
}

#[test]
fn chat_lifecycle_entry_points_dispatch_once_for_local_targets() {
    use crate::surfaces::root::{GlobalNativePaths, NativeRoot};
    use crate::{AppRoute, ForkConfirm, NativePaths};
    use gpui::{px, size};
    let _theme = crate::theme_snapshot::ThemeGuard::new();
    let temp = tempfile::tempdir().unwrap();
    let mut cx = gpui::TestAppContext::single();
    let store = crate::app_store::test_lifecycle_store(&mut cx, temp.path());
    cx.update(|cx| {
        cx.set_global(GlobalNativePaths(NativePaths::new(
            temp.path().into(),
            temp.path().join("logs"),
        )));
    });
    let core = store.read_with(&cx, |store, _| store.test_core.clone());
    let trace = Arc::new(EntryTrace {
        base: runner_daemon::daemon::InProcessTransport(core.clone()),
        calls: Mutex::new(Vec::new()),
    });
    store.update(&mut cx, |store, _| {
        store.client = DaemonClient::new(trace.clone())
    });
    let window = cx.add_window(|window, cx| {
        NativeRoot::new(
            "workspace-entry-points".into(),
            temp.path().join("logs"),
            None,
            None,
            store.clone(),
            window,
            cx,
        )
    });
    cx.simulate_window_resize(window.into(), size(px(1440.), px(900.)));
    window
        .update(&mut cx, |root, _window, cx| {
            let conn = root.app_store.read(cx).test_core.db.get().unwrap();
            conn.execute("INSERT INTO sessions(id, status, agent_runtime, agent_command, agent_session_key) VALUES ('local-chat', 'stopped', 'codex', 'codex', 'conversation')", []).unwrap();
            drop(conn);
            let mut layout = PaneLayout::single(Some("local-chat"), &[]);
            layout.id = "01ARZ3NDEKTSV4RRFFQ69G5FAV".into();
            root.core(cx)
                .node_tab_upsert(layout.upsert_input().unwrap())
                .unwrap();
            root.refresh_sessions(cx);
            root.reload_tabs(cx).unwrap();
            root.tabs.activate(&layout.id);
            root.route = AppRoute::Chat;
        })
        .unwrap();
    cx.run_until_parked();
    let session_id = "local-chat";
    for (command, local_method) in [
        ("stop", "session_kill"),
        ("resume", "session_resume"),
        ("fork", "session_fork"),
        ("archive", "session_archive"),
    ] {
        trace.calls.lock().unwrap().clear();
        window
            .update(&mut cx, |root, window, cx| match command {
                "stop" => root.stop_chat(session_id, window, cx),
                "resume" => root.resume_chat("p1", session_id, window, cx),
                "archive" => {
                    let close = crate::PendingPaneClose {
                        tab_id: root.front_page_layout().unwrap().id.clone(),
                        pane_id: "p1".into(),
                    };
                    crate::TerminalCloseConfirm::open(
                        &mut root.terminal_close_confirm,
                        crate::TerminalCloseTarget::ArchiveChatPane {
                            session_id: session_id.into(),
                            close,
                        },
                        window,
                        cx,
                    );
                    root.confirm_terminal_close(window, cx);
                }
                _ => {
                    root.fork_confirm = Some(ForkConfirm {
                        session_id: session_id.into(),
                        pending: false,
                    });
                    root.confirm_fork_chat(window, cx);
                }
            })
            .unwrap();
        cx.run_until_parked();
        let calls = trace.calls.lock().unwrap();
        let requests = calls
            .iter()
            .filter(|value| value.get(local_method).is_some())
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1, "{command}: {calls:?}");
        let request = &requests[0][local_method];
        assert_eq!(
            request[if command == "fork" {
                "source_session_id"
            } else {
                "session_id"
            }]
            .clone(),
            session_id
        );
        window
            .read_with(&cx, |root, _| {
                let error = root
                    .chat_error
                    .as_ref()
                    .map(|error| error.resolve().to_string());
                assert_eq!(
                    error.as_deref(),
                    Some("entry dispatch failure"),
                    "{command}"
                );
            })
            .unwrap();
    }
}
