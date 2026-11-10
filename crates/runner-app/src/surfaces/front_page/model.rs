use runner_core::companion::CatalogRole;
use runner_core::protocol::{model::Role, session::DirectSessionEntry};

#[derive(Clone)]
pub(crate) struct SessionDisplay {
    pub(crate) session_id: String,
    pub(crate) agent_runtime: String,
    pub(crate) status: runner_core::protocol::model::SessionStatus,
    pub(crate) title: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) resumable: bool,
    pub(crate) native_fork: bool,
    pub(crate) forkable: bool,
    pub(crate) local: Option<DirectSessionEntry>,
    pub(crate) archived_at: Option<runner_core::protocol::model::Timestamp>,
    pub(crate) project_id: Option<String>,
}

impl SessionDisplay {
    pub(crate) fn local(entry: &DirectSessionEntry) -> Self {
        Self {
            session_id: entry.session_id.clone(),
            agent_runtime: entry.agent_runtime.clone(),
            status: entry.status,
            title: entry.title.clone(),
            cwd: entry.cwd.clone(),
            resumable: entry.resumable,
            native_fork: entry.native_fork,
            forkable: entry.forkable,
            local: Some(entry.clone()),
            archived_at: entry.archived_at,
            project_id: entry.project_id.clone(),
        }
    }

    pub(crate) fn label(&self, live: Option<&str>) -> String {
        if let Some(local) = &self.local {
            return crate::surfaces::sidebar::session_label_live(local, live);
        }
        self.title.clone().unwrap_or_else(|| self.default_label())
    }

    pub(crate) fn default_label(&self) -> String {
        if let Some(local) = &self.local {
            return crate::surfaces::sidebar::default_session_label(local);
        }
        if self.agent_runtime == "shell" {
            crate::i18n::text("noun.terminal").into()
        } else {
            crate::surfaces::roles::logic::runtime_display_name(&self.agent_runtime)
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct RoleChoice {
    pub(crate) id: String,
    pub(crate) handle: String,
    pub(crate) display_name: String,
    pub(crate) runtime: String,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) codex_speed: Option<runner_core::protocol::model::CodexSpeed>,
    pub(crate) working_dir: Option<String>,
}
impl From<Role> for RoleChoice {
    fn from(role: Role) -> Self {
        Self {
            id: role.id,
            handle: role.handle,
            display_name: role.display_name,
            runtime: role.runtime,
            model: role.model,
            effort: role.effort,
            codex_speed: role.codex_speed,
            working_dir: role.working_dir,
        }
    }
}
impl From<CatalogRole> for RoleChoice {
    fn from(role: CatalogRole) -> Self {
        Self {
            id: role.id,
            handle: role.handle,
            display_name: role.name,
            runtime: role.runtime,
            model: role.model,
            effort: role.effort,
            codex_speed: role.speed.as_deref().and_then(|s| match s {
                "standard" => Some(runner_core::protocol::model::CodexSpeed::Standard),
                "fast" => Some(runner_core::protocol::model::CodexSpeed::Fast),
                _ => None,
            }),
            working_dir: None,
        }
    }
}

#[derive(Clone)]
pub(crate) struct RuntimeChoice {
    pub(crate) name: runner_core::protocol::model::Runtime,
    pub(crate) display_name: String,
    pub(crate) command: Option<String>,
    pub(crate) available: bool,
    pub(crate) default_model: Option<String>,
    pub(crate) default_effort: Option<String>,
    pub(crate) models: Vec<runner_core::protocol::runtime::RuntimeCatalogOption>,
    pub(crate) efforts: Vec<runner_core::protocol::runtime::RuntimeCatalogOption>,
    pub(crate) speed: bool,
    pub(crate) effort_needs_launch_model: bool,
}
impl From<runner_core::protocol::runtime::RuntimeCatalogEntry> for RuntimeChoice {
    fn from(runtime: runner_core::protocol::runtime::RuntimeCatalogEntry) -> Self {
        Self {
            name: runtime.name,
            display_name: runtime.display_name,
            command: Some(runtime.command),
            available: runtime.available,
            default_model: runtime.default_model,
            default_effort: runtime.default_effort,
            models: runtime.models,
            efforts: runtime.efforts,
            speed: runtime.capabilities.codex_speed,
            effort_needs_launch_model: runtime.capabilities.effort_needs_launch_model,
        }
    }
}
impl RuntimeChoice {
    pub(crate) fn efforts_for_model(
        &self,
        model: &str,
    ) -> Vec<runner_core::protocol::runtime::RuntimeCatalogOption> {
        let model = model.trim();
        let supported = self
            .models
            .iter()
            .find(|option| option.value == model)
            .and_then(|option| option.supported_efforts.as_ref());
        self.efforts
            .iter()
            .filter(|effort| {
                effort.value.is_empty()
                    || (!model.is_empty()
                        && supported.is_none_or(|levels| levels.contains(&effort.value)))
            })
            .cloned()
            .collect()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SidebarNodeDisplay {
    pub(crate) id: String,
    pub(crate) parent_id: Option<String>,
    pub(crate) node_type: runner_core::protocol::node::NodeType,
    pub(crate) ref_id: Option<String>,
    pub(crate) position: i64,
    pub(crate) pinned_position: Option<i64>,
    pub(crate) created_at: Option<String>,
}
impl From<runner_core::protocol::node::NodeRow> for SidebarNodeDisplay {
    fn from(node: runner_core::protocol::node::NodeRow) -> Self {
        Self {
            id: node.id,
            parent_id: node.parent_id,
            node_type: node.node_type,
            ref_id: node.ref_id,
            position: node.position,
            pinned_position: node.pinned_position,
            created_at: Some(node.created_at),
        }
    }
}
#[derive(Clone)]
pub(crate) struct ProjectDisplay {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) cwd: String,
    pub(crate) local: Option<runner_core::protocol::project::ProjectRow>,
}
impl From<runner_core::protocol::project::ProjectRow> for ProjectDisplay {
    fn from(project: runner_core::protocol::project::ProjectRow) -> Self {
        Self {
            id: project.id.clone(),
            name: project.name.clone(),
            cwd: project.cwd.clone(),
            local: Some(project),
        }
    }
}
#[derive(Clone)]
pub(crate) struct MissionDisplay {
    pub(crate) mission: MissionFields,
    pub(crate) any_session_live: bool,
    pub(crate) session_statuses: Vec<(String, runner_core::protocol::AgentStatus)>,
}
#[derive(Clone)]
pub(crate) struct MissionFields {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) running: bool,
    pub(crate) project_id: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) archived_at: Option<runner_core::protocol::model::Timestamp>,
}
impl From<runner_core::protocol::mission::MissionSummary> for MissionDisplay {
    fn from(summary: runner_core::protocol::mission::MissionSummary) -> Self {
        let mission = summary.mission;
        Self {
            mission: MissionFields {
                id: mission.id,
                title: mission.title,
                running: mission.status == runner_core::protocol::model::MissionStatus::Running,
                project_id: mission.project_id,
                cwd: mission.cwd,
                archived_at: mission.archived_at,
            },
            any_session_live: summary.any_session_live,
            session_statuses: summary.session_statuses,
        }
    }
}

impl From<DirectSessionEntry> for SessionDisplay {
    fn from(entry: DirectSessionEntry) -> Self {
        Self::local(&entry)
    }
}

#[derive(Clone)]
pub(crate) struct WorkspaceMission {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) status: runner_core::protocol::model::MissionStatus,
    pub(crate) cwd: Option<String>,
    pub(crate) project_id: Option<String>,
    pub(crate) started_at: Option<runner_core::protocol::model::Timestamp>,
    pub(crate) archived: bool,
    pub(crate) pinned: bool,
}

impl From<&runner_core::protocol::model::Mission> for WorkspaceMission {
    fn from(mission: &runner_core::protocol::model::Mission) -> Self {
        Self {
            id: mission.id.clone(),
            title: mission.title.clone(),
            status: mission.status,
            cwd: mission.cwd.clone(),
            project_id: mission.project_id.clone(),
            started_at: Some(mission.started_at),
            archived: mission.archived_at.is_some(),
            pinned: mission.pinned_at.is_some(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct MissionMemberDisplay {
    pub(crate) session: MissionMemberSession,
    pub(crate) handle: String,
    pub(crate) runtime: String,
    pub(crate) live_title: Option<String>,
    pub(crate) agent_session_key: Option<String>,
}

#[derive(Clone)]
pub(crate) struct MissionMemberSession {
    pub(crate) id: String,
    pub(crate) status: runner_core::protocol::model::SessionStatus,
    pub(crate) cwd: Option<String>,
}

impl From<&runner_core::protocol::session::SessionRow> for MissionMemberDisplay {
    fn from(row: &runner_core::protocol::session::SessionRow) -> Self {
        Self {
            session: MissionMemberSession {
                id: row.session.id.clone(),
                status: row.session.status,
                cwd: row.session.cwd.clone(),
            },
            handle: row.handle.clone(),
            runtime: row.runtime.clone(),
            live_title: row.live_title.clone(),
            agent_session_key: row.agent_session_key.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MissionQuestionDisplay {
    pub(crate) id: String,
    pub(crate) markdown_id: String,
    pub(crate) asker: String,
    pub(crate) prompt: String,
    pub(crate) choices: Vec<String>,
    pub(crate) chain: String,
    pub(crate) time: Option<String>,
}
