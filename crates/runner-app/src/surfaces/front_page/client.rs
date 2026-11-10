use runner_core::companion::{ActionKind, ActionTarget};
use runner_core::protocol::{ClientError, ClientResult, DaemonClient};

#[derive(Clone)]
pub(crate) struct FrontPageClient {
    daemon: DaemonClient,
}

impl FrontPageClient {
    pub(crate) fn local(daemon: DaemonClient) -> Self {
        Self { daemon }
    }

    pub(crate) fn start_chat(&self, input: ChatStart) -> ClientResult<String> {
        let ChatStart {
            role,
            runtime,
            model,
            effort,
            speed,
            project,
            cwd,
            size,
        } = input;
        let scope = runner_core::protocol::project::ProjectScope::or_root(project);
        let speed = speed.map(|s| {
            if s == "fast" {
                runner_core::protocol::model::CodexSpeed::Fast
            } else {
                runner_core::protocol::model::CodexSpeed::Standard
            }
        });
        if let Some(role) = role {
            self.daemon
                .session_start_direct_with_speed(
                    role,
                    runtime,
                    model,
                    effort,
                    speed,
                    scope,
                    cwd,
                    Some(size.0),
                    Some(size.1),
                )
                .map(|s| s.id)
        } else {
            self.daemon
                .session_start_runtime_with_speed(
                    runtime.as_deref().unwrap_or_default(),
                    scope,
                    cwd,
                    Some(size.0),
                    Some(size.1),
                    model,
                    effort,
                    speed,
                )
                .map(|s| s.session.id)
        }
    }

    pub(crate) fn launch_chat(&self, launch: &mut ChatLaunch) -> ClientResult<ChatLaunched> {
        let session = if let Some(session) = &launch.started {
            session.clone()
        } else {
            let session = self.start_chat(launch.input.clone())?;
            launch.started = Some(session.clone());
            session
        };
        if let Some((layout, pane)) = &launch.placement {
            let mut layout = layout.clone();
            layout
                .assign_session(pane, &session)
                .map_err(|error| ClientError::msg(error.to_string()))?;
            self.launcher_layout(&layout, launch.input.project.clone())?;
        }
        Ok(ChatLaunched { session })
    }

    pub(crate) fn launcher_layout(
        &self,
        layout: &runner_app::pane_layout::PaneLayout,
        project: Option<String>,
    ) -> ClientResult<()> {
        let nodes = self.daemon.node_list()?;
        let existing = nodes.iter().find(|node| node.id == layout.id);
        let existing_project =
            existing.and_then(|node| crate::surfaces::sidebar::node_project_id(&nodes, node));
        let parent = if existing.is_some() && existing_project == project {
            existing.and_then(|node| node.parent_id.clone())
        } else {
            project
                .as_ref()
                .map(|project| {
                    nodes
                        .iter()
                        .find(|node| {
                            node.node_type == runner_core::protocol::node::NodeType::Project
                                && node.ref_id.as_ref() == Some(project)
                        })
                        .map(|node| node.id.clone())
                        .ok_or_else(|| {
                            ClientError::msg("The selected project is no longer available.")
                        })
                })
                .transpose()?
        };
        if let Some(existing) = existing.filter(|existing| existing.parent_id != parent) {
            let mut order = nodes
                .iter()
                .filter(|node| node.parent_id == parent && node.id != existing.id)
                .collect::<Vec<_>>();
            order.sort_by_key(|node| node.position);
            let mut order = order
                .into_iter()
                .map(|node| node.id.clone())
                .collect::<Vec<_>>();
            order.push(existing.id.clone());
            self.daemon
                .node_move(existing.id.clone(), parent.clone(), order)?;
        }
        let mut draft = layout.clone();
        draft.parent_id = parent;
        self.layout(&draft, project)
    }

    pub(crate) fn start_shell(
        &self,
        project: Option<String>,
        cwd: Option<String>,
        size: (u16, u16),
    ) -> ClientResult<String> {
        self.daemon
            .session_start_shell_in(
                runner_core::protocol::project::ProjectScope::or_root(project),
                cwd,
                Some(size.0),
                Some(size.1),
            )
            .map(|s| s.id)
    }

    pub(crate) fn start_role_chat(
        &self,
        role: String,
        cwd: Option<String>,
        size: (u16, u16),
    ) -> ClientResult<String> {
        self.daemon
            .session_start_direct(
                role,
                None,
                None,
                None,
                runner_core::protocol::project::ProjectScope::Root,
                cwd,
                Some(size.0),
                Some(size.1),
            )
            .map(|session| session.id)
    }

    pub(crate) fn layout(
        &self,
        layout: &runner_app::pane_layout::PaneLayout,
        _project: Option<String>,
    ) -> ClientResult<()> {
        let input = layout
            .upsert_input()
            .map_err(|e| ClientError::msg(e.to_string()))?;
        self.daemon.node_tab_upsert(input).map(drop)
    }

    pub(crate) fn stop(&self, session: &str) -> ClientResult<()> {
        self.daemon.session_kill(session)
    }

    pub(crate) fn resume(&self, session: &str, size: (u16, u16)) -> ClientResult<String> {
        self.resume_session(session, size, false)
    }

    pub(crate) fn resume_session(
        &self,
        session: &str,
        size: (u16, u16),
        launch_claim: bool,
    ) -> ClientResult<String> {
        if launch_claim {
            self.daemon
                .session_resume_on_launch(session, Some(size.0), Some(size.1))
        } else {
            self.daemon
                .session_resume(session, Some(size.0), Some(size.1))
        }
        .map(|session| session.id)
    }

    pub(crate) fn restart(&self, session: &str, size: (u16, u16)) -> ClientResult<String> {
        self.daemon
            .session_restart(session, Some(size.0), Some(size.1))
            .map(|s| s.id)
    }

    pub(crate) fn fork(
        &self,
        session: &str,
        title: Option<String>,
        size: (u16, u16),
    ) -> ClientResult<String> {
        self.daemon
            .session_fork(session, title, Some(size.0), Some(size.1))
            .map(|s| s.id)
    }

    pub(crate) fn close(&self, session: &str) -> ClientResult<()> {
        self.daemon.session_close(session)
    }

    pub(crate) fn rename(&self, target: ActionTarget, name: String) -> ClientResult<()> {
        self.rename_title(target, Some(name))
    }

    pub(crate) fn rename_title(
        &self,
        target: ActionTarget,
        name: Option<String>,
    ) -> ClientResult<()> {
        match target.kind {
            ActionKind::Session => self.daemon.session_rename(&target.id, name),
            ActionKind::Tab => self
                .daemon
                .node_rename(target.id, name.unwrap_or_default())
                .map(drop),
            ActionKind::Mission => self
                .daemon
                .mission_rename_impl(target.id, name.unwrap_or_default())
                .map(drop),
            ActionKind::Project => self
                .daemon
                .project_rename(target.id, name.unwrap_or_default())
                .map(drop),
        }
    }

    pub(crate) fn pin(
        &self,
        node: &super::model::SidebarNodeDisplay,
        pinned: bool,
    ) -> ClientResult<()> {
        self.daemon
            .node_set_pinned(node.id.clone(), pinned)
            .map(drop)
    }

    pub(crate) fn move_item(
        &self,
        node: &super::model::SidebarNodeDisplay,
        project: Option<&super::model::SidebarNodeDisplay>,
        order: &[super::model::SidebarNodeDisplay],
    ) -> ClientResult<Option<Vec<runner_core::protocol::node::NodeRow>>> {
        self.daemon
            .node_move(
                node.id.clone(),
                project.map(|project| project.id.clone()),
                order.iter().map(|node| node.id.clone()).collect(),
            )
            .map(Some)
    }

    pub(crate) fn reorder_pinned(
        &self,
        order: &[super::model::SidebarNodeDisplay],
    ) -> ClientResult<Option<Vec<runner_core::protocol::node::NodeRow>>> {
        self.daemon
            .node_reorder_pinned(order.iter().map(|node| node.id.clone()).collect())
            .map(Some)
    }

    pub(crate) fn create_project(&self, name: String, path: String) -> ClientResult<String> {
        self.daemon
            .project_create(name, path)
            .map(|project| project.id)
    }

    pub(crate) fn delete_project(&self, project: String, force: bool) -> ClientResult<()> {
        if force {
            self.daemon.project_delete(project).map(drop)
        } else {
            self.daemon.project_delete_checked(project, false).map(drop)
        }
    }

    pub(crate) fn stop_mission_state(&self, mission: String) -> ClientResult<MissionStop> {
        let state = self.daemon.mission_stop_impl(mission.clone())?;
        let sessions = self.daemon.session_list(&mission)?;
        Ok(MissionStop {
            mission: Some(state),
            sessions,
        })
    }

    pub(crate) fn rename_mission_state(
        &self,
        mission: String,
        title: String,
    ) -> ClientResult<runner_core::protocol::model::Mission> {
        self.daemon.mission_rename_impl(mission, title)
    }

    pub(crate) fn archive_mission_state(
        &self,
        mission: String,
    ) -> ClientResult<runner_core::protocol::model::Mission> {
        self.daemon.mission_archive_impl(mission)
    }

    pub(crate) fn pin_mission_state(
        &self,
        mission: String,
        pinned: bool,
    ) -> ClientResult<runner_core::protocol::model::Mission> {
        self.daemon.mission_pin_impl(mission, pinned)
    }

    #[cfg(test)]
    pub(crate) fn pin_chat(&self, session: &str, pinned: bool) -> ClientResult<()> {
        self.daemon.session_pin(session, pinned)
    }

    pub(crate) fn archive_chat(&self, session: &str, running: bool) -> ClientResult<()> {
        if running {
            let _ = self.daemon.session_kill(session);
        }
        self.daemon.session_archive(session)
    }

    pub(crate) fn archive_sessions(
        &self,
        plan: Vec<(String, ArchiveSessionOperation)>,
    ) -> (Vec<String>, Option<String>) {
        let mut removed = Vec::new();
        for (session, operation) in plan {
            let result = match operation {
                ArchiveSessionOperation::CloseTerminal => self.close(&session),
                ArchiveSessionOperation::ArchiveChat { running } => {
                    self.archive_chat(&session, running)
                }
            };
            if let Err(error) = result {
                return (removed, Some(error.to_string()));
            }
            removed.push(session);
        }
        (removed, None)
    }

    pub(crate) fn resume_mission_state(
        &self,
        mission: String,
        size: (u16, u16),
    ) -> ClientResult<MissionResume> {
        super::local::resume_mission(&self.daemon, &mission, size)
    }

    pub(crate) fn archive(&self, target: ActionTarget) -> ClientResult<()> {
        match target.kind {
            ActionKind::Session => self.daemon.session_archive_direct(&target.id),
            ActionKind::Tab => self.daemon.node_tab_delete(&target.id),
            ActionKind::Mission => self.daemon.mission_archive_impl(target.id).map(drop),
            ActionKind::Project => Err(ClientError::msg("Projects use deleteProject.")),
        }
    }

    pub(crate) fn start_mission(
        &self,
        crew: String,
        title: String,
        goal: Option<String>,
        project: Option<String>,
        cwd: Option<String>,
        size: (u16, u16),
    ) -> ClientResult<String> {
        self.daemon
            .mission_start_impl_with_size(
                runner_core::protocol::mission::MissionStart {
                    crew_id: (!crew.is_empty()).then_some(crew),
                    scope: runner_core::protocol::project::ProjectScope::or_root(project),
                    title,
                    goal_override: goal,
                    cwd,
                },
                Some(size),
            )
            .map(|output| output.mission.id)
    }

    pub(crate) fn answer(
        &self,
        mission: String,
        question: String,
        text: String,
    ) -> ClientResult<()> {
        self.daemon
            .mission_signal_impl(runner_core::protocol::mission::PostSignalInput {
                mission_id: mission,
                from: None,
                signal_type: "human_response".into(),
                payload: serde_json::json!({"question_id": question, "choice": text}),
            })
            .map(drop)
    }

    pub(crate) fn post(&self, mission: String, to: Vec<String>, text: String) -> ClientResult<()> {
        self.daemon
            .mission_post_impl(runner_core::protocol::mission::PostMessageInput {
                mission_id: mission,
                from: None,
                to: runner_core::model::Recipients::new(to),
                text,
            })
            .map(drop)
    }

    pub(crate) fn save_mission_presentation(
        &self,
        node: Option<&str>,
        layout: &runner_app::pane_layout::MissionLayout,
    ) -> ClientResult<()> {
        let node = node.ok_or_else(|| ClientError::msg("mission node is missing"))?;
        let layout = layout
            .serialize()
            .map_err(|error| ClientError::msg(error.to_string()))?;
        self.daemon.node_mission_layout_set(node, layout).map(drop)
    }
}

#[derive(Clone)]
pub(crate) struct ChatStart {
    pub(crate) role: Option<String>,
    pub(crate) runtime: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) speed: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) size: (u16, u16),
}

pub(crate) struct MissionResume {
    pub(crate) local_sessions: Vec<runner_core::protocol::session::SessionRow>,
    pub(crate) error: Option<String>,
}

pub(crate) fn is_concurrent_resume_error(error: &str) -> bool {
    [
        "is already being resumed",
        "is already running — attach instead",
    ]
    .iter()
    .any(|fragment| error.contains(fragment))
}

pub(crate) struct MissionStop {
    pub(crate) mission: Option<runner_core::protocol::model::Mission>,
    pub(crate) sessions: Vec<runner_core::protocol::session::SessionRow>,
}

pub(crate) struct ChatLaunch {
    pub(crate) input: ChatStart,
    pub(crate) placement: Option<(runner_app::pane_layout::PaneLayout, String)>,
    pub(crate) started: Option<String>,
}

pub(crate) struct ChatLaunched {
    pub(crate) session: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArchiveSessionOperation {
    ArchiveChat { running: bool },
    CloseTerminal,
}
