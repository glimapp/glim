//! Application refresh adapter around the independently reusable UI contract.
use glim_services::documents::refresh::RefreshRequest;
use gpui_kit::*;
use std::path::Path;

pub(super) struct SurfaceEntity<T: 'static>(pub Entity<T>);
impl<T> std::ops::Deref for SurfaceEntity<T> {
    type Target = Entity<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub(super) trait ReaderSurface: crate::views::reader::contract::ReaderView {
    fn refresh_request(&self, path: &Path, _force: bool, _cx: &App) -> Option<RefreshRequest> {
        Some(RefreshRequest::Reload(path.to_path_buf()))
    }
    fn refresh_in_place(
        &self,
        _content: &glim_services::documents::Content,
        _cx: &mut App,
    ) -> bool {
        false
    }
}
