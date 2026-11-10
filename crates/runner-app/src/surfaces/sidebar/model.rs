use super::*;
use crate::surfaces::front_page::model::ProjectDisplay;

impl Sidebar {
    pub(crate) fn active_only(&self, cx: &App) -> bool {
        self.settings(cx).sidebar_active_only
    }

    pub(crate) fn source_nodes(&self, cx: &App) -> Vec<NodeRow> {
        self.app_store
            .read(cx)
            .nodes
            .iter()
            .cloned()
            .map(Into::into)
            .collect()
    }

    pub(super) fn source_projects(&self, cx: &App) -> Vec<ProjectDisplay> {
        self.app_store
            .read(cx)
            .projects
            .iter()
            .cloned()
            .map(Into::into)
            .collect()
    }

    pub(super) fn session_status(
        &self,
        id: &str,
        cx: &App,
    ) -> Option<runner_core::protocol::AgentStatus> {
        self.app_store.read(cx).session_statuses.get(id).cloned()
    }
}

impl Sidebar {
    pub(super) fn tab_selected(&self, tab: &str, cx: &App) -> bool {
        self.shell.upgrade().is_some_and(|shell| {
            let shell = shell.read(cx);
            super::elements::chat_tab_row_active(&shell.route, shell.tabs.active_tab_id(), tab)
        })
    }
    pub(super) fn mission_selected(&self, mission: &str, cx: &App) -> bool {
        self.shell.upgrade().is_some_and(
            |shell| matches!(&shell.read(cx).route, AppRoute::Mission(id) if id == mission),
        )
    }
}
