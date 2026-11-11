use super::media::{GifReader, ImageReader, UnavailableReader};
use super::*;
use glim_services::documents::Content;

impl Reader {
    pub fn from_content(content: Content, window: &mut Window, cx: &mut App) -> Reader {
        Self::build_content(content, true, window, cx)
    }
    pub fn from_restored_content(content: Content, window: &mut Window, cx: &mut App) -> Reader {
        Self::build_content(content, false, window, cx)
    }
    fn build_content(
        content: Content,
        autoplay: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Reader {
        let path = content.path().to_path_buf();
        let _scope = glim_services::diagnostics::FileScope::enter(&path, "initialize");
        log::info!("reader initialize file={}", path.display());
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Self::from_content_inner(content, autoplay, window, cx)
        })) {
            Ok(reader) => reader,
            Err(_) => Reader::unavailable(
                path.clone(),
                glim_services::diagnostics::failure(&path)
                    .unwrap_or_else(|| "Reader initialization failed".into())
                    .into(),
                window,
                cx,
            ),
        }
    }
    fn from_content_inner(
        content: Content,
        autoplay: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Reader {
        match content {
            Content::OpenApi(source) => {
                let mut reader = Self::from_content_inner(*source, autoplay, window, cx);
                reader.projection = Some(Projection::openapi(reader.document.path.clone(), cx));
                reader.set_preview(true, cx);
                reader
            }
            Content::Archive(d) => Reader::from_archive(d, cx),
            Content::File(d) => Reader::new(d, window, cx),
            Content::Page(d) => Reader::from_page(d, window, cx),
            Content::Image(d) => Reader::from_image(d, window, cx),
            Content::XMind(d) => Reader::from_xmind(d, cx),
            Content::Opml(d) => Reader::from_opml(d, cx),
            Content::Book(d) => Reader::from_book(d, window, cx),
            Content::Sqlite(path) => Reader::from_sqlite(path, window, cx),
            Content::Native(path, folder, shuffle) => {
                Reader::from_native_playback(path, folder, shuffle, autoplay, cx)
            }
            Content::Unavailable(p, e) => Reader::unavailable(p, e, window, cx),
        }
    }
}

impl Reader {
    pub fn new(document: Document, window: &mut Window, cx: &mut App) -> Self {
        let presentation = TextPresentation::for_path(&document.path);
        let renderer = if document.kind == DocumentKind::Markdown || presentation.markup.is_some() {
            Renderer::Markup(Box::new(MarkupReader::new(
                &document.path,
                &document.text,
                presentation.markup.unwrap_or(MarkupFormat::Markdown),
                window,
                cx,
            )))
        } else {
            Renderer::Source(SourceReader::new(
                &document.text,
                language_for_path(&document.path),
                false,
                ScrollHandle::new(),
                window,
                cx,
            ))
        };
        let projection = Projection::new(&presentation, None, cx);
        let structured_controls = presentation.structured.then(|| {
            let editor = match &renderer {
                Renderer::Source(source) => Some(source.edit.editor().clone()),
                _ => None,
            };
            cx.new(|_| json_format::JsonFormat::new(document.path.clone(), editor))
        });
        let mut reader = Self {
            active: std::cell::Cell::new(None),
            title: document.path.display().to_string().into(),
            document: DocumentSession::new(
                document.path,
                ContentSnapshot::Text(document.text.into()),
            )
            .with_edit(match &renderer {
                Renderer::Source(source) => Some(source.edit.clone()),
                Renderer::Markup(markup) => Some(markup.source.edit.clone()),
                _ => None,
            }),
            structured_controls,
            projection,
            renderer,
        };
        reader.restore_csv_preview(cx);
        reader
    }
    pub fn from_archive(document: glim_services::archive::Archive, cx: &mut App) -> Self {
        Self {
            active: std::cell::Cell::new(None),
            title: document.path.display().to_string().into(),
            document: DocumentSession::new(document.path.clone(), ContentSnapshot::Untracked),
            structured_controls: None,
            projection: None,
            renderer: Renderer::entity(cx.new(|cx| archive::ArchiveReader::new(document, cx))),
        }
    }
    pub fn from_sqlite(path: PathBuf, window: &mut Window, cx: &mut App) -> Self {
        Self {
            active: std::cell::Cell::new(None),
            title: path.display().to_string().into(),
            document: DocumentSession::new(path.clone(), ContentSnapshot::Untracked),
            structured_controls: None,
            projection: None,
            renderer: Renderer::entity(cx.new(|cx| sqlite::SqliteReader::new(path, window, cx))),
        }
    }
    pub(super) fn from_native_playback(
        path: PathBuf,
        folder: bool,
        shuffle: bool,
        autoplay: bool,
        cx: &mut App,
    ) -> Self {
        Self {
            active: std::cell::Cell::new(None),
            title: path.display().to_string().into(),
            document: DocumentSession::new(path.clone(), ContentSnapshot::Untracked),
            structured_controls: None,
            projection: None,
            renderer: Renderer::entity(cx.new(|cx| {
                if autoplay {
                    native::NativeReader::new(path, folder, shuffle, cx)
                } else {
                    native::NativeReader::new_paused(path, folder, shuffle, cx)
                }
            })),
        }
    }
    pub fn from_page(
        page: glim_services::paged::TextPage,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let path = page.path.clone();
        let snapshot = page.fingerprint();
        let view = cx.new(|cx| paged::PagedReader::new(page, window, cx));
        let edit = view
            .read(cx)
            .source_editor
            .as_ref()
            .map(|source| source.edit.clone());
        let saving = view.read(cx).saving.clone();
        let presentation = TextPresentation::for_path(&path);
        let projection = Projection::new(&presentation, Some(&path), cx);
        let html = (presentation.markup == Some(MarkupFormat::Html)).then(|| {
            let active = cx
                .try_global::<crate::app::ReaderPreferences>()
                .is_none_or(|prefs| prefs.html_preview);
            (
                cx.new(|cx| native::NativeReader::html(path.clone(), None, window, cx)),
                active,
            )
        });
        let structured_controls = presentation
            .structured
            .then(|| cx.new(|_| json_format::JsonFormat::new(path.clone(), None)));
        let mut reader = Self {
            active: std::cell::Cell::new(None),
            title: path.display().to_string().into(),
            document: DocumentSession::new(path, ContentSnapshot::Fingerprint(snapshot))
                .with_edit(edit),
            structured_controls,
            projection,
            renderer: Renderer::Paged { html, view, saving },
        };
        reader.restore_csv_preview(cx);
        reader
    }
    #[cfg(test)]
    pub fn from_bytes(
        document: glim_services::binary::BytePreview,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let source = SourceReader::new(
            &document.text,
            "plain",
            true,
            ScrollHandle::new(),
            window,
            cx,
        );
        Self {
            active: std::cell::Cell::new(None),
            title: document.path.display().to_string().into(),
            document: DocumentSession::new(
                document.path,
                ContentSnapshot::Text(document.text.into()),
            ),
            structured_controls: None,
            projection: None,
            renderer: Renderer::Source(source),
        }
    }
    pub fn from_image(
        document: glim_services::media::ImageDocument,
        _: &mut Window,
        cx: &mut App,
    ) -> Self {
        let renderer = if document.gif {
            Renderer::entity(cx.new(|cx| GifReader::from_file(document.path.clone(), cx)))
        } else {
            let pages = if document.pages.is_empty() {
                vec![document.png]
            } else {
                document.pages
            };
            Renderer::surface(ImageReader::new(pages, cx))
        };
        Self {
            active: std::cell::Cell::new(None),
            title: document.path.display().to_string().into(),
            document: DocumentSession::new(
                document.path,
                ContentSnapshot::Fingerprint(document.fingerprint),
            ),
            structured_controls: None,
            projection: None,
            renderer,
        }
    }
    pub fn from_book(
        document: glim_services::books::Book,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let path = document.path.clone();
        let fingerprint = document.fingerprint.clone();
        let view = cx.new(|cx| books::BookReader::new(document, window, cx));
        let writing = view.read(cx).writing.clone();
        let pending = view.read(cx).writes.clone();
        Self {
            active: std::cell::Cell::new(None),
            title: path.display().to_string().into(),
            document: DocumentSession::new(path, ContentSnapshot::Fingerprint(fingerprint)),
            structured_controls: None,
            projection: None,
            renderer: Renderer::surface(books::BookSurface {
                view,
                writing,
                pending,
            }),
        }
    }
    pub fn from_xmind(document: glim_services::xmind::MindMap, cx: &mut App) -> Self {
        Self {
            active: std::cell::Cell::new(None),
            title: document.path.display().to_string().into(),
            document: DocumentSession::new(
                document.path.clone(),
                ContentSnapshot::Fingerprint(document.fingerprint.clone()),
            ),
            structured_controls: None,
            projection: None,
            renderer: Renderer::surface(xmind::GraphSurface::new(document, super::host(), cx)),
        }
    }
    pub fn from_opml(document: glim_services::opml::Document, cx: &mut App) -> Self {
        Self {
            active: std::cell::Cell::new(None),
            title: document.path.display().to_string().into(),
            document: DocumentSession::new(
                document.path.clone(),
                ContentSnapshot::Fingerprint(document.fingerprint.clone()),
            ),
            structured_controls: None,
            projection: None,
            renderer: Renderer::surface(opml::OutlineSurface::new(document, super::host(), cx)),
        }
    }
    pub fn unavailable(
        path: PathBuf,
        reason: glim_services::documents::ReadFailure,
        _: &mut Window,
        _: &mut App,
    ) -> Self {
        let mut document = DocumentSession::new(path.clone(), ContentSnapshot::Untracked);
        document.source_missing =
            reason.kind == glim_services::documents::ReadFailureKind::NotFound;
        let renderer = Renderer::surface(UnavailableReader::new(&path, reason));
        Self {
            active: std::cell::Cell::new(None),
            title: path.display().to_string().into(),
            document,
            structured_controls: None,
            projection: None,
            renderer,
        }
    }
}
