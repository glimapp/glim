mod archive;
mod books;
pub(crate) use books::{
    BookContents, BookNextChapter, BookNextPage, BookPreviousChapter, BookPreviousPage,
};
mod boundary;
mod construction;
mod contract;
mod controls;
mod csv;
mod diagrams;
mod editing;
mod find;
pub(crate) use find::{NextMatch, PreviousMatch};
mod image_detail;
mod json;
mod json_format;
mod lexical;
mod markup;
mod media;
mod native;
mod text;

mod native_bridge;
mod openapi;
mod opml;
mod paged;
mod pictures;
mod projection;
mod refresh;
mod source;
mod sqlite;
mod surface;
mod xmind;
use crate::app::document_session::DocumentSession;
use glim_core::{ContentSnapshot, Document, DocumentKind, ReaderMode, language_for_path};
use glim_services::documents::presentation::{MarkupFormat, TableFormat, TextPresentation};
use gpui_kit::*;
use markup::MarkupReader;
use projection::Projection;
use source::SourceReader;
use std::{path::PathBuf, sync::Arc};

/// Keep native text editing actions without the toolkit's unused LSP entries.
pub(super) fn editor_menu(
    menu: gpui_kit::component::native_menu::NativeMenu,
    editable: bool,
    copyable: bool,
) -> gpui_kit::component::native_menu::NativeMenu {
    use gpui_kit::component::input::{Copy, Cut, Paste, SelectAll};
    menu.menu_with_disabled(
        glim_services::locale::tr("Cut"),
        !(editable && copyable),
        Box::new(Cut),
    )
    .menu_with_disabled(glim_services::locale::tr("Copy"), !copyable, Box::new(Copy))
    .menu_with_disabled(
        glim_services::locale::tr("Paste"),
        !editable,
        Box::new(Paste),
    )
    .separator()
    .menu(glim_services::locale::tr("Select All"), Box::new(SelectAll))
}

pub(super) fn preview_loading(cx: &App) -> AnyElement {
    controls::loading(glim_services::locale::tr("Loading preview…"), cx)
}

pub use crate::views::reader::contract::ArchiveAction;

pub type OpenLink = Arc<dyn Fn(PathBuf, Option<String>, &mut Window, &mut App) + Send + Sync>;

pub type ToggleTask =
    Arc<dyn Fn(EntityId, std::ops::Range<usize>, bool, usize, &mut Window, &mut App) + Send + Sync>;

/// Tab identity is separate from the concrete renderer and its persistent state.
/// No image/error tab allocates an unused editor. JSON shares the code editor's
/// syntax-aware folding; Markdown owns preview state as well as a source editor.
enum Renderer {
    Source(SourceReader),
    Paged {
        view: Entity<paged::PagedReader>,
        html: Option<(Entity<native::NativeReader>, bool)>,
        saving: std::rc::Rc<std::cell::Cell<bool>>,
    },
    Markup(Box<MarkupReader>),
    Surface(Box<dyn surface::ReaderSurface>),
}
impl Renderer {
    fn entity<T: 'static>(view: Entity<T>) -> Self
    where
        surface::SurfaceEntity<T>: surface::ReaderSurface,
    {
        Self::surface(surface::SurfaceEntity(view))
    }
    fn surface(view: impl surface::ReaderSurface + 'static) -> Self {
        Self::Surface(Box::new(view))
    }
}

pub struct Reader {
    active: std::cell::Cell<Option<bool>>,
    pub(crate) document: DocumentSession,
    pub title: SharedString,
    projection: Option<Projection>,
    renderer: Renderer,
    structured_controls: Option<Entity<json_format::JsonFormat>>,
}
impl Reader {
    pub(crate) fn report_opened(&self, trigger: &'static str) {
        use glim_services::telemetry::{file_event, flag, text};
        file_event(
            "reader_opened",
            &self.document.path,
            [
                ("reader_kind", text(self.telemetry_kind())),
                ("unavailable", flag(self.is_unavailable())),
                ("can_preview", flag(self.can_preview())),
                (
                    "can_edit",
                    flag(self.document.remote.is_none() && self.editor().is_some()),
                ),
                (
                    "storage",
                    text(if self.document.remote.is_some() {
                        "ssh"
                    } else {
                        "local"
                    }),
                ),
                ("trigger", text(trigger)),
            ],
        );
    }
    pub fn telemetry_kind(&self) -> &'static str {
        if self.projection.as_ref().is_some_and(Projection::is_openapi) {
            return "openapi";
        }
        match &self.renderer {
            Renderer::Source(_) => "source",
            Renderer::Markup(markup) => {
                if markup.html {
                    "html"
                } else {
                    "markdown"
                }
            }
            Renderer::Paged { .. } => "paged",
            Renderer::Surface(view) => view.kind(),
        }
    }
    pub fn copy_image(&self, cx: &mut App) -> bool {
        matches!(&self.renderer, Renderer::Surface(view) if view.operations().image_copy.is_some_and(|copy| copy.copy_image(cx)))
    }
    pub fn set_archive_action(&mut self, label: String, action: ArchiveAction, cx: &mut App) {
        if let Renderer::Surface(view) = &self.renderer
            && let Some(archive) = view.operations().archive
        {
            archive.set_extract_action(label, action, cx);
        }
    }
    pub fn is_archive(&self) -> bool {
        matches!(&self.renderer, Renderer::Surface(view) if view.operations().archive.is_some())
    }

    pub(crate) fn session_tab(&self, cx: &App) -> glim_core::session::SessionTab {
        use glim_core::session::SessionTab;
        if let Renderer::Surface(view) = &self.renderer
            && let Some(shuffle) = view
                .operations()
                .playback
                .and_then(|playback| playback.playlist_shuffle(cx))
        {
            return SessionTab::Playlist {
                path: self.document.path.clone(),
                shuffle,
            };
        }
        SessionTab::File {
            path: self.document.path.clone(),
            preview: self.mode() == glim_core::ReaderMode::Preview,
        }
    }

    pub fn is_persisting(&self) -> bool {
        matches!(&self.renderer, Renderer::Surface(view) if view.operations().persistence.is_some_and(|write| write.is_persisting()))
    }
    pub(crate) fn take_reader_write(&mut self, cx: &mut App) -> Option<Task<()>> {
        match &self.renderer {
            Renderer::Surface(view) => view
                .operations()
                .persistence
                .and_then(|write| write.take_write(cx)),
            _ => None,
        }
    }

    pub(crate) fn can_move_to_window(&self, cx: &App) -> bool {
        self.structured_controls
            .as_ref()
            .is_none_or(|controls| controls.read(cx).can_move_to_window())
            && match &self.renderer {
                Renderer::Paged { view, .. } => view.read(cx).can_move_to_window(),
                Renderer::Surface(view) => view.can_move_to_window(cx),
                _ => true,
            }
    }

    pub(crate) fn bind_window(&self, window: &mut Window, cx: &mut App) {
        if let Renderer::Surface(view) = &self.renderer {
            view.bind_window(window, cx);
        }
    }

    pub fn deactivate(&self, cx: &mut App) {
        if self.active.replace(Some(false)) == Some(false) {
            return;
        }
        if let Some(source) = self.source() {
            source
                .minimap
                .update(cx, |map, cx| map.set_active(false, cx));
        }
        if let Some(projection) = &self.projection {
            projection.set_active(false, cx);
        }
        if let Renderer::Paged { view, .. } = &self.renderer {
            view.update(cx, |view, _| view.suspend());
        }
        if let Renderer::Paged {
            html: Some((view, _)),
            ..
        } = &self.renderer
        {
            view.update(cx, |view, cx| view.set_active(false, cx));
        }
        if let Renderer::Markup(view) = &self.renderer {
            view.set_images_active(false, cx);
        }
        if let Renderer::Surface(view) = &self.renderer {
            view.set_active(false, cx);
        }
    }

    pub fn activate(&self, cx: &mut App) {
        if self.active.replace(Some(true)) == Some(true) {
            return;
        }
        if let Some(source) = self.source() {
            source
                .minimap
                .update(cx, |map, cx| map.set_active(true, cx));
        }
        if let Some(projection) = &self.projection {
            projection.set_active(true, cx);
        }
        if let Renderer::Paged { view, .. } = &self.renderer {
            view.update(cx, |view, cx| view.resume(cx));
        }
        if let Renderer::Paged {
            html: Some((view, true)),
            ..
        } = &self.renderer
        {
            view.update(cx, |view, cx| view.open(cx));
            return;
        }
        if let Renderer::Surface(view) = &self.renderer {
            view.set_active(true, cx);
        }
    }
    pub fn is_unavailable(&self) -> bool {
        matches!(&self.renderer, Renderer::Surface(view) if view.is_unavailable())
    }

    pub(crate) fn refresh_in_place(
        &self,
        content: &glim_services::documents::Content,
        cx: &mut App,
    ) -> bool {
        matches!(&self.renderer, Renderer::Surface(view) if view.refresh_in_place(content, cx))
    }

    pub fn render(&mut self, open: OpenLink, cx: &mut App) -> AnyElement {
        if let Some(error) = glim_services::diagnostics::failure(&self.document.path) {
            self.deactivate(cx);
            return boundary::failed(&self.document.path, error, cx);
        }
        let _scope = glim_services::diagnostics::FileScope::enter(&self.document.path, "render");
        let content = self.render_content(open, cx);
        let content = if let Some(controls) = &self.structured_controls
            && !self
                .projection
                .as_ref()
                .is_some_and(|view| view.is_openapi() && view.visible())
        {
            let blocked = matches!(&self.renderer, Renderer::Paged { .. }) && self.is_dirty();
            controls.update(cx, |controls, _| controls.blocked = blocked);
            div()
                .relative()
                .size_full()
                .child(content)
                .child(controls.clone())
                .into_any_element()
        } else {
            content
        };
        boundary::FileBoundary {
            path: self.document.path.clone(),
            content,
        }
        .into_any_element()
    }
    fn render_content(&mut self, open: OpenLink, cx: &mut App) -> AnyElement {
        if let Some(content) = self.projection.as_ref().and_then(Projection::render) {
            return content;
        }
        match &mut self.renderer {
            Renderer::Source(s) => s.render(cx),
            Renderer::Markup(m) => m.render(open, cx),
            Renderer::Surface(view) => view.render(cx),
            Renderer::Paged {
                html: Some((browser, true)),
                ..
            } => {
                browser.update(cx, |browser, cx| {
                    browser.set_html_open(open);
                    browser.open(cx);
                });
                browser.clone().into_any_element()
            }
            Renderer::Paged { view, .. } => view.clone().into_any_element(),
        }
    }
}

pub(crate) fn set_native_telemetry_enabled(enabled: bool) {
    native_bridge::set_telemetry_enabled(enabled);
}

pub(super) fn host() -> crate::views::reader::contract::ReaderHost {
    crate::views::reader::contract::ReaderHost {
        translate: glim_services::locale::tr,
        action: |module, action, metrics| {
            use crate::views::reader::contract::ReaderMetric;
            use glim_services::telemetry::{flag, number, reader_feature};
            reader_feature(
                module,
                action,
                metrics.iter().map(|metric| match metric {
                    ReaderMetric::Flag(key, value) => (*key, flag(*value)),
                    ReaderMetric::Count(key, value) => (*key, number(*value)),
                }),
            );
        },
        resources: glim_services::resources::pool().clone(),
    }
}
