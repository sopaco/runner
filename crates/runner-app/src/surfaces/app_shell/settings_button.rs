use super::*;

impl NativeRoot {
    pub(super) fn render_settings_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let zoom = self.settings(cx).app_zoom;
        div()
            .id("sidebar-settings")
            .debug_selector(|| "SIDEBAR_SETTINGS".into())
            .tab_index(0)
            .flex_none()
            .size(px(32. * zoom))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .occlude()
            .hover(|button| button.bg(theme::chrome_hover()))
            .focus_visible(|button| button.bg(theme::chrome_hover()))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(
                cx.listener(|root, _, window, cx| root.enter_settings_route(None, window, cx)),
            )
            .on_key_down(cx.listener(|root, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    root.enter_settings_route(None, window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                svg()
                    .path("settings.svg")
                    .size(px(16. * zoom))
                    .text_color(theme::muted()),
            )
            .into_any_element()
    }
}
