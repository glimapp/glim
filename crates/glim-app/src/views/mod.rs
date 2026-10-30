pub(crate) mod about;
pub mod explorer;
mod minimap;
mod native_menu_bridge;
mod path_hint;
mod reader;
mod welcome;
pub mod workspace;
pub(crate) mod workspace_window;

mod file_icons;

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

/// Opaque selection fill shared by sidebar navigation and file-tree rows.
fn selection_background(cx: &gpui_kit::App) -> gpui_kit::Hsla {
    use gpui_kit::component::ActiveTheme;
    let strength = if cx.theme().is_dark() { 0.18 } else { 0.12 };
    cx.theme()
        .sidebar
        .blend(accent(cx).opacity(strength))
        .opacity(1.)
}

pub(crate) use reader::resource_snapshot;

pub(crate) use reader::set_native_telemetry_enabled;
