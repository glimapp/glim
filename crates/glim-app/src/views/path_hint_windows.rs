use gpui_kit::{Bounds, Pixels, Window};

#[derive(Default)]
pub(super) struct PathHint;

impl PathHint {
    pub fn hide(&mut self) {}

    pub fn hide_for(&mut self, _path: &str, _bounds: Bounds<Pixels>) {}

    pub fn show(
        &mut self,
        _title: &str,
        _path: &str,
        _directory: bool,
        _bounds: Bounds<Pixels>,
        _window: &Window,
    ) {
    }
}
