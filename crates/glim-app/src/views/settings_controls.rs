use gpui_kit::{
    component::{
        ActiveTheme,
        button::Button,
        h_flex,
        menu::{DropdownMenu, PopupMenu, PopupMenuItem},
        tooltip::Tooltip,
        v_flex,
    },
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

/// A choice menu takes its width from the laid-out field, including in narrow dialogs.
pub(super) fn field_dropdown(
    mut button: Button,
    window: &mut Window,
    cx: &mut App,
    choices: impl Fn(PopupMenu) -> PopupMenu + 'static,
) -> Div {
    let id = button
        .interactivity()
        .element_id
        .clone()
        .expect("button id");
    let measured =
        window.use_keyed_state((id, "field-width"), cx, |_, _| std::cell::Cell::new(px(0.)));
    let width = measured.clone();
    div()
        .min_w_0()
        .child(button.dropdown_menu(move |menu, _, cx| {
            let width = width.read(cx).get();
            choices(menu).min_w(width).max_w(width)
        }))
        .on_children_prepainted(move |bounds, _, cx| {
            if let Some(field) = bounds.first() {
                measured.read(cx).set(field.size.width);
            }
        })
}

pub(super) fn field_choice(label: impl Into<SharedString>) -> PopupMenuItem {
    let label = label.into();
    PopupMenuItem::element(move |_, _| {
        let full_label = label.clone();
        div()
            .id("choice-label")
            .flex_1()
            .min_w_0()
            .truncate()
            .aria_label(label.clone())
            .tooltip(move |window, cx| Tooltip::new(full_label.clone()).build(window, cx))
            .child(label.clone())
    })
}
