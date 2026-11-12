use super::manager::{resolve_runtime_override, runtime_direct_role};
use crate::error::{Error, Result};
use crate::model::{CodexSpeed, LaunchDefinition, Role, Runtime};

pub(crate) fn from_effective(role: &Role) -> LaunchDefinition {
    LaunchDefinition {
        command: role.command.clone(),
        args: role.args.clone(),
        env: role
            .env
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        model: role.model.clone(),
        effort: role.effort.clone(),
        codex_speed: role.codex_speed,
        system_prompt: role.system_prompt.clone(),
    }
}

pub(crate) fn to_role(definition: &LaunchDefinition, runtime: &str, mut identity: Role) -> Role {
    identity.runtime = runtime.to_owned();
    identity.command = definition.command.clone();
    identity.args = definition.args.clone();
    identity.env = definition
        .env
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    identity.model = definition.model.clone();
    identity.effort = definition.effort.clone();
    identity.codex_speed = definition.codex_speed;
    identity.system_prompt = definition.system_prompt.clone();
    identity
}

pub(crate) fn convert_legacy(
    role: Option<&Role>,
    runtime: Option<&str>,
    command: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
    speed: Option<CodexSpeed>,
) -> Result<(String, LaunchDefinition)> {
    let mut effective = if let Some(role) = role {
        resolve_runtime_override(role, runtime, model, effort)?.unwrap_or_else(|| role.clone())
    } else {
        let runtime =
            runtime.ok_or_else(|| Error::msg("runtime-only session missing agent_runtime"))?;
        let command = command.map(str::trim).filter(|command| !command.is_empty());
        let command = if Runtime::parse(runtime).is_some_and(Runtime::is_shell) {
            Some(command.ok_or_else(|| Error::msg("shell session missing agent_command"))?)
        } else {
            crate::runtimes::for_key(runtime)
                .catalog()
                .ok_or_else(|| Error::msg(format!("unknown runtime: {runtime}")))?;
            command.filter(|command| !std::path::Path::new(command).is_absolute())
        };
        runtime_direct_role(runtime, command, model, effort)?
    };
    if Runtime::parse(&effective.runtime).is_none() {
        return Err(Error::msg(format!(
            "unknown runtime: {}",
            effective.runtime
        )));
    }
    effective.codex_speed = speed.or(effective.codex_speed);
    Ok((effective.runtime.clone(), from_effective(&effective)))
}
