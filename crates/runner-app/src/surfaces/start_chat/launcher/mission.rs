use super::*;
use runner_core::protocol::crew::CrewListItem;
use runner_core::protocol::model::SlotWithRole;

pub(super) const SHIFT_ENTER: &str = "⇧↵";

/// The view to return to when leaving the launcher page.
#[derive(Clone)]
pub(crate) enum LauncherOrigin {
    Local {
        route: AppRoute,
        tab: Option<String>,
    },
}

struct MissionRequest {
    crew: String,
    title: String,
    goal: Option<String>,
    project: Option<String>,
    cwd: Option<String>,
}

pub(super) fn local_crews(crews: &[CrewListItem]) -> Vec<CrewEntry> {
    crews
        .iter()
        .map(|item| CrewEntry {
            id: item.crew.id.clone(),
            name: item.crew.name.clone(),
            handles: item
                .members
                .iter()
                .map(|member| member.slot_handle.clone())
                .collect(),
            roles: usize::try_from(item.role_count).unwrap_or_default(),
            slots: None,
        })
        .collect()
}

fn slot_line(member: &SlotWithRole, runtimes: &[RuntimeChoice]) -> SlotLine {
    let setup = crate::surfaces::crews::slot_setup(member);
    SlotLine {
        handle: member.slot.slot_handle.clone(),
        role: member.role.display_name.clone(),
        setup: setup_text(
            setup
                .model
                .or_else(|| Some(runtime_display_name(runtimes, &setup.runtime))),
            setup.effort,
        ),
    }
}

fn setup_text(model: Option<String>, effort: Option<String>) -> String {
    [model, effort]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn crew_row(crew: &CrewEntry, label: Option<&str>) -> Row {
    let members = if crew.roles == 0 {
        crate::i18n::text("ui.no_roles_yet").to_owned()
    } else {
        crew.handles.join(", ")
    };
    Row {
        choice: Choice::Crew(crew.id.clone()),
        name: crew.name.clone(),
        detail: match label {
            Some(label) => format!("{label} · {members}"),
            None => members,
        },
        runtime: None,
        avatar: None,
        crew: Some(crew.handles.iter().cloned().map(Into::into).collect()),
        unavailable: false,
        dimmed: crew.roles == 0,
        used_at: None,
        previously_used: false,
    }
}

impl Row {
    pub(super) fn pinned(choice: Choice, name: &str, detail: &str) -> Self {
        Self {
            choice,
            name: name.into(),
            detail: detail.into(),
            runtime: None,
            avatar: None,
            crew: None,
            unavailable: false,
            dimmed: false,
            used_at: None,
            previously_used: false,
        }
    }
}

pub(crate) fn record_launcher_crew(directory: &Path, crew: String) {
    let _writer = RECENT_WRITER.lock().unwrap();
    let mut store = RecentStore::load(directory);
    store.record_crew(computer_key(), crew, chrono::Utc::now().timestamp());
    match store.save(directory) {
        Ok(()) => {
            RECENT_REVISION.fetch_add(1, std::sync::atomic::Ordering::Release);
        }
        Err(error) => tracing::warn!(%error, "could not save launcher history"),
    }
}

impl ChatLauncher {
    /// Whether this launcher starts missions: only a tab's own page can,
    /// since a mission takes the whole tab.
    pub(super) fn offers_missions(&self) -> bool {
        self.default_page
    }

    pub(super) fn offers_empty_missions(&self) -> bool {
        self.offers_missions()
    }

    pub(super) fn crew_rows(&self, cx: &App) -> Vec<Row> {
        let recent = self.recents.crews(computer_key());
        let mut rows = model::ranked_crews(&self.crews, recent, self.query.read(cx).text())
            .into_iter()
            .map(|crew| crew_row(crew, None))
            .collect::<Vec<_>>();
        if self.offers_empty_missions() {
            rows.push(Row::pinned(
                Choice::EmptyMission,
                crate::i18n::text("launcher.empty_mission"),
                crate::i18n::text("launcher.empty_mission_detail"),
            ));
        }
        rows
    }

    pub(super) fn typed_crew_rows(&self, query: &str, _cx: &App) -> Vec<Row> {
        let recent = self.recents.crews(computer_key());
        model::ranked_crews(&self.crews, recent, query)
            .into_iter()
            .map(|crew| crew_row(crew, Some(crate::i18n::text("launcher.crew"))))
            .collect()
    }

    pub(crate) fn open_step(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending.is_some() {
            return;
        }
        self.selected = 0;
        self.error = None;
        self.slots = None;
        if !matches!(step, Step::Goal(_)) {
            let placeholder = if step == Step::Crews {
                "launcher.search_crews"
            } else {
                "launcher.search"
            };
            self.query.update(cx, |query, cx| {
                query.reset("", cx);
                query.set_placeholder(crate::i18n::message(placeholder), cx);
            });
        }
        self.step = step;
        self.settle_goal_crew(cx);
        self.focus(window, cx);
        cx.notify();
    }

    /// Shows the goal step's crew once its list is known: its slots, or the
    /// crew step when it has gone or has no roles.
    pub(super) fn settle_goal_crew(&mut self, cx: &mut Context<Self>) {
        let Step::Goal(Some(id)) = &self.step else {
            return;
        };
        if self.slots.is_some() {
            return;
        }
        match self.crews.iter().find(|crew| &crew.id == id) {
            Some(crew) if crew.roles > 0 => match crew.slots.clone() {
                Some(slots) => self.slots = Some(slots),
                None => self.load_slots(id.clone(), cx),
            },
            _ => {
                self.step = Step::Crews;
                self.error = Some(crate::i18n::text("launcher.crew_unavailable").into());
                self.query.update(cx, |query, cx| {
                    query.set_placeholder(crate::i18n::message("launcher.search_crews"), cx)
                });
            }
        }
        cx.notify();
    }

    fn load_slots(&mut self, id: String, cx: &mut Context<Self>) {
        let core = self.store.read(cx).client.clone();
        let task = cx.background_spawn(async move {
            let slots = core.slot_list(&id);
            (id, slots)
        });
        cx.spawn(async move |weak, cx| {
            let (id, result) = task.await;
            let _ = weak.update(cx, |this, cx| {
                if this.step != Step::Goal(Some(id)) {
                    return;
                }
                match result {
                    Ok(slots) => {
                        this.slots = Some(
                            slots
                                .iter()
                                .map(|member| slot_line(member, &this.runtimes))
                                .collect(),
                        )
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn leave_crews(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_step(Step::Start, window, cx);
    }

    pub(super) fn leave_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending.is_some() {
            return;
        }
        let Some(origin) = self.origin.take() else {
            return;
        };
        let Some(root) = self.root.upgrade() else {
            return;
        };
        window.defer(cx, move |window, cx| {
            root.update(cx, |root, cx| root.leave_launcher_page(origin, window, cx));
        });
    }

    pub(super) fn pick_crew(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(crew) = self.crews.iter().find(|crew| crew.id == id) else {
            return;
        };
        if crew.roles > 0 {
            return self.open_step(Step::Goal(Some(id)), window, cx);
        }
        let Some(root) = self.root.upgrade() else {
            return;
        };
        window.defer(cx, move |window, cx| {
            root.update(cx, |root, cx| root.open_crew_editor(id, window, cx));
        });
    }

    pub(super) fn start_mission(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Step::Goal(crew) = self.step.clone() else {
            return;
        };
        if self.pending.is_some() || (crew.is_none() && !self.offers_empty_missions()) {
            return;
        }
        let crew_name = match &crew {
            Some(id) => match self.crews.iter().find(|entry| &entry.id == id) {
                Some(entry) if entry.roles > 0 => Some(entry.name.clone()),
                _ => return,
            },
            None => None,
        };
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let goal = self.goal.read(cx).text().to_owned();
        let request = MissionRequest {
            crew: crew.unwrap_or_default(),
            title: model::mission_title(
                &goal,
                crew_name.as_deref(),
                chrono::Local::now().date_naive(),
            ),
            goal: normalized_value(&goal),
            project: self.scope().project_id().map(str::to_owned),
            cwd: normalized_value(self.cwd.read(cx).text()),
        };
        self.error = None;
        self.pending = Some(Choice::Mission);
        let launcher = cx.entity();
        window.defer(cx, move |window, cx| {
            root.update(cx, |root, cx| {
                root.launch_mission_from_launcher(launcher, request, window, cx)
            });
        });
        cx.notify();
    }

    fn chip(
        &self,
        id: &'static str,
        leading: AnyElement,
        label: String,
        on_remove: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .debug_selector(move || id.to_uppercase())
            .flex_none()
            .h(rems(22. / 16.))
            .pl(rems(6. / 16.))
            .pr(rems(4. / 16.))
            .flex()
            .items_center()
            .gap(rems(6. / 16.))
            .rounded(rems(5. / 16.))
            .border_1()
            .border_color(theme::border_strong())
            .bg(theme::raised())
            .child(leading)
            .child(
                div()
                    .text_size(theme::text_ui())
                    .text_color(theme::text())
                    .child(label),
            )
            .child(
                div()
                    .id(id)
                    .cursor_pointer()
                    .child(
                        svg()
                            .path("close.svg")
                            .size(rems(11. / 16.))
                            .text_color(theme::faint()),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| on_remove(this, window, cx))),
            )
            .into_any_element()
    }

    pub(super) fn render_mission_chip(&self, cx: &mut Context<Self>) -> AnyElement {
        self.chip(
            "launcher-mission-chip",
            svg()
                .path("users.svg")
                .size(rems(11. / 16.))
                .text_color(theme::muted())
                .into_any_element(),
            crate::i18n::text("launcher.mission").into(),
            |this, window, cx| this.leave_crews(window, cx),
            cx,
        )
    }

    pub(super) fn render_goal_field(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let crew = match &self.step {
            Step::Goal(Some(id)) => self.crews.iter().find(|crew| &crew.id == id),
            _ => None,
        };
        let leading = match crew {
            Some(crew) => crate::surfaces::crews::crew_picture(
                &crew
                    .handles
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .collect::<Vec<_>>(),
                14.,
            ),
            None => svg()
                .path("plus.svg")
                .size(rems(12. / 16.))
                .text_color(theme::muted())
                .into_any_element(),
        };
        let label = crew.map_or_else(
            || crate::i18n::text("launcher.empty_mission").to_owned(),
            |crew| crew.name.clone(),
        );
        let focused = self.goal.read(cx).focus_handle().is_focused(window);
        div()
            .debug_selector(|| "LAUNCHER_GOAL".into())
            .flex_none()
            .flex()
            .flex_col()
            .gap(rems(8. / 16.))
            .pt(rems(8. / 16.))
            .pb(rems(10. / 16.))
            .px(rems(12. / 16.))
            .rounded(rems(8. / 16.))
            .border_1()
            .border_color(if focused {
                theme::with_alpha(theme::text(), 0.2)
            } else {
                theme::border()
            })
            .bg(theme::panel())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.focus(window, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(8. / 16.))
                    .child(self.chip(
                        "launcher-crew-chip",
                        leading,
                        label,
                        |this, window, cx| this.open_step(Step::Crews, window, cx),
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex_none()
                            .text_size(theme::text_meta())
                            .text_color(theme::faint())
                            .child(crate::i18n::text("launcher.goal_note")),
                    ),
            )
            .child(self.goal.clone())
            .into_any_element()
    }

    pub(super) fn render_goal_body(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut body = Vec::new();
        if matches!(self.step, Step::Goal(Some(_))) {
            let card = div()
                .debug_selector(|| "LAUNCHER_SLOTS".into())
                .flex()
                .flex_col()
                .p(rems(4. / 16.))
                .rounded(rems(10. / 16.))
                .border_1()
                .border_color(theme::border())
                .bg(theme::panel());
            body.push(match &self.slots {
                None => card
                    .child(div().p_3().child(crate::i18n::text("ui.loading")))
                    .into_any_element(),
                Some(slots) => card
                    .children(slots.iter().map(|slot| {
                        let handle = slot.handle.clone();
                        div()
                            .debug_selector(move || format!("LAUNCHER_SLOT {handle}"))
                            .h(rems(model::COMPACT_ROW / 16.))
                            .px(rems(10. / 16.))
                            .flex()
                            .items_center()
                            .gap(rems(8. / 16.))
                            .child(RoleAvatar::new(slot.handle.clone(), 20.))
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(theme::text_body())
                                    .text_color(theme::text())
                                    .child(slot.handle.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .truncate()
                                    .text_size(theme::text_ui())
                                    .text_color(theme::muted())
                                    .child(slot.role.clone()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .max_w(rems(200. / 16.))
                                    .truncate()
                                    .font_family(theme::UI_MONOSPACE_FONT)
                                    .text_size(theme::text_meta())
                                    .text_color(theme::faint())
                                    .child(slot.setup.clone()),
                            )
                    }))
                    .into_any_element(),
            });
        }
        let starting = self.pending.is_some();
        let launcher = cx.entity();
        body.push(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .text_size(theme::text_meta())
                        .text_color(theme::faint())
                        .child(format!(
                            "{SHIFT_ENTER} {}",
                            crate::i18n::text("launcher.new_line")
                        )),
                )
                .child(
                    Button::new(
                        "launcher-start-mission",
                        if starting {
                            crate::i18n::text("ui.starting")
                        } else {
                            crate::i18n::text("ui.start_mission")
                        },
                    )
                    .variant(ButtonVariant::Primary)
                    .size(runner_app::ui::ButtonSize::Compact)
                    .shortcut("↵")
                    .loading(starting)
                    .disabled(starting)
                    .on_press(move |window, cx| {
                        launcher.update(cx, |launcher, cx| launcher.start_mission(window, cx))
                    }),
                )
                .into_any_element(),
        );
        body
    }
}

impl NativeRoot {
    pub(crate) fn open_mission_launcher(
        &mut self,
        crew: Option<String>,
        scope: ProjectScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let step = crew.map_or(Step::Crews, |id| Step::Goal(Some(id)));
        if self.launcher_page_visible() {
            self.set_route(AppRoute::Launcher, cx);
            let launcher = self.render_launcher(None, true, window, cx);
            launcher.update(cx, |launcher, cx| launcher.open_step(step, window, cx));
        } else {
            self.open_launcher_page(scope, step, window, cx);
        }
    }

    pub(crate) fn leave_launcher_page(
        &mut self,
        origin: LauncherOrigin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match origin {
            LauncherOrigin::Local {
                route: AppRoute::Chat,
                tab,
            } => {
                if let Some(tab) =
                    tab.filter(|tab| self.tabs.tabs().iter().any(|layout| &layout.id == tab))
                {
                    self.tabs.activate(&tab);
                }
                self.set_route(AppRoute::Chat, cx);
                self.sync_active_project_from_active_tab(cx);
                if let Err(error) = self.ensure_active_tab_attached(window, cx) {
                    self.chat_error = Some(error.to_string().into());
                }
                self.focus_active_terminal(window, cx);
            }
            LauncherOrigin::Local {
                route: AppRoute::Settings,
                ..
            } => self.enter_settings_route(None, window, cx),
            LauncherOrigin::Local { route, .. } => self.open_page_route(route, window, cx),
        }
        cx.notify();
    }

    fn launch_mission_from_launcher(
        &mut self,
        launcher: Entity<ChatLauncher>,
        request: MissionRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let client = launcher.read(cx).client.clone();
        let size = self.estimated_mission_terminal_size(window, cx);
        let directory = self.app_store.read(cx).app_data_dir.clone();
        let scope = ProjectScope::or_root(request.project.clone());
        let task = cx.background_spawn(async move {
            let crew = request.crew.clone();
            let result = client
                .start_mission(
                    request.crew,
                    request.title,
                    request.goal,
                    request.project,
                    request.cwd,
                    size,
                )
                .map_err(|error| error.to_string());
            if result.is_ok() && !crew.is_empty() {
                record_launcher_crew(&directory, crew);
            }
            result
        });
        cx.spawn_in(window, async move |weak, cx| {
            let result = task.await;
            let _ = weak.update_in(cx, |root, window, cx| {
                match result {
                    Ok(mission) => {
                        root.reveal_started_scope(&scope, cx);
                        root.launchers.retain(|_, cached| cached != &launcher);
                        root.refresh_store(StoreRefreshKind::All, cx);
                        root.open_mission(mission, window, cx);
                    }
                    Err(error) => {
                        launcher.update(cx, |launcher, cx| {
                            launcher.pending = None;
                            launcher.error = Some(error);
                            cx.notify();
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::start_chat::tests::{modal_harness, project_fixture, ModalHarness};

    fn harness() -> ModalHarness {
        let mut harness = modal_harness(1200., 1000., Vec::new(), Vec::new(), ChatMode::Runtime);
        harness.act(|root, _, cx| {
            root.start_chat_modal = None;
            root.set_route(AppRoute::Chat, cx);
        });
        harness
    }

    /// A crew with one codex role per handle; none leaves it without roles.
    fn crew(root: &NativeRoot, name: &str, handles: &[&str], cx: &App) -> String {
        let core = root.core(cx);
        let crew = core
            .crew_create(runner_core::protocol::crew::CreateCrewInput {
                name: name.into(),
                ..Default::default()
            })
            .unwrap();
        for handle in handles {
            let role = core
                .role_create(
                    serde_json::from_value(serde_json::json!({
                        "handle": format!("{}-{handle}", name.split(' ').next().unwrap().to_lowercase()),
                        "display_name": format!("Role {handle}"),
                        "runtime": "codex",
                        "command": "codex",
                    }))
                    .unwrap(),
                )
                .unwrap();
            core.slot_create(
                serde_json::from_value(serde_json::json!({
                    "crew_id": crew.id,
                    "role_id": role.id,
                    "slot_handle": handle,
                }))
                .unwrap(),
            )
            .unwrap();
        }
        crew.id
    }

    fn active_launcher(root: &NativeRoot, cx: &App) -> Entity<ChatLauncher> {
        root.launchers
            .values()
            .find(|launcher| launcher.read(cx).pane.is_none())
            .cloned()
            .expect("the launcher page")
    }

    fn step(root: &NativeRoot, cx: &App) -> Step {
        active_launcher(root, cx).read(cx).step.clone()
    }

    fn choices(root: &NativeRoot, cx: &App) -> Vec<Choice> {
        let launcher = active_launcher(root, cx);
        let launcher = launcher.read(cx);
        launcher
            .rows(cx)
            .0
            .into_iter()
            .map(|row| row.choice)
            .collect()
    }

    fn shown(harness: &mut ModalHarness, selector: &'static str) -> bool {
        harness.visual.debug_bounds(selector).is_some()
    }

    #[test]
    fn mission_entry_points_show_a_page_and_dismiss_back_through_steps() {
        let mut harness = harness();
        harness.act(|root, window, cx| root.open_crews(window, cx));
        harness.act(|root, window, cx| root.new_mission_action(&NewMission, window, cx));
        harness.act(|root, _, cx| {
            assert_eq!(root.route, AppRoute::Launcher);
            assert!(root.tabs.tabs().is_empty());
            assert_eq!(step(root, cx), Step::Crews);
            assert_eq!(choices(root, cx).last(), Some(&Choice::EmptyMission));
        });
        assert!(shown(&mut harness, "LAUNCHER_BRAND"));
        assert!(!shown(&mut harness, "SIDEBAR_ROW sidebar-launcher-tab"));
        harness.visual.simulate_input("find a crew");
        harness.visual.simulate_keystrokes("escape");
        harness.visual.run_until_parked();
        harness.act(|root, _, cx| assert_eq!(step(root, cx), Step::Start));
        harness.visual.simulate_keystrokes("escape");
        harness.visual.run_until_parked();
        harness.act(|root, _, _| assert_eq!(root.route, AppRoute::Crews));
    }

    #[test]
    fn new_chat_and_mission_reuse_the_page_and_escape_restores_the_previous_tab() {
        let mut harness = harness();
        let mut previous = String::new();
        harness.act(|root, window, cx| {
            root.test_open_empty_tab(window, cx);
            previous = root.tabs.active_tab_id().unwrap().to_owned();
            root.open_new_tab_modal(&NewTab, window, cx);
        });
        harness.act(|root, window, cx| {
            assert_eq!(root.route, AppRoute::Launcher);
            assert!(root.front_page_layout().is_none());
            assert_eq!(root.tabs.tabs().len(), 1);
            active_launcher(root, cx).update(cx, |launcher, cx| {
                launcher.start(Choice::Mission, false, window, cx)
            });
        });
        harness.visual.simulate_keystrokes("backspace");
        harness.visual.run_until_parked();
        harness.act(|root, _, cx| assert_eq!(step(root, cx), Step::Start));
        harness.visual.simulate_input("a search draft");
        harness.visual.simulate_keystrokes("escape");
        harness.visual.run_until_parked();
        harness.act(|root, _, _| {
            assert_eq!(root.route, AppRoute::Chat);
            assert_eq!(root.tabs.active_tab_id(), Some(previous.as_str()));
            assert_eq!(root.tabs.tabs().len(), 1);
        });
    }

    #[test]
    fn a_split_keeps_its_panes_and_mission_and_new_chat_show_the_launcher_page() {
        let mut harness = harness();
        let mut previous = String::new();
        harness.act(|root, window, cx| {
            root.test_open_empty_tab(window, cx);
            let pane = root.tabs.active().unwrap().focused_pane_id.clone();
            root.split_pane(&pane, SplitOrientation::Row, window, cx);
            let layout = root.tabs.active().unwrap().clone();
            previous = layout.id.clone();
            let view = root.render_launcher(
                Some(PaneKey::new(&layout.id, &layout.focused_pane_id)),
                false,
                window,
                cx,
            );
            assert!(view
                .read(cx)
                .rows(cx)
                .0
                .iter()
                .all(|row| row.choice != Choice::Mission));
        });
        assert!(!shown(&mut harness, "LAUNCHER_BRAND"));
        harness.act(|root, window, cx| root.new_mission_action(&NewMission, window, cx));
        harness.act(|root, _, cx| {
            assert_eq!(step(root, cx), Step::Crews);
            assert!(root.front_page_layout().is_none());
            assert_eq!(root.tabs.tabs().len(), 1);
            assert_eq!(root.tabs.active().unwrap().root.leaves().len(), 2);
        });
        assert!(shown(&mut harness, "LAUNCHER_BRAND"));
        harness.visual.simulate_keystrokes("escape escape");
        harness.visual.run_until_parked();
        harness.act(|root, _, _| {
            assert_eq!(root.tabs.active_tab_id(), Some(previous.as_str()));
            assert_eq!(root.route, AppRoute::Chat);
        });
        harness
            .visual
            .simulate_keystrokes(&keymap::platform_default("cmd-n"));
        harness.visual.run_until_parked();
        harness.act(|root, _, _| {
            assert_eq!(root.route, AppRoute::Launcher);
            assert_eq!(root.tabs.tabs().len(), 1);
        });
    }

    #[test]
    fn project_choices_and_folder_changes_save_no_launcher_tab_on_any_step() {
        let mut harness = harness();
        let projects = project_fixture(&mut harness);
        harness.act(|root, window, cx| root.open_new_tab_modal(&NewTab, window, cx));
        for (next, project) in [
            (Step::Start, projects[0].id.as_str()),
            (Step::Crews, projects[1].id.as_str()),
            (Step::Goal(None), ""),
        ] {
            harness.act(|root, window, cx| {
                active_launcher(root, cx).update(cx, |launcher, cx| {
                    launcher.open_step(next, window, cx);
                    launcher.choose_project(project, cx);
                });
                assert!(root.tabs.tabs().is_empty());
                assert!(root
                    .core(cx)
                    .node_list()
                    .unwrap()
                    .iter()
                    .all(|node| node.node_type != runner_core::protocol::node::NodeType::Tab));
            });
            assert!(shown(&mut harness, "LAUNCHER_BRAND"));
            assert!(!shown(&mut harness, "SIDEBAR_ROW sidebar-launcher-tab"));
        }
        harness.act(|root, window, cx| {
            active_launcher(root, cx)
                .update(cx, |launcher, cx| launcher.change_directory(window, cx))
        });
        let folder = PathBuf::from(&projects[0].cwd).join(".worktrees/feature");
        harness
            .visual
            .simulate_path_prompt_response(|_| Some(vec![folder]));
        harness.visual.run_until_parked();
        harness.act(|root, _, cx| {
            assert_eq!(
                active_launcher(root, cx).read(cx).scope(),
                ProjectScope::Project(projects[0].id.clone())
            );
            assert!(root.tabs.tabs().is_empty());
        });
    }

    #[test]
    fn sidebar_selection_leaves_the_page_without_saving_an_empty_tab() {
        let mut harness = harness();
        let mut tab = String::new();
        let mut sidebar = None;
        harness.act(|root, window, cx| {
            let core = root.app_store.read(cx).test_core.clone();
            let conn = core.db.get().unwrap();
            conn.execute(
                "INSERT INTO sessions(id, status, agent_runtime, agent_command) VALUES ('previous-chat', 'stopped', 'codex', 'codex')",
                [],
            )
            .unwrap();
            let layout = PaneLayout::single(Some("previous-chat"), &["previous-chat".into()]);
            tab = runner_daemon::repo::node::create_tab(
                &conn, None, "Previous chat", 0, &layout.serialize().unwrap(),
            )
            .unwrap()
            .id;
            drop(conn);
            root.app_store.update(cx, |store, cx| {
                store.refresh_sessions(cx);
                store.refresh_nodes(cx).unwrap();
            });
            root.reload_tabs(cx).unwrap();
            root.open_new_tab_modal(&NewTab, window, cx);
            sidebar = Some(root.front_page_sidebar());
        });
        harness.visual.update(|window, cx| {
            sidebar
                .unwrap()
                .update(cx, |sidebar, cx| sidebar.select_shortcut_row(1, window, cx));
        });
        harness.visual.run_until_parked();
        harness.act(|root, _, cx| {
            assert_eq!(root.route, AppRoute::Chat);
            assert_eq!(root.tabs.active_tab_id(), Some(tab.as_str()));
            assert_eq!(root.tabs.tabs().len(), 1);
            assert!(!root.launcher_page_visible());
            assert_eq!(
                root.core(cx)
                    .node_list()
                    .unwrap()
                    .iter()
                    .filter(|node| node.node_type == runner_core::protocol::node::NodeType::Tab)
                    .count(),
                1
            );
        });
    }

    #[test]
    fn successful_chat_terminal_and_mission_starts_reveal_their_collapsed_destination() {
        for kind in ["chat", "terminal", "mission"] {
            for in_project in [false, true] {
                let mut harness = harness();
                let projects = project_fixture(&mut harness);
                let scope = ProjectScope::or_root(in_project.then(|| projects[0].id.clone()));
                harness.act(|root, window, cx| {
                    root.app_store.update(cx, |store, _| {
                        store.settings.sidebar_projects_open = false;
                        store.settings.sidebar_chats_open = false;
                        store
                            .settings
                            .sidebar_collapsed_projects
                            .insert(projects[0].id.clone());
                    });
                    root.open_launcher_page(scope.clone(), Step::Start, window, cx);
                });
                harness.act(|root, _, cx| {
                    assert!(!root.settings(cx).sidebar_projects_open);
                    assert!(!root.settings(cx).sidebar_chats_open);
                    assert!(root.tabs.tabs().is_empty());
                });
                harness.act(|root, window, cx| match kind {
                    "chat" => {
                        let parent = scope.project_id().map(|id| {
                            root.core(cx).node_list().unwrap().into_iter()
                                .find(|node| node.ref_id.as_deref() == Some(id))
                                .unwrap().id
                        });
                        let core = root.app_store.read(cx).test_core.clone();
                        let conn = core.db.get().unwrap();
                        conn.execute(
                            "INSERT INTO sessions(id, status, agent_runtime, agent_command) VALUES ('started-chat', 'stopped', 'codex', 'codex')",
                            [],
                        ).unwrap();
                        let layout = PaneLayout::single(Some("started-chat"), &["started-chat".into()]);
                        runner_daemon::repo::node::create_tab(
                            &conn, parent.as_deref(), "Started chat", 0, &layout.serialize().unwrap(),
                        ).unwrap();
                        drop(conn);
                        root.finish_local_chat_launch(
                            Some("started-chat".into()),
                            Ok(crate::surfaces::front_page::client::ChatLaunched {
                                session: "started-chat".into(),
                            }),
                            window, cx,
                        );
                    }
                    "terminal" => active_launcher(root, cx).update(cx, |launcher, cx| {
                        launcher.start(Choice::Terminal, false, window, cx)
                    }),
                    "mission" => active_launcher(root, cx).update(cx, |launcher, cx| {
                        launcher.open_step(Step::Goal(None), window, cx);
                        launcher.start_mission(window, cx);
                    }),
                    _ => unreachable!(),
                });
                let mut selector = String::new();
                harness.act(|root, _, cx| {
                    let settings = root.settings(cx);
                    if in_project {
                        assert!(settings.sidebar_projects_open, "{kind}");
                        assert!(
                            !settings
                                .sidebar_collapsed_projects
                                .contains(&projects[0].id),
                            "{kind}"
                        );
                        assert!(!settings.sidebar_chats_open);
                    } else {
                        assert!(settings.sidebar_chats_open, "{kind}");
                        assert!(!settings.sidebar_projects_open);
                    }
                    let node = root
                        .core(cx)
                        .node_list()
                        .unwrap()
                        .into_iter()
                        .find(|node| {
                            node.node_type != runner_core::protocol::node::NodeType::Project
                        })
                        .unwrap();
                    selector = if kind == "mission" {
                        format!("SIDEBAR_ROW sidebar-mission-{}", node.ref_id.unwrap())
                    } else {
                        format!("SIDEBAR_ROW sidebar-tab-{}", node.id)
                    };
                });
                assert!(
                    shown(&mut harness, Box::leak(selector.into_boxed_str())),
                    "{kind}, project={in_project}"
                );
                if kind == "terminal" {
                    harness.act(|root, _, cx| {
                        let session = root
                            .tabs
                            .active()
                            .unwrap()
                            .focused_session_id()
                            .unwrap()
                            .to_owned();
                        root.core(cx).session_close(&session).unwrap();
                    });
                }
            }
        }
    }

    struct RefusedMissionStart(runner_daemon::daemon::InProcessTransport);

    impl runner_core::protocol::Transport for RefusedMissionStart {
        fn call(
            &self,
            request: runner_core::protocol::Request,
        ) -> runner_core::protocol::ClientResult<runner_core::protocol::Response> {
            if matches!(
                request,
                runner_core::protocol::Request::mission_start_impl_with_size { .. }
            ) {
                return Ok(
                    runner_core::protocol::Response::mission_start_impl_with_size(Err(
                        runner_core::protocol::ClientError::msg("bounded start failure"),
                    )),
                );
            }
            self.0.call(request)
        }

        fn subscribe(&self) -> Box<dyn runner_core::protocol::EventSubscription> {
            self.0.subscribe()
        }
    }

    #[test]
    fn failed_and_canceled_starts_keep_the_destination_collapsed() {
        for kind in ["chat", "terminal", "mission", "cancel"] {
            for in_project in [false, true] {
                let mut harness = harness();
                let projects = project_fixture(&mut harness);
                let scope = ProjectScope::or_root(in_project.then(|| projects[0].id.clone()));
                let invalid = PathBuf::from(&projects[0].cwd).join("not-a-directory");
                std::fs::write(&invalid, "fixture").unwrap();
                harness.act(|root, window, cx| {
                    root.app_store.update(cx, |store, _| {
                        store.settings.sidebar_projects_open = false;
                        store.settings.sidebar_chats_open = false;
                        store
                            .settings
                            .sidebar_collapsed_projects
                            .insert(projects[0].id.clone());
                    });
                    if kind == "mission" {
                        root.app_store.update(cx, |store, _| {
                            store.client =
                                runner_core::protocol::DaemonClient::new(std::sync::Arc::new(
                                    RefusedMissionStart(runner_daemon::daemon::InProcessTransport(
                                        store.test_core.clone(),
                                    )),
                                ));
                        });
                    }
                    root.open_crews(window, cx);
                    root.open_launcher_page(scope.clone(), Step::Start, window, cx);
                    if kind == "chat" {
                        root.finish_local_chat_launch(
                            None,
                            Err(runner_core::protocol::ClientError::msg(
                                "bounded start failure",
                            )),
                            window,
                            cx,
                        );
                    }
                    active_launcher(root, cx).update(cx, |launcher, cx| {
                        launcher.cwd.update(cx, |cwd, cx| {
                            cwd.set_text(invalid.to_string_lossy().into_owned(), cx)
                        });
                        if kind == "terminal" {
                            launcher.start(Choice::Terminal, false, window, cx);
                        } else if kind == "mission" {
                            launcher.open_step(Step::Goal(None), window, cx);
                            launcher.start_mission(window, cx);
                        }
                    });
                });
                if kind == "cancel" {
                    harness.visual.simulate_keystrokes("escape");
                    harness.visual.run_until_parked();
                }
                harness.act(|root, _, cx| {
                    assert!(!root.settings(cx).sidebar_projects_open, "{kind}");
                    assert!(!root.settings(cx).sidebar_chats_open, "{kind}");
                    assert!(root
                        .settings(cx)
                        .sidebar_collapsed_projects
                        .contains(&projects[0].id));
                    assert!(root.tabs.tabs().is_empty());
                    assert!(root.core(cx).node_list().unwrap().iter().all(
                        |node| node.node_type == runner_core::protocol::node::NodeType::Project
                    ));
                    if kind == "cancel" {
                        assert_eq!(root.route, AppRoute::Crews);
                    } else if kind == "chat" {
                        assert!(root.chat_error.is_some());
                    } else {
                        assert!(active_launcher(root, cx).read(cx).error.is_some());
                    }
                });
            }
        }
    }

    #[test]
    fn crew_step_lists_crews_with_roles_and_a_roleless_crew_dimmed_that_opens_its_page() {
        let mut harness = harness();
        let mut ids = (String::new(), String::new());
        harness.act(|root, _, cx| {
            ids = (
                crew(root, "Alpha crew", &["impl", "review"], cx),
                crew(root, "Empty crew", &[], cx),
            )
        });
        let (alpha, empty) = ids;
        harness.act(|root, window, cx| root.open_crews(window, cx));
        harness.act(|root, window, cx| root.new_mission_action(&NewMission, window, cx));
        harness.act(|root, window, cx| {
            let launcher = active_launcher(root, cx);
            let rows = launcher.read(cx).rows(cx).0;
            let row = |id: &str| {
                rows.iter()
                    .find(|row| row.choice == Choice::Crew(id.into()))
                    .unwrap()
            };
            assert!(!row(&alpha).dimmed);
            assert_eq!(row(&alpha).detail, "impl, review");
            assert!(row(&empty).dimmed);
            assert_eq!(row(&empty).detail, "No roles yet");
            assert_eq!(rows.last().unwrap().choice, Choice::EmptyMission);
            launcher.update(cx, |launcher, cx| {
                launcher.start(Choice::Crew(empty.clone()), false, window, cx)
            });
        });
        harness.act(|root, _, _| {
            assert_eq!(root.route, AppRoute::CrewEditor(empty.clone()));
            assert!(
                root.tabs.tabs().is_empty(),
                "the launcher page never saved a tab"
            );
        });
    }

    #[test]
    fn an_empty_mission_starts_from_the_launcher_page_in_the_chosen_project() {
        let mut harness = harness();
        let projects = project_fixture(&mut harness);
        let project = projects[0].clone();
        let mut sidebar = None;
        harness.act(|root, _, _| sidebar = Some(root.front_page_sidebar()));
        let scope = ProjectScope::Project(project.id.clone());
        harness.visual.update(|window, cx| {
            sidebar.clone().unwrap().update(cx, |sidebar, cx| {
                sidebar.open_launcher_in(scope, window, cx)
            })
        });
        harness.visual.run_until_parked();
        harness.act(|root, _, cx| {
            assert!(root.tabs.tabs().is_empty());
            assert_eq!(root.active_project_id(cx), Some(project.id.clone()));
            assert_eq!(step(root, cx), Step::Start);
        });
        harness.act(|root, window, cx| root.new_mission_action(&NewMission, window, cx));
        harness.visual.simulate_keystrokes("shift-enter");
        harness.visual.run_until_parked();
        harness.act(|root, _, cx| assert_eq!(step(root, cx), Step::Goal(None)));
        assert!(shown(&mut harness, "LAUNCHER-CREW-CHIP"));
        assert!(shown(&mut harness, "LAUNCHER_BRAND"));
        assert!(!shown(&mut harness, "LAUNCHER_SLOTS"));
        harness.visual.simulate_keystrokes("enter");
        harness.visual.run_until_parked();
        harness.act(|root, _, cx| {
            let AppRoute::Mission(id) = &root.route else {
                panic!("the mission opened");
            };
            let mission = root.core(cx).mission_get(id).unwrap();
            assert!(mission.crew_id.is_none());
            assert_eq!(
                mission.title,
                format!("Mission {}", chrono::Local::now().format("%Y-%m-%d"))
            );
            assert_eq!(mission.project_id.as_deref(), Some(project.id.as_str()));
            assert_eq!(mission.cwd.as_deref(), Some(project.cwd.as_str()));
            assert!(root.core(cx).session_list(id).unwrap().is_empty());
            assert!(
                root.tabs.tabs().is_empty(),
                "no empty launcher tab was saved"
            );
            assert!(
                root.launchers
                    .values()
                    .all(|launcher| launcher.read(cx).pane.is_none()),
                "no launcher is left on the tab"
            );
        });
        assert!(!shown(&mut harness, "SIDEBAR_ROW sidebar-launcher-tab"));
    }

    #[test]
    fn typing_finds_crews_after_agents_and_the_goal_step_shows_slots_and_steps_back() {
        let mut harness = harness();
        let mut alpha = String::new();
        harness.act(|root, _, cx| alpha = crew(root, "Alpha crew", &["impl", "review"], cx));
        harness.act(|root, window, cx| root.open_new_tab_modal(&NewTab, window, cx));
        harness.visual.simulate_input("alpha");
        harness.visual.run_until_parked();
        harness.act(|root, window, cx| {
            let launcher = active_launcher(root, cx);
            let rows = launcher.read(cx).rows(cx).0;
            let crew = rows
                .iter()
                .position(|row| row.choice == Choice::Crew(alpha.clone()))
                .expect("a matching crew");
            assert_eq!(rows[crew].detail, "Crew · impl, review");
            assert!(rows[crew + 1..].iter().all(|row| row.choice.pinned()));
            launcher.update(cx, |launcher, cx| {
                launcher.start(Choice::Crew(alpha.clone()), false, window, cx)
            });
        });
        harness.act(|root, _, cx| assert_eq!(step(root, cx), Step::Goal(Some(alpha.clone()))));
        for selector in [
            "LAUNCHER_GOAL",
            "LAUNCHER_SLOT impl",
            "LAUNCHER_SLOT review",
            "LAUNCHER_BRAND",
        ] {
            assert!(shown(&mut harness, selector), "{selector}");
        }
        harness.visual.simulate_input("First");
        harness.visual.simulate_keystrokes("shift-enter");
        harness.visual.simulate_input("Second");
        harness.visual.run_until_parked();
        harness.act(|root, _, cx| {
            let launcher = active_launcher(root, cx);
            assert_eq!(launcher.read(cx).goal.read(cx).text(), "First\nSecond");
            assert!(launcher.read(cx).pending.is_none());
        });
        harness.visual.simulate_keystrokes("escape");
        harness.visual.run_until_parked();
        harness.act(|root, _, cx| {
            assert_eq!(step(root, cx), Step::Crews, "Esc goes back to the crews");
            assert_eq!(
                active_launcher(root, cx).read(cx).goal.read(cx).text(),
                "First\nSecond",
                "the goal waits for the next crew"
            );
        });
        let chip = harness
            .visual
            .debug_bounds("LAUNCHER-MISSION-CHIP")
            .unwrap();
        harness.visual.simulate_click(
            chip.top_right() + gpui::point(px(-6.), px(10.)),
            Default::default(),
        );
        harness.visual.run_until_parked();
        harness.act(|root, _, cx| {
            assert_eq!(step(root, cx), Step::Start, "the chip's × goes home");
            assert!(root.tabs.tabs().is_empty());
        });
    }
}
