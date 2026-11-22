use gpui_kit::{
    component::{ActiveTheme, h_flex, v_flex},
    *,
};

pub(super) fn settings_row(
    label: &'static str,
    detail: &'static str,
    control: AnyElement,
    cx: &App,
) -> Div {
    let mut text = v_flex()
        .flex_1()
        .min_w(px(0.))
        .gap_1()
        .child(div().text_sm().child(glim_services::locale::tr(label)));
    if !detail.is_empty() {
        text = text.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(glim_services::locale::tr(detail)),
        );
    }
    h_flex()
        .w_full()
        .min_h(px(58.))
        .py_3()
        .gap_4()
        .child(text)
        .child(div().flex_shrink_0().child(control))
}

pub(super) fn settings_group(cx: &App) -> Div {
    v_flex().w_full().px_4().rounded(px(12.)).bg(cx
        .theme()
        .background
        .blend(cx.theme().foreground.opacity(0.035)))
}
