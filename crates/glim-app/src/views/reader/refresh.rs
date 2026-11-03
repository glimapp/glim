use super::{Reader, Renderer};
use glim_services::documents::refresh::RefreshRequest;
use gpui_kit::App;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct RefreshVersion {
    document: crate::app::document_session::DocumentVersion,
    page: Option<u64>,
    edit: Option<u64>,
}
impl Reader {
    pub(crate) fn refresh_busy(&self, cx: &App) -> bool {
        matches!(&self.renderer, Renderer::Paged { view, .. } if !view.read(cx).can_move_to_window())
    }

    pub(crate) fn refresh_version(&self, cx: &App) -> RefreshVersion {
        RefreshVersion {
            document: self.document.version(),
            edit: self.document.edit.as_ref().map(|edit| edit.revision()),
            page: match &self.renderer {
                Renderer::Paged { view, .. } => Some(view.read(cx).refresh_revision()),
                _ => None,
            },
        }
    }

    pub(crate) fn refresh_request(&self, force: bool, cx: &App) -> Option<RefreshRequest> {
        if self.document.draft_directory.is_some() || self.is_dirty() {
            return None;
        }
        let path = self.document.path.clone();
        match &self.renderer {
            Renderer::Surface(view) => view.refresh_request(&path, force, cx),
            Renderer::Paged { view, .. } => view
                .read(cx)
                .refresh_request()
                .map(RefreshRequest::Page)
                .or(Some(RefreshRequest::Reload(path))),
            _ => Some(RefreshRequest::Reload(path)),
        }
    }
}
