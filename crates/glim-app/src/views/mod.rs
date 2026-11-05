pub(crate) mod about;
pub mod explorer;
mod minimap;
mod native_menu_bridge;
mod path_hint;
mod reader;
#[cfg(feature = "reader-measurements")]
pub(crate) use reader::Reader;
mod welcome;
pub mod workspace;
pub(crate) mod workspace_window;

mod file_icons;
pub(crate) mod integrations;

pub(crate) mod updates;

/// Brand ink for navigation and primary entry points; tuned for each appearance.
fn accent(cx: &gpui_kit::App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    gpui_kit::rgb(if cx.theme().is_dark() {
        0x65d5c3
    } else {
        0x167566
    })
    .into()
}

fn icon_button(
    id: impl Into<gpui_kit::ElementId>,
    icon: gpui_kit::assets::IconName,
    label: String,
) -> gpui_kit::component::button::Button {
    use gpui_kit::{
        component::{Icon, Sizable, button::Button},
        *,
    };
    Button::new(id)
        .small()
        .w(px(28.))
        .h(px(28.))
        .icon(Icon::new(icon).size(px(16.)))
        .tooltip(label.clone())
        .accessibility_label(label)
}

/// Opaque selection fill shared by sidebar navigation and file-tree rows.
fn selection_background(cx: &gpui_kit::App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    let strength = if cx.theme().is_dark() { 0.18 } else { 0.12 };
    cx.theme()
        .sidebar
        .blend(accent(cx).opacity(strength))
        .opacity(1.)
}

fn connection_color(connected: bool, cx: &gpui_kit::App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    if connected {
        accent(cx)
    } else {
        cx.theme().danger
    }
}

fn loading_icon(
    id: impl Into<gpui_kit::ElementId>,
    icon_size: gpui_kit::Pixels,
    label: String,
    cx: &gpui_kit::App,
) -> gpui_kit::AnyElement {
    use gpui_kit::{
        component::{Sizable, spinner::Spinner, tooltip::Tooltip},
        *,
    };
    div()
        .id(id)
        .size(icon_size)
        .flex_shrink_0()
        .role(Role::Status)
        .aria_label(label.clone())
        .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
        .child(Spinner::new().with_size(icon_size).color(accent(cx)))
        .into_any_element()
}

fn connection_icon(
    id: impl Into<gpui_kit::ElementId>,
    connected: bool,
    icon_size: gpui_kit::Pixels,
    cx: &gpui_kit::App,
) -> gpui_kit::AnyElement {
    use gpui_kit::{
        component::{Icon, tooltip::Tooltip},
        *,
    };
    let status = glim_services::locale::tr(if connected {
        "Connected"
    } else {
        "Not connected"
    });
    div()
        .id(id)
        .size(icon_size)
        .relative()
        .flex_shrink_0()
        .role(Role::Status)
        .aria_label(status.clone())
        .tooltip(move |window, cx| Tooltip::new(status.clone()).build(window, cx))
        .child(
            Icon::new(gpui_kit::assets::IconName::Globe)
                .absolute()
                .top(px(-1.))
                .left(px(-1.))
                .size(icon_size + px(2.))
                .text_color(connection_color(connected, cx)),
        )
        .into_any_element()
}

pub(crate) use reader::diagram_cache_activity;
pub(crate) use reader::resource_snapshot;

pub(crate) use reader::set_native_telemetry_enabled;

pub(crate) use reader::{
    BookContents, BookNextChapter, BookNextPage, BookPreviousChapter, BookPreviousPage,
};
