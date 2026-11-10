use crate::*;
use runner_core::companion::{ActionKind, ActionTarget};

pub(crate) struct ShellStart {
    pub(crate) project: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) size: (u16, u16),
}

pub(crate) struct ArchiveOperation {
    pub(crate) removed: Vec<String>,
    pub(crate) error: Option<String>,
}

impl NativeRoot {
    pub(crate) fn save_workspace_layout(
        &mut self,
        draft: PaneLayout,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if self.launcher_tab_pending(&draft.id, cx) {
            anyhow::bail!("A start is pending in this tab.");
        }
        let client = self.front_page_client(cx);
        client
            .layout(&draft, draft.parent_id.clone())
            .map_err(|error| anyhow::Error::msg(error.to_string()))
    }

    pub(crate) fn launch_workspace_shell(
        &mut self,
        input: ShellStart,
        window: &mut Window,
        cx: &mut Context<Self>,
        finish_local: impl FnOnce(&mut Self, Result<String>, &mut Window, &mut Context<Self>),
    ) {
        let client = self.front_page_client(cx);
        let result = client
            .start_shell(input.project, input.cwd, input.size)
            .map_err(anyhow::Error::msg);
        finish_local(self, result, window, cx);
    }
}

impl NativeRoot {
    pub(crate) fn submit_workspace_rename(
        &mut self,
        pane: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (input, target, original) = if pane {
            let Some(rename) = &self.pane_rename else {
                return;
            };
            (
                rename.input.clone(),
                ActionTarget {
                    kind: ActionKind::Session,
                    id: rename.session_id.clone(),
                },
                rename.original.clone(),
            )
        } else {
            let Some(modal) = &self.chat_rename_modal else {
                return;
            };
            if modal.submitting {
                return;
            }
            let (target, original) = match &modal.target {
                ChatRenameTarget::Session {
                    session_id,
                    original,
                } => (
                    ActionTarget {
                        kind: ActionKind::Session,
                        id: session_id.clone(),
                    },
                    original.clone(),
                ),
            };
            (modal.input.clone(), target, original)
        };
        if input.read(cx).is_composing() {
            return;
        }
        let client = self.front_page_client(cx);
        let name = input.read(cx).text().trim().to_owned();
        let (unchanged, title) = if pane {
            match crate::surfaces::chat::pane_rename_change(&original, &name) {
                crate::surfaces::chat::PaneRenameChange::Unchanged => (true, None),
                crate::surfaces::chat::PaneRenameChange::Persist(title) => (false, title),
            }
        } else {
            (name == original.trim(), Some(name))
        };
        if unchanged && !pane {
            self.close_chat_rename(window, cx);
            return;
        }
        if let Some(modal) = self
            .chat_rename_modal
            .as_mut()
            .filter(|modal| modal.input == input)
        {
            modal.submitting = true;
            modal.error = None;
        }
        let operation = move || {
            if unchanged {
                Ok(())
            } else {
                client
                    .rename_title(target, title)
                    .map_err(|error| error.to_string())
            }
        };
        if pane {
            let result = operation();
            self.finish_workspace_rename(pane, input, result, window, cx);
        } else {
            let task = cx.background_spawn(async move { operation() });
            cx.spawn_in(window, async move |weak, cx| {
                let result = task.await;
                let _ = weak.update_in(cx, |root, window, cx| {
                    root.finish_workspace_rename(pane, input, result, window, cx);
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn finish_workspace_rename(
        &mut self,
        pane: bool,
        input: Entity<runner_app::ui::TextField>,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => {
                if pane
                    && self
                        .pane_rename
                        .as_ref()
                        .is_some_and(|rename| rename.input == input)
                {
                    self.pane_rename = None;
                    self._pane_rename_focus_subscription = None;
                }
                if !pane
                    && self
                        .chat_rename_modal
                        .as_ref()
                        .is_some_and(|modal| modal.input == input)
                {
                    self.chat_rename_modal = None;
                }
                self.refresh_sessions(cx);
                if let Err(error) = self.reload_tabs(cx) {
                    self.chat_error = Some(error.to_string().into());
                }
                self.sync_active_chat_detail(cx);
                self.focus_active_terminal(window, cx);
            }
            Err(error) => {
                if let Some(modal) = self
                    .chat_rename_modal
                    .as_mut()
                    .filter(|modal| !pane && modal.input == input)
                {
                    modal.submitting = false;
                    modal.error = Some(error);
                } else {
                    self.chat_error = Some(error.into());
                    if let Some(rename) = &self.pane_rename {
                        rename.input.read(cx).focus_handle().focus(window, cx);
                    }
                }
            }
        }
        cx.notify();
    }
}
