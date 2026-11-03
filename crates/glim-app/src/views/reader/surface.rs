//! Contract for retained, non-editing reader presentations.
use super::ArchiveAction;
use glim_services::documents::refresh::RefreshRequest;
use gpui_kit::*;
use std::path::Path;

pub(super) trait ReaderSurface {
    fn kind(&self) -> &'static str;
    fn render(&mut self, cx: &mut App) -> AnyElement;
    fn can_move_to_window(&self, _cx: &App) -> bool {
        true
    }
    fn bind_window(&self, _window: &mut Window, _cx: &mut App) {}
    fn set_active(&self, _active: bool, _cx: &mut App) {}
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
    fn playlist_shuffle(&self, _cx: &App) -> Option<bool> {
        None
    }
    fn copy_image(&self, _cx: &mut App) -> bool {
        false
    }
    fn is_archive(&self) -> bool {
        false
    }
    fn set_archive_action(&self, _label: String, _action: ArchiveAction, _cx: &mut App) {}
    fn is_unavailable(&self) -> bool {
        false
    }
    fn is_persisting(&self) -> bool {
        false
    }
    fn take_write(&self, _cx: &mut App) -> Option<Task<()>> {
        None
    }
}
