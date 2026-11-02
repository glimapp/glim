use super::Reader;
use glim_services::documents::Content;
use gpui_kit::{App, Window};

impl Reader {
    pub fn from_content(content: Content, window: &mut Window, cx: &mut App) -> Reader {
        let path = content.path().to_path_buf();
        let _scope = glim_services::diagnostics::FileScope::enter(&path, "initialize");
        log::info!("reader initialize file={}", path.display());
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Self::from_content_inner(content, window, cx)
        })) {
            Ok(reader) => reader,
            Err(_) => Reader::unavailable(
                path.clone(),
                glim_services::diagnostics::failure(&path)
                    .unwrap_or_else(|| "Reader initialization failed".into()),
                window,
                cx,
            ),
        }
    }
    fn from_content_inner(content: Content, window: &mut Window, cx: &mut App) -> Reader {
        match content {
            Content::Archive(d) => Reader::from_archive(d, cx),
            Content::File(d) => Reader::new(d, window, cx),
            Content::Page(d) => Reader::from_page(d, window, cx),
            Content::Image(d) => Reader::from_image(d, window, cx),
            Content::XMind(d) => Reader::from_xmind(d, cx),
            Content::Opml(d) => Reader::from_opml(d, cx),
            Content::Book(d) => Reader::from_book(d, window, cx),
            Content::Sqlite(path) => Reader::from_sqlite(path, window, cx),
            Content::Native(path, folder, shuffle) => {
                Reader::from_native(path, folder, shuffle, cx)
            }
            Content::Unavailable(p, e) => Reader::unavailable(p, e, window, cx),
        }
    }
}
