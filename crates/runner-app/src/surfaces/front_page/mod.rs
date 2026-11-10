pub(crate) mod actions;
pub(crate) mod client;
mod local;
pub(crate) mod model;
#[cfg(test)]
mod tests;

pub(crate) use client::FrontPageClient;

impl crate::NativeRoot {
    pub(crate) fn front_page_client(&self, cx: &gpui::App) -> FrontPageClient {
        FrontPageClient::local(self.core(cx).clone())
    }
}

impl crate::NativeRoot {
    pub(crate) fn front_page_layout(&self) -> Option<&runner_app::pane_layout::PaneLayout> {
        if self.route == crate::surfaces::AppRoute::Launcher {
            None
        } else {
            self.tabs.active()
        }
    }

    pub(crate) fn front_page_session(
        &self,
        id: &str,
        cx: &gpui::App,
    ) -> Option<model::SessionDisplay> {
        self.session_entry(id, cx).map(model::SessionDisplay::local)
    }

    pub(crate) fn front_page_status(
        &self,
        id: &str,
        cx: &gpui::App,
    ) -> Option<runner_app::ui::agent_status::StatusPresentation> {
        self.session_entry(id, cx).map(|entry| {
            super::sidebar::direct_chat_display_status(
                entry,
                self.app_store.read(cx).session_statuses.get(id),
            )
        })
    }
}

impl crate::NativeRoot {
    pub(crate) fn front_page_sidebar(&self) -> gpui::Entity<crate::surfaces::Sidebar> {
        self.sidebar.clone()
    }
}

impl crate::NativeRoot {
    pub(crate) fn front_page_has_islands(&self) -> bool {
        matches!(
            self.route,
            crate::surfaces::AppRoute::Chat
                | crate::surfaces::AppRoute::Launcher
                | crate::surfaces::AppRoute::Mission(_)
        )
    }

    pub(crate) fn front_page_is_chat(&self) -> bool {
        matches!(
            self.route,
            crate::surfaces::AppRoute::Chat | crate::surfaces::AppRoute::Launcher
        )
    }
}

impl crate::NativeRoot {
    pub(crate) fn front_page_pane_bounds(
        &self,
    ) -> &std::collections::HashMap<crate::surfaces::PaneKey, gpui::Size<gpui::Pixels>> {
        &self.pane_bounds
    }
    pub(crate) fn front_page_pane_bounds_mut(
        &mut self,
    ) -> &mut std::collections::HashMap<crate::surfaces::PaneKey, gpui::Size<gpui::Pixels>> {
        &mut self.pane_bounds
    }
    pub(crate) fn front_page_resizing_split(&self) -> Option<&str> {
        self.resizing_split.as_deref()
    }
}

impl crate::NativeRoot {
    pub(crate) fn front_page_sidebar_collapsed(&self) -> bool {
        self.sidebar_collapsed
    }
    pub(crate) fn front_page_sidebar_width(&self, cx: &gpui::App) -> f32 {
        self.settings(cx).sidebar_width
    }
}
