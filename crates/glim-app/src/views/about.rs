use crate::app::actions::{CloseWindow, CloseWorkspace};
use gpui_kit::{
    component::{TitleBar, button::Button, h_flex, v_flex},
    *,
};

pub(crate) struct About {
    focus: FocusHandle,
    closing: bool,
}

impl About {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let view = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            let _ = view.update(cx, |view, cx| view.close("window_button", window, cx));
            false
        });
        glim_services::telemetry::track("about_opened", []);
        cx.on_release(|_, _| glim_services::telemetry::track("about_closed", []))
            .detach();
        Self {
            focus,
            closing: false,
        }
    }

    fn close(&mut self, source: &'static str, window: &Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.closing = true;
        glim_services::telemetry::track(
            "about_close_requested",
            [("input_source", glim_services::telemetry::text(source))],
        );
        let handle = window.window_handle();
        // Leave AppKit's windowShouldClose callback before dropping the native
        // window and its renderer. Only this captured window may be removed.
        cx.spawn(async move |_, cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        })
        .detach();
    }
}

impl Render for About {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .track_focus(&self.focus)
            .on_action(cx.listener(|view, _: &CloseWindow, window, cx| {
                view.close("close_window_action", window, cx)
            }))
            .on_action(cx.listener(|view, _: &CloseWorkspace, window, cx| {
                view.close("close_workspace_action", window, cx)
            }))
            .child(TitleBar::new().child(glim_services::locale::tr("About Glim")))
            .child(
                v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_4()
                            .child(img("branding/glim.png").size(px(64.)))
                            .child(
                                div()
                                    .text_3xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Glim"),
                            ),
                    )
                    .child(env!("CARGO_PKG_VERSION"))
                    .child(
                        Button::new("about-check-update")
                            .label(glim_services::locale::tr("Check for Updates…"))
                            .on_click(|_, window, cx| {
                                crate::app::updates::show(window, cx);
                                crate::app::updates::command("check", cx);
                            }),
                    ),
            )
    }
}
