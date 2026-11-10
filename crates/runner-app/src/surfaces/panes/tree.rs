use super::*;

impl NativeRoot {
    /// Split menus and pane sizes are keyed by tab *and* pane, because a pane
    /// id such as `p1` exists in every tab; drop everything the active tab
    /// does not have so a switch cannot inherit the other tab's measurement
    /// or leave its menu hanging open.
    pub(super) fn prune_pane_state(&mut self, layout: &PaneLayout) {
        let panes = layout
            .root
            .leaves()
            .into_iter()
            .map(|leaf| leaf.id.as_str())
            .collect::<HashSet<_>>();
        let tab_id = layout.id.as_str();
        let region_keys = panes
            .iter()
            .map(|pane| format!("pane:{tab_id}:{pane}"))
            .collect::<HashSet<_>>();
        self.render_regions
            .retain(|key, _| !key.starts_with("pane:") || region_keys.contains(key));
        self.split_menus
            .retain(|key, _| key.tab_id == tab_id && panes.contains(key.pane_id.as_str()));
        self.front_page_pane_bounds_mut()
            .retain(|key, _| key.tab_id == tab_id && panes.contains(key.pane_id.as_str()));
    }

    /// The split menu for one pane. The header and an identity line differ
    /// only in trigger size, so each keeps its own entity rather than
    /// resizing a shared one when a tab is split or collapsed.
    pub(super) fn split_menu(
        &mut self,
        surface: SplitMenuSurface,
        tab_id: &str,
        pane_id: &str,
        cx: &mut Context<Self>,
    ) -> Entity<PopoverMenu> {
        let key = PaneKey::new(tab_id, pane_id);
        let menu = self
            .split_menus
            .entry(SplitMenuKey {
                surface,
                tab_id: key.tab_id.clone(),
                pane_id: key.pane_id.clone(),
            })
            .or_insert_with(|| {
                let root = cx.entity();
                let target = pane_id.to_owned();
                let id = SharedString::from(format!("split-menu-{}-{pane_id}", surface.key()));
                cx.new(move |menu_cx| {
                    let menu = PopoverMenu::new(
                        id,
                        menu_cx.focus_handle(),
                        Vec::new(),
                        Rc::new(move |index, window, cx| {
                            let pane_id = target.clone();
                            root.update(cx, |this, cx| match index {
                                0 => this.split_pane(&pane_id, SplitOrientation::Row, window, cx),
                                1 => {
                                    this.split_pane(&pane_id, SplitOrientation::Column, window, cx)
                                }
                                _ => {}
                            });
                        }),
                        menu_cx,
                    )
                    .min_width(px(196.))
                    .trigger_icon("columns-2.svg")
                    .without_trigger_tooltip();
                    match surface {
                        SplitMenuSurface::Pane => menu.trigger_size(IconButtonSize::Sm),
                    }
                })
            })
            .clone();
        let settings = self.settings(cx);
        let zoom = settings.app_zoom;
        let overrides = settings.keymap_overrides.clone();
        let items = split_menu_items(
            self.front_page_pane_bounds().get(&key).copied(),
            zoom,
            &overrides,
        );
        menu.update(cx, |menu, menu_cx| menu.set_items(items, menu_cx));
        menu
    }

    pub(crate) fn render_pane_node(
        &mut self,
        node: &PaneNode,
        layout: &PaneLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_card_pane_node(node, layout, cx)
    }

    pub(super) fn render_card_pane_node(
        &mut self,
        node: &PaneNode,
        layout: &PaneLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node {
            PaneNode::Leaf(leaf) => {
                let key = format!("pane:{}:{}", layout.id, leaf.id);
                let leaf = leaf.clone();
                let layout = layout.clone();
                self.cached_region(
                    key,
                    gpui::StyleRefinement::default().size_full(),
                    move |root, window, cx| root.render_pane(&leaf, &layout, window, cx),
                    cx,
                )
            }
            PaneNode::Split(split) => {
                let a = self.render_card_pane_node(&split.a, layout, cx);
                let b = self.render_card_pane_node(&split.b, layout, cx);
                let first = split.sizes[0] / 100.;
                let second = split.sizes[1] / 100.;
                let split_id = split.id.clone();
                let orientation = split.orientation;
                let drag = SplitResizeDrag {
                    split_id: split.id.clone(),
                    orientation,
                };
                let a_wrap = div()
                    .min_w(px(0.))
                    .min_h(px(0.))
                    .when(orientation == SplitOrientation::Row, |pane| {
                        pane.flex_basis(px(0.)).flex_grow(first).h_full()
                    })
                    .when(orientation == SplitOrientation::Column, |pane| {
                        pane.flex_basis(px(0.)).flex_grow(first).w_full()
                    })
                    .child(a);
                let b_wrap = div()
                    .min_w(px(0.))
                    .min_h(px(0.))
                    .when(orientation == SplitOrientation::Row, |pane| {
                        pane.flex_basis(px(0.)).flex_grow(second).h_full()
                    })
                    .when(orientation == SplitOrientation::Column, |pane| {
                        pane.flex_basis(px(0.)).flex_grow(second).w_full()
                    })
                    .child(b);
                let gutter = resize_strip(
                    "pane-gutter",
                    match orientation {
                        SplitOrientation::Row => ResizeAxis::Columns,
                        SplitOrientation::Column => ResizeAxis::Rows,
                    },
                    self.front_page_resizing_split() == Some(split.id.as_str()),
                    self.settings(cx).app_zoom,
                    None,
                    false,
                )
                .id(SharedString::from(format!("gutter-{}", split.id)))
                .on_drag(drag, |drag: &SplitResizeDrag, _, _, cx: &mut App| {
                    cx.new(|_| drag.clone())
                });
                div()
                    .id(SharedString::from(format!("split-{}", split.id)))
                    .size_full()
                    .flex()
                    .when(orientation == SplitOrientation::Column, |container| {
                        container.flex_col()
                    })
                    .child(a_wrap)
                    .child(gutter)
                    .child(b_wrap)
                    .on_drag_move::<SplitResizeDrag>(cx.listener(
                        move |this, event: &DragMoveEvent<SplitResizeDrag>, _, cx| {
                            let drag = event.drag(cx);
                            if drag.split_id == split_id {
                                this.resize_split(&split_id, drag.orientation, event, cx);
                            }
                        },
                    ))
                    .on_drop(cx.listener(|this, _: &SplitResizeDrag, _, cx| {
                        this.finish_split_resize(cx);
                    }))
                    .into_any_element()
            }
        }
    }

    pub(super) fn pane_action_menu(
        &mut self,
        entry: &SessionDisplay,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> Entity<PopoverMenu> {
        let shell = entry.agent_runtime == "shell";
        let session_id = &entry.session_id;
        let menu = self
            .pane_action_menus
            .entry(session_id.to_owned())
            .or_insert_with(|| {
                let root = cx.entity();
                let target = session_id.to_owned();
                cx.new(move |menu_cx| {
                    let action_root = root.clone();
                    let action_target = target.clone();
                    PopoverMenu::new(
                        SharedString::from(format!("pane-actions-{target}")),
                        menu_cx.focus_handle(),
                        Vec::new(),
                        Rc::new(move |index, window, cx| {
                            let session_id = action_target.clone();
                            action_root.update(cx, |this, cx| {
                                let Some(entry) = this.front_page_session(&session_id, cx) else {
                                    return;
                                };
                                let Some(index) = pane_action_indexes(
                                    &entry,
                                    this.cached_chat_secondary_state(&session_id).secondary,
                                )
                                .get(index)
                                .copied() else {
                                    return;
                                };
                                match index {
                                    0 => this.activate_primary_pane_action(&session_id, window, cx),
                                    1 => this.fork_chat(&session_id, window, cx),
                                    2 => this.open_chat_rename(
                                        ChatRenameTarget::Session {
                                            session_id,
                                            original: entry.title.clone().unwrap_or_default(),
                                        },
                                        window,
                                        cx,
                                    ),
                                    _ => {}
                                }
                            });
                        }),
                        menu_cx,
                    )
                    .min_width(px(if shell { 144. } else { 196. }))
                    .single_line_labels()
                    .trigger_size(IconButtonSize::Sm)
                    .trigger_icon("more-horizontal.svg")
                    .trigger_tooltip(crate::i18n::message("ui.pane_actions"))
                })
            })
            .clone();
        let stop_shortcut =
            keymap::effective_binding("stop-session", &self.settings(cx).keymap_overrides)
                .map(|combo| keymap::format_combo(&combo));
        let resume_shortcut =
            keymap::effective_binding("resume-session", &self.settings(cx).keymap_overrides)
                .map(|combo| keymap::format_combo(&combo));
        let secondary = self.cached_chat_secondary_state(session_id).secondary;
        let indexes = pane_action_indexes(entry, secondary);
        let fork = if let Some(local) = entry.local.as_ref() {
            header_fork_state(Some(local), secondary)
        } else if !entry.native_fork {
            HeaderForkState::Disabled(None)
        } else if !entry.forkable {
            HeaderForkState::Disabled(Some(crate::i18n::text("ui.no_session_key_captured_yet")))
        } else {
            HeaderForkState::Enabled
        };
        let pending = self
            .fork_confirm
            .as_ref()
            .is_some_and(|confirm| confirm.pending && confirm.session_id == *session_id)
            || super::super::chat::fork_in_progress(&self.forking_sessions, session_id);
        let mut items = pane_action_items_for(
            &entry.agent_runtime,
            entry.status == SessionStatus::Running,
            disabled,
            stop_shortcut,
            resume_shortcut,
        );
        if entry.agent_runtime != "shell" {
            items[1].disabled = disabled || pending || !matches!(fork, HeaderForkState::Enabled);
            if let HeaderForkState::Disabled(Some(reason)) = fork {
                items[1] = items[1].clone().tooltip(reason);
            }
            items = items
                .into_iter()
                .enumerate()
                .filter_map(|(index, item)| indexes.contains(&index).then_some(item))
                .collect();
        }
        menu.update(cx, |menu, menu_cx| menu.set_items(items, menu_cx));
        menu
    }

    pub(super) fn activate_primary_pane_action(
        &mut self,
        session_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session_lifecycle_disabled(session_id, cx)
            || self.chat_secondary_state(session_id, cx).secondary
        {
            return;
        }
        let Some(status) = self
            .front_page_session(session_id, cx)
            .map(|entry| entry.status)
        else {
            return;
        };
        if status == SessionStatus::Running {
            self.stop_chat(session_id, window, cx);
        } else {
            self.resume_chats(vec![session_id.to_owned()], window, cx);
        }
    }
}
