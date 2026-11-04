//! Application adapter for book preparation, image ownership and reading-state storage.
mod view;
use self::view::{BookView, Catalog, Policy, PreparedChapter, Request};
use super::media::PreparedImage;
use crate::app::reader_jobs::{self, JobClass, Priority};
use glim_core::{
    books::{Chapter, Format, ReadingState, TocEntry},
    resources::{ResourceClass, ResourceLease},
};
use glim_services::{
    books::{self, Book, ChapterContent},
    limits,
    work::{Cancellation, scheduler::Scheduler},
};
use gpui_kit::*;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
pub(crate) use view::{
    BookContents, BookNextChapter, BookNextPage, BookPreviousChapter, BookPreviousPage,
};

trait StateStore: Send + Sync {
    fn read(&self, fingerprint: &str) -> anyhow::Result<ReadingState>;
    fn save(&self, fingerprint: &str, state: &ReadingState) -> anyhow::Result<()>;
}
struct StoredPreferences;
impl StateStore for StoredPreferences {
    fn read(&self, fingerprint: &str) -> anyhow::Result<ReadingState> {
        books::preferences::read(fingerprint)
    }
    fn save(&self, fingerprint: &str, state: &ReadingState) -> anyhow::Result<()> {
        books::preferences::save(fingerprint, state)
    }
}

impl Catalog for Book {
    fn title(&self) -> &str {
        &self.title
    }
    fn author(&self) -> &str {
        &self.author
    }
    fn format(&self) -> Format {
        self.format
    }
    fn chapters(&self) -> &[Chapter] {
        &self.chapters
    }
    fn toc(&self) -> &[TocEntry] {
        &self.toc
    }
}
fn activity(kind: &'static str, action: &'static str, metrics: &[super::contract::ReaderMetric]) {
    use glim_services::telemetry::{flag, track};
    match action {
        "page" | "chapter" => {
            let forward = metrics
                .iter()
                .find_map(|metric| match metric {
                    super::contract::ReaderMetric::Flag("forward", forward) => Some(*forward),
                    _ => None,
                })
                .unwrap_or_default();
            track(
                if action == "page" {
                    "ebook_page"
                } else {
                    "ebook_chapter"
                },
                [("forward", flag(forward))],
            );
        }
        "bookmark" => track("ebook_bookmark", []),
        _ => (super::host().action)(kind, action, metrics),
    }
}

pub(super) struct BookReader {
    active: bool,
    initialized: bool,
    _book_memory: ResourceLease,
    book: Arc<Book>,
    view: Entity<BookView>,
    scheduler: Arc<Scheduler>,
    storage: Arc<dyn StateStore>,
    paged: Option<Entity<super::paged::PagedReader>>,
    images: Arc<HashMap<String, Entity<PreparedImage>>>,
    missing: Entity<PreparedImage>,
    comic: Option<Entity<PreparedImage>>,
    task: Option<Task<()>>,
    write_task: Option<Task<()>>,
    pub(super) writes: Arc<Mutex<Option<ReadingState>>>,
    pub writing: Arc<AtomicBool>,
    _subscription: Option<Subscription>,
    _image_subscriptions: Vec<Subscription>,
    _missing_subscription: Subscription,
    _view_subscription: Subscription,
}
impl BookReader {
    pub fn new(book: Book, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_dependencies(
            book,
            Scheduler::global().clone(),
            Arc::new(StoredPreferences),
            window,
            cx,
        )
    }
    fn with_dependencies(
        book: Book,
        scheduler: Arc<Scheduler>,
        storage: Arc<dyn StateStore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let memory =
            glim_services::resources::pool().track(book.heap_bytes(), ResourceClass::State);
        let book = Arc::new(book);
        let missing = cx.new(|cx| PreparedImage::new(books::missing_image(), cx));
        let missing_subscription = cx.observe(&missing, |_, _, cx| cx.notify());
        let mut host = super::host();
        host.action = activity;
        let view = cx.new(|cx| {
            BookView::new(
                book.clone(),
                host,
                Policy {
                    label_chars: limits::BOOK_STATE_LABEL_CHARS,
                    toc_depth: limits::BOOK_TOC_DEPTH,
                },
                memory.clone(),
                window,
                cx,
            )
        });
        let view_subscription = cx.observe(&view, |_, _, cx| cx.notify());
        let writes = view.read(cx).pending.clone();
        let mut reader = Self {
            active: true,
            initialized: false,
            _book_memory: memory,
            book,
            view,
            scheduler,
            storage,
            paged: None,
            images: Arc::new(HashMap::new()),
            missing,
            comic: None,
            task: None,
            write_task: None,
            writes,
            writing: Arc::new(AtomicBool::new(false)),
            _subscription: None,
            _image_subscriptions: Vec::new(),
            _missing_subscription: missing_subscription,
            _view_subscription: view_subscription,
        };
        cx.on_release(|reader, cx| reader.release_images(cx))
            .detach();
        reader.bind_window(window, cx);
        reader.load(true, cx);
        reader
    }
    fn bind_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._subscription = Some(cx.subscribe_in(
            &self.view,
            window,
            |reader, _, request: &Request, window, cx| {
                reader.request(request.clone(), window, cx);
            },
        ));
    }
    fn request(&mut self, request: Request, window: &mut Window, cx: &mut Context<Self>) {
        match request {
            Request::Prepare(index) => {
                if self.view.read(cx).state.chapter != index {
                    return;
                }
                self.initialized = true;
                self.task = None;
                self.paged = None;
                if self.active {
                    self.load(false, cx);
                } else {
                    self.release_images(cx);
                }
            }
            Request::Persist => self.start_write(cx),
            Request::Follow(href) if self.active => self.follow(&href, cx),
            Request::ShowText { title, text } if self.active => {
                super::source::show_text_dialog(glim_services::locale::tr(title), text, window, cx);
            }
            _ => {}
        }
    }
    fn load(&mut self, initial: bool, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        self.task = None;
        self.release_images(cx);
        let index = self.view.read(cx).state.chapter;
        let revision = self.view.read(cx).revision;
        self.view.update(cx, |view, cx| {
            view.loading = true;
            view.error = None;
            view.release_layout();
            cx.notify();
        });
        let book = self.book.clone();
        let memory = self._book_memory.clone();
        let storage = self.storage.clone();
        let (cancel, guard) = Cancellation::task();
        let started = std::time::Instant::now();
        let work = reader_jobs::run_with_scheduler(
            cx.background_executor().clone(),
            self.scheduler.clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |cancel| {
                let _memory = memory;
                let state = if initial {
                    let mut state = storage.read(&book.fingerprint).unwrap_or_default();
                    state.chapter = state.chapter.min(book.chapters.len() - 1);
                    Some(state)
                } else {
                    None
                };
                let content = books::chapter_cancellable(
                    &book,
                    state.as_ref().map_or(index, |state| state.chapter),
                    cancel,
                )?;
                Ok((state, content))
            },
        );
        self.task = Some(cx.spawn(async move |reader, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result("books_load", None, started, &result);
            let _ = reader.update(cx, |reader, cx| {
                if !reader.active || reader.view.read(cx).revision != revision {
                    return;
                }
                reader.task = None;
                match result {
                    Ok((state, content)) => {
                        reader.initialized = true;
                        if let Some(state) = state {
                            reader.view.update(cx, |view, _| view.state = state);
                        }
                        reader.install(content, cx);
                    }
                    Err(error) => {
                        glim_services::telemetry::operation_failed("books_load", None, &error);
                        reader.view.update(cx, |view, cx| {
                            view.loading = false;
                            view.error = Some(format!("{error:#}"));
                            cx.notify();
                        });
                    }
                }
                cx.notify();
            });
        }));
    }
    fn install(&mut self, mut content: ChapterContent, cx: &mut Context<Self>) {
        let memory = glim_services::resources::pool().track(
            content.html.len() as u64
                + (content.anchors.capacity()
                    * std::mem::size_of::<(String, std::ops::Range<usize>)>())
                    as u64
                + content
                    .anchors
                    .keys()
                    .map(|key| key.capacity() as u64)
                    .sum::<u64>(),
            ResourceClass::Working,
        );
        self.paged = content.page.take().map(|page| {
            cx.new(|_| {
                let mut reader = super::paged::PagedReader::readonly(page);
                reader.controls_bottom = px(128.);
                reader
            })
        });
        self.images = Arc::new(
            content
                .images
                .into_iter()
                .filter(|(_, data)| !data.is_empty())
                .map(|(name, data)| (name, cx.new(|cx| PreparedImage::new(data, cx))))
                .collect(),
        );
        let ratio = content
            .comic
            .as_ref()
            .and_then(|data| {
                let width = u32::from_be_bytes(data.get(16..20)?.try_into().ok()?);
                let height = u32::from_be_bytes(data.get(20..24)?.try_into().ok()?);
                (width > 0 && height > 0).then_some(width as f32 / height as f32)
            })
            .unwrap_or(1.);
        self.comic = content
            .comic
            .map(|data| cx.new(|cx| PreparedImage::new(data, cx)));
        self._image_subscriptions = self
            .images
            .values()
            .chain(self.comic.iter())
            .map(|image| cx.observe(image, |_, _, cx| cx.notify()))
            .collect();
        let images = self.images.clone();
        let missing = self.missing.clone();
        let prepared = PreparedChapter {
            html: content.html,
            anchors: content.anchors,
            images: Arc::new(move |name| {
                PreparedImage::source(images.get(name).unwrap_or(&missing))
            }),
            comic: self.comic.as_ref().map(PreparedImage::source),
            comic_ratio: ratio,
            page: self.paged.clone().map(Into::into),
            memory,
        };
        self.view.update(cx, |view, cx| view.install(prepared, cx));
    }
    fn suspend_images(&self, cx: &mut App) {
        for image in self
            .images
            .values()
            .chain(self.comic.iter())
            .chain(std::iter::once(&self.missing))
        {
            image.update(cx, |image, cx| image.suspend(cx));
        }
    }
    fn release_images(&mut self, cx: &mut App) {
        self.suspend_images(cx);
        self._image_subscriptions.clear();
        self.images = Arc::new(HashMap::new());
        self.comic = None;
    }
    fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        if !active {
            self.task = None;
            let selected = self.view.update(cx, |view, cx| view.suspend(cx));
            if selected {
                self.suspend_images(cx);
            } else {
                self.release_images(cx);
            }
            if let Some(page) = &self.paged {
                page.update(cx, |page, _| page.suspend());
            }
        } else {
            self.view.update(cx, |view, cx| {
                view.active = true;
                cx.notify();
            });
            if let Some(page) = &self.paged {
                page.update(cx, |page, cx| page.resume(cx));
            } else if !self.view.read(cx).has_content() {
                self.load(!self.initialized, cx);
            }
        }
        cx.notify();
    }
    fn follow(&mut self, href: &str, cx: &mut Context<Self>) {
        glim_services::telemetry::reader_feature("ebook", "follow_link", []);
        if let Ok(url) = url::Url::parse(href)
            && matches!(url.scheme(), "https" | "http" | "mailto")
        {
            cx.open_url(href);
            return;
        }
        let chapter = self.view.read(cx).state.chapter;
        let base = &self.book.chapters[chapter].href;
        if self.book.format == Format::FictionBook && href.starts_with('#') {
            self.view.update(cx, |view, cx| {
                view.anchor = Some(href.trim_start_matches('#').into());
                cx.notify();
            });
            return;
        }
        let direct = href
            .split('#')
            .next()
            .is_some_and(|path| self.book.chapters.iter().any(|c| c.href == path));
        if let Some(target) = if direct {
            Some(href.to_owned())
        } else {
            books::resolve(base, href)
        } {
            let (path, anchor) = target
                .split_once('#')
                .map(|(p, a)| (p, Some(a.to_owned())))
                .unwrap_or((target.as_str(), None));
            if let Some(index) = self.book.chapters.iter().position(|c| c.href == path) {
                if index == chapter {
                    self.view.update(cx, |view, cx| {
                        view.anchor = anchor;
                        cx.notify();
                    });
                } else {
                    self.task = None;
                    self.view
                        .update(cx, |view, cx| view.go(index, 0., anchor, cx));
                }
            }
        }
    }
    fn start_write(&mut self, cx: &mut Context<Self>) {
        if self.writes.lock().unwrap().is_none() {
            return;
        }
        if self.writing.swap(true, Ordering::SeqCst) {
            return;
        }
        let writes = self.writes.clone();
        let writing = self.writing.clone();
        let fingerprint = self.book.fingerprint.clone();
        let storage = self.storage.clone();
        self.write_task = Some(cx.spawn(async move |reader, cx| {
            loop {
                let state = {
                    let mut pending = writes.lock().unwrap();
                    if let Some(state) = pending.take() {
                        state
                    } else {
                        writing.store(false, Ordering::SeqCst);
                        break;
                    }
                };
                let key = fingerprint.clone();
                let storage = storage.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { storage.save(&key, &state) })
                    .await;
                if let Err(error) = result {
                    let _ = reader.update(cx, |reader, cx| {
                        reader.view.update(cx, |view, cx| {
                            view.error = Some(format!("Cannot save reading position: {error:#}"));
                            cx.notify();
                        });
                    });
                }
            }
        }));
    }
    pub fn take_write(&mut self, cx: &mut Context<Self>) -> Option<Task<()>> {
        self.start_write(cx);
        self.write_task.take()
    }
}
impl Drop for BookReader {
    fn drop(&mut self) {
        if let Some(task) = self.write_task.take() {
            task.detach();
        }
    }
}
impl Render for BookReader {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.view.clone()
    }
}

pub(super) struct BookSurface {
    pub view: Entity<BookReader>,
    pub writing: Arc<AtomicBool>,
    pub pending: Arc<Mutex<Option<ReadingState>>>,
}
impl crate::views::reader::contract::ReaderView for BookSurface {
    fn kind(&self) -> &'static str {
        "ebook"
    }
    fn render(&mut self, _cx: &mut App) -> AnyElement {
        self.view.clone().into_any_element()
    }
    fn set_active(&self, active: bool, cx: &mut App) {
        self.view.update(cx, |view, cx| view.set_active(active, cx));
    }
    fn bind_window(&self, window: &mut Window, cx: &mut App) {
        self.view
            .update(cx, |reader, cx| reader.bind_window(window, cx));
    }
    fn operations(&self) -> crate::views::reader::contract::ReaderOperations<'_> {
        crate::views::reader::contract::ReaderOperations {
            persistence: Some(self),
            ..Default::default()
        }
    }
}
impl super::surface::ReaderSurface for BookSurface {}
impl crate::views::reader::contract::ReaderPersistence for BookSurface {
    fn is_persisting(&self) -> bool {
        self.writing.load(Ordering::SeqCst) || self.pending.lock().unwrap().is_some()
    }
    fn take_write(&self, cx: &mut App) -> Option<Task<()>> {
        self.view.update(cx, |view, cx| view.take_write(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::{BookReader, BookSurface, ReadingState, Scheduler, StateStore};
    use crate::views::reader::contract::ReaderPersistence;
    use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext, WindowOptions};
    use std::collections::HashMap;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Default)]
    struct MemoryStore {
        state: Mutex<ReadingState>,
        reads: AtomicUsize,
        saves: AtomicUsize,
    }
    impl StateStore for MemoryStore {
        fn read(&self, _: &str) -> anyhow::Result<ReadingState> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self.state.lock().unwrap().clone())
        }
        fn save(&self, _: &str, state: &ReadingState) -> anyhow::Result<()> {
            self.saves.fetch_add(1, Ordering::SeqCst);
            *self.state.lock().unwrap() = state.clone();
            Ok(())
        }
    }
    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.fb2");
        std::fs::write(
            &path,
            include_bytes!("../../../../../assets/reader-contracts/book.fb2"),
        )
        .unwrap();
        (dir, path)
    }
    fn epub_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let entries: HashMap<String, String> = serde_json::from_str(include_str!(
            "../../../../../assets/reader-contracts/book.epub.json"
        ))
        .unwrap();
        for (name, text) in entries {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let path = dir.path().join("fixture.epub");
        // macOS's archive tool is used only to assemble the test fixture.
        let result = std::process::Command::new("/usr/bin/zip")
            .current_dir(dir.path())
            .args(["-q", "-r", "fixture.epub", "mimetype", "META-INF", "OPS"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        (dir, path)
    }
    fn open(
        cx: &mut TestAppContext,
        path: &std::path::Path,
        storage: Arc<MemoryStore>,
    ) -> (AnyWindowHandle, Entity<BookReader>) {
        let book = glim_services::books::read(path).unwrap();
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    BookReader::with_dependencies(
                        book,
                        Scheduler::from_policy(),
                        storage,
                        window,
                        cx,
                    )
                })
            })
            .unwrap()
        })
    }
    fn settle(cx: &mut TestAppContext, reader: &Entity<BookReader>) {
        cx.run_until_parked();
        cx.update(|cx| {
            let reader = reader.read(cx);
            assert!(reader.task.is_none(), "chapter task still active");
            assert!(
                !reader.writing.load(Ordering::SeqCst),
                "state writer still active"
            );
            assert!(reader.writes.lock().unwrap().is_none());
        });
    }
    #[gpui_kit::test]
    fn interrupted_initial_restore_and_hidden_navigation_keep_the_latest_state(
        cx: &mut TestAppContext,
    ) {
        let (_dir, path) = fixture();
        let original = std::fs::read(&path).unwrap();
        let storage = Arc::new(MemoryStore::default());
        *storage.state.lock().unwrap() = ReadingState {
            chapter: 1,
            fraction: 0.625,
            font_size: 23.,
            ..Default::default()
        };
        let (_, reader) = open(cx, &path, storage.clone());
        cx.update(|cx| reader.update(cx, |reader, cx| reader.set_active(false, cx)));
        settle(cx, &reader);
        assert_eq!(storage.saves.load(Ordering::SeqCst), 0);
        cx.update(|cx| reader.update(cx, |reader, cx| reader.set_active(true, cx)));
        settle(cx, &reader);
        cx.update(|cx| {
            let view = reader.read(cx).view.read(cx);
            assert_eq!(
                (
                    view.state.chapter,
                    view.state.fraction,
                    view.state.font_size
                ),
                (1, 0.625, 23.)
            );
            assert!(view.error.is_none() && view.has_content());
        });
        assert_eq!(storage.saves.load(Ordering::SeqCst), 0);
        cx.update(|cx| {
            let view = reader.read(cx).view.clone();
            view.update(cx, |view, cx| view.go(0, 0.25, Some("first".into()), cx));
            reader.update(cx, |reader, cx| reader.set_active(false, cx));
        });
        settle(cx, &reader);
        assert_eq!(storage.state.lock().unwrap().chapter, 0);
        assert_eq!(storage.state.lock().unwrap().fraction, 0.25);
        cx.update(|cx| reader.update(cx, |reader, cx| reader.set_active(true, cx)));
        settle(cx, &reader);
        cx.update(|cx| {
            let view = reader.read(cx).view.read(cx);
            assert_eq!(view.state.chapter, 0);
            assert!(view.error.is_none() && view.has_content());
        });
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    #[gpui_kit::test]
    fn queued_state_is_visible_to_shutdown_before_request_delivery_and_writes_outlive_drop(
        cx: &mut TestAppContext,
    ) {
        let (_dir, path) = fixture();
        let storage = Arc::new(MemoryStore::default());
        let (handle, reader) = open(cx, &path, storage.clone());
        settle(cx, &reader);
        let writing = cx.update(|cx| reader.read(cx).writing.clone());
        let task = cx.update(|cx| {
            let view = reader.read(cx).view.clone();
            view.update(cx, |view, cx| view.go(1, 0.375, None, cx));
            let surface = BookSurface {
                view: reader.clone(),
                writing: writing.clone(),
                pending: reader.read(cx).writes.clone(),
            };
            assert!(surface.is_persisting());
            let task = surface
                .take_write(cx)
                .expect("pending state is drained synchronously");
            assert!(surface.is_persisting());
            task
        });
        cx.update_window(handle, |_, window, _| window.remove_window())
            .unwrap();
        drop(reader);
        task.detach();
        cx.run_until_parked();
        assert!(!writing.load(Ordering::SeqCst));
        let saved = storage.state.lock().unwrap();
        assert_eq!((saved.chapter, saved.fraction), (1, 0.375));
    }
    #[gpui_kit::test]
    fn chapter_replacement_retry_and_window_rebinding_keep_one_presentation(
        cx: &mut TestAppContext,
    ) {
        let (_dir, path) = epub_fixture();
        let storage = Arc::new(MemoryStore::default());
        let (old, reader) = open(cx, &path, storage.clone());
        settle(cx, &reader);
        let presentation = cx.update(|cx| reader.read(cx).view.entity_id());
        cx.update(|cx| {
            let view = reader.read(cx).view.clone();
            view.update(cx, |view, cx| {
                view.go(1, 0.5, None, cx);
                view.go(0, 0.125, None, cx);
            });
        });
        settle(cx, &reader);
        let (destination, _) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                reader.update(cx, |reader, cx| reader.bind_window(window, cx));
                reader.clone()
            })
            .unwrap()
        });
        cx.update_window(old, |_, window, _| window.remove_window())
            .unwrap();
        cx.update(|cx| {
            let view = reader.read(cx).view.clone();
            view.update(cx, |view, cx| view.follow("OPS/two.xhtml", cx));
        });
        settle(cx, &reader);
        cx.update(|cx| {
            assert_eq!(reader.read(cx).view.entity_id(), presentation);
            assert_eq!(reader.read(cx).view.read(cx).state.chapter, 1);
            assert!(reader.read(cx).view.read(cx).has_content());
        });
        let original = std::fs::read(&path).unwrap();
        // Fail one restoration, then use the same navigation path to retry.
        std::fs::remove_file(&path).unwrap();
        cx.update(|cx| {
            reader.update(cx, |reader, cx| {
                reader.set_active(false, cx);
                reader.set_active(true, cx);
            })
        });
        settle(cx, &reader);
        cx.update(|cx| assert!(reader.read(cx).view.read(cx).error.is_some()));
        std::fs::write(&path, &original).unwrap();
        cx.update(|cx| {
            reader.update(cx, |reader, cx| {
                reader.set_active(false, cx);
                reader.set_active(true, cx);
            })
        });
        settle(cx, &reader);
        cx.update_window(destination, |_, _, cx| {
            assert_eq!(reader.read(cx).view.entity_id(), presentation);
            assert!(reader.read(cx).view.read(cx).error.is_none());
        })
        .unwrap();
    }
}
