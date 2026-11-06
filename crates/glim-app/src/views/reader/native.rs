use crate::app::reader_jobs::{self, JobClass, Priority};
use gpui_kit::{
    component::{
        ActiveTheme, WindowExt,
        button::{Button, ButtonVariants},
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};
use std::path::PathBuf;

pub(super) struct NativeReader {
    path: PathBuf,
    folder: bool,
    shuffle: bool,
    autoplay: bool,
    task: Option<Task<()>>,
    pdf_refresh_task: Option<Task<()>>,
    // Drop the native controller before its prepared data (including SQLite snapshots).
    preview: Option<super::native_bridge::NativePreview>,
    prepared: Option<glim_services::preview::PreparedPreview>,
    error: Option<String>,
    obscured: bool,
    raw: Option<Entity<super::paged::PagedReader>>,
    loading_bytes: bool,
    html: Option<String>,
    html_file: bool,
    html_open: Option<super::OpenLink>,
    html_anchor: Option<String>,
    _html_navigation: Option<Task<()>>,
}
impl NativeReader {
    pub(super) fn find(&self, show: bool) -> bool {
        self.preview
            .as_ref()
            .is_some_and(|preview| preview.find(show))
    }
    pub fn html(
        path: PathBuf,
        text: Option<String>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self::browser(path, text)
    }
    pub fn openapi(path: PathBuf, configuration: &[u8]) -> Self {
        let mut reader = Self::browser(path, Some(String::new()));
        reader.preview = Some(super::native_bridge::NativePreview::new(configuration));
        reader
    }
    fn browser(path: PathBuf, text: Option<String>) -> Self {
        // WebKit and navigation polling start only when this preview is visible.
        Self {
            path,
            folder: false,
            shuffle: false,
            autoplay: false,
            task: None,
            pdf_refresh_task: None,
            preview: None,
            prepared: None,
            error: None,
            obscured: false,
            raw: None,
            loading_bytes: false,
            html_file: text.is_none(),
            html: text,
            html_open: None,
            html_anchor: None,
            _html_navigation: None,
        }
    }
    fn start_html_navigation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self._html_navigation.is_some()
            || self.html_open.is_none()
            || !(self.html_file || self.html.is_some())
        {
            return;
        }
        self._html_navigation = Some(cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                if view
                    .update_in(cx, |view, window, cx| {
                        let Some(preview) = &view.preview else {
                            return;
                        };
                        if let Some(anchor) = view.html_anchor.take() {
                            preview.reveal_anchor(&anchor);
                        }
                        if let Some(href) = preview.take_navigation()
                            && let Some(open) = view.html_open.clone()
                            && let Ok(base) = url::Url::from_file_path(&view.path)
                        {
                            // Let the child update finish before opening another reader.
                            window.defer(cx, move |window, cx| {
                                super::markup::follow_link(&base, &open, &href, window, cx)
                            });
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
    fn load_html(&mut self) {
        use glim_services::preview::request::{NativeContent, NativeRequest};
        let request = NativeRequest::new(NativeContent::Html {
            source: self.path.clone(),
            text: self.html.clone(),
        });
        match request.encode() {
            Ok(configuration) => {
                self.preview = Some(super::native_bridge::NativePreview::new(&configuration));
                self.error = None;
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }
    pub fn reload_html_file(&mut self, cx: &mut Context<Self>) {
        if self.html_file && self.preview.is_some() {
            self.load_html();
            cx.notify();
        }
    }
    pub fn update_html(&mut self, text: String, cx: &mut Context<Self>) {
        if self.html.as_deref() == Some(text.as_str()) {
            return;
        }
        self.html = Some(text);
        if self.preview.is_some() {
            self.load_html();
        }
        cx.notify();
    }
    pub fn set_html_open(&mut self, open: super::OpenLink) {
        self.html_open = Some(open);
    }
    pub fn reveal_html_anchor(&mut self, anchor: &str) {
        self.html_anchor = Some(anchor.to_owned());
    }

    fn open_bytes(&mut self, cx: &mut Context<Self>) {
        if self.task.is_some() || self.folder {
            return;
        }
        self.hide();
        self.loading_bytes = true;
        let path = self.path.clone();
        let telemetry_path = path.clone();
        let telemetry_started = std::time::Instant::now();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |cancel| glim_services::paged::read_binary_page_cancellable(&path, 0, cancel),
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result(
                "native_load",
                Some(&telemetry_path),
                telemetry_started,
                &result,
            );
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                view.loading_bytes = false;
                match result {
                    Ok(page) => {
                        let parent = cx.entity().downgrade();
                        view.raw = Some(cx.new(|_| {
                            let mut reader = super::paged::PagedReader::readonly(page);
                            reader.return_to_preview = Some(std::rc::Rc::new(move |cx| {
                                let _ = parent.update(cx, |view, cx| {
                                    view.raw = None;
                                    cx.notify();
                                });
                            }));
                            reader
                        }))
                    }
                    Err(error) => {
                        glim_services::telemetry::operation_failed("native_load", None, &error);
                        view.error = Some(format!("{error:#}"));
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn can_move_to_window(&self, cx: &App) -> bool {
        self.pdf_refresh_task.is_none()
            && self
                .raw
                .as_ref()
                .is_none_or(|raw| raw.read(cx).can_move_to_window())
    }

    fn refresh_pdf(
        &mut self,
        on_failure: super::surface::RefreshFailureHandler,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(completion_rx) = self
            .preview
            .as_ref()
            .and_then(|preview| preview.start_pdf_reload())
        else {
            return false;
        };
        // Replacing this task drops the old receiver. Swift owns the sender until
        // completion or cancellation; its callback never accesses this reader.
        self.pdf_refresh_task = Some(cx.spawn(async move |view, cx| {
            let reload_result = completion_rx.await;
            let _ = view.update(cx, |view, cx| {
                view.pdf_refresh_task = None;
                if let Ok(Err(failure)) = reload_result
                    && let Some(kind) = failure.failure_kind()
                {
                    on_failure(kind, cx);
                }
            });
        }));
        true
    }

    pub fn image_stamp(&self) -> Option<(u64, Option<std::time::SystemTime>)> {
        self.prepared
            .as_ref()
            .and_then(|prepared| prepared.image_stamp)
    }
    pub fn new(path: PathBuf, folder: bool, shuffle: bool, cx: &mut Context<Self>) -> Self {
        Self::with_playback(path, folder, shuffle, true, cx)
    }
    pub fn new_paused(path: PathBuf, folder: bool, shuffle: bool, cx: &mut Context<Self>) -> Self {
        Self::with_playback(path, folder, shuffle, false, cx)
    }
    pub fn playlist_shuffle(&self) -> Option<bool> {
        self.folder.then_some(self.shuffle)
    }
    fn with_playback(
        path: PathBuf,
        folder: bool,
        shuffle: bool,
        autoplay: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            path,
            folder,
            shuffle,
            autoplay,
            task: None,
            pdf_refresh_task: None,
            preview: None,
            prepared: None,
            error: None,
            obscured: false,
            raw: None,
            loading_bytes: false,
            html: None,
            html_file: false,
            html_open: None,
            html_anchor: None,
            _html_navigation: None,
        };
        view.open(cx);
        view
    }
    pub(super) fn open(&mut self, cx: &mut Context<Self>) {
        if self.html.is_some() || self.html_file {
            if self.preview.is_none() {
                self.load_html();
                cx.notify();
            }
            return;
        }
        if self.task.is_some() || self.prepared.is_some() {
            return;
        }
        self.error = None;
        let path = self.path.clone();
        let folder = self.folder;
        let shuffle = self.shuffle;
        let autoplay = self.autoplay;
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let telemetry_path = path.clone();
        let telemetry_started = std::time::Instant::now();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Native,
            Priority::Interactive,
            cancel,
            move |cancel| {
                glim_services::preview::prepare_cancellable(
                    &path, folder, shuffle, autoplay, cancel,
                )
            },
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result(
                "native_load",
                Some(&telemetry_path),
                telemetry_started,
                &result,
            );
            let _ = view.update(cx, |v, cx| {
                v.task = None;
                match result {
                    Ok(prepared) => {
                        v.preview = Some(super::native_bridge::NativePreview::new(
                            &prepared.configuration,
                        ));
                        v.prepared = Some(prepared);
                    }
                    Err(e) => {
                        glim_services::telemetry::operation_failed("native_load", None, &e);
                        v.error = Some(format!("{e:#}"));
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn hide(&mut self) {
        self._html_navigation = None;
        if let Some(preview) = &self.preview {
            preview.hide();
        }
    }
    pub(super) fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if !active {
            self.task = None;
            self.hide();
            if let Some(raw) = &self.raw {
                raw.update(cx, |raw, _| raw.suspend());
            }
        } else if self.loading_bytes {
            self.open_bytes(cx);
        } else if let Some(raw) = &self.raw {
            raw.update(cx, |raw, cx| raw.resume(cx));
        } else {
            self.open(cx);
        }
    }
}
impl Render for NativeReader {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Native views sit above GPUI's Metal surface. Hide them while a toolkit
        // menu/dialog is open so the overlay remains visible and clickable.
        let view = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let obscured = window.has_active_dialog(cx)
                || window.has_active_sheet(cx)
                || window
                    .context_stack()
                    .iter()
                    .any(|context| context.contains("PopupMenu") || context.contains("Dialog"));
            let _ = view.update(cx, |view, cx| {
                if view.obscured != obscured {
                    view.obscured = obscured;
                    if obscured {
                        view.hide();
                    }
                    cx.notify();
                }
            });
        });
        if self.obscured {
            return div().size_full().into_any_element();
        }

        if self.loading_bytes {
            return super::preview_loading(cx);
        }
        if let Some(raw) = self.raw.clone() {
            return raw.into_any_element();
        }
        let bytes_available = self.html.is_none()
            && !self.html_file
            && !self.folder
            && !glim_services::preview::is_image(&self.path)
            && !glim_services::preview::is_iwork(&self.path);
        if self.preview.is_some() {
            self.start_html_navigation(window, cx);
            let view = cx.entity();
            let surface = canvas(
                move |bounds, window, cx| {
                    if let Some(preview) = &view.read(cx).preview {
                        preview.layout(bounds, window);
                    }
                },
                |_, _, _, _| {},
            )
            .size_full();
            return v_flex()
                .size_full()
                .min_h_0()
                .child(div().flex_1().min_h_0().w_full().child(surface))
                .into_any_element();
        }
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .p_6()
            .child(
                div().text_lg().child(
                    self.path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                ),
            )
            .when(self.task.is_some(), |view| {
                view.child(glim_services::locale::tr("Loading preview…"))
            })
            .when_some(self.error.clone(), |view, error| {
                view.child(div().text_sm().text_color(cx.theme().danger).child(error))
                    .child(
                        Button::new("retry-native-preview")
                            .primary()
                            .label(glim_services::locale::tr("Retry"))
                            .on_click(cx.listener(|v, _, _, cx| v.open(cx))),
                    )
            })
            .when(bytes_available && self.error.is_some(), |view| {
                view.child(
                    Button::new("failed-native-file-bytes")
                        .ghost()
                        .label(glim_services::locale::tr("File bytes"))
                        .on_click(cx.listener(|view, _, _, cx| view.open_bytes(cx))),
                )
            })
            .into_any_element()
    }
}

impl crate::views::reader::contract::ReaderView for super::surface::SurfaceEntity<NativeReader> {
    fn can_find(&self, cx: &App) -> bool {
        self.read(cx).find(false)
    }
    fn find(&self, _: &mut Window, cx: &mut App) {
        self.read(cx).find(true);
    }
    fn kind(&self) -> &'static str {
        "native"
    }
    fn render(&mut self, _cx: &mut App) -> AnyElement {
        self.0.clone().into_any_element()
    }
    fn can_move_to_window(&self, cx: &App) -> bool {
        self.read(cx).can_move_to_window(cx)
    }
    fn set_active(&self, active: bool, cx: &mut App) {
        self.update(cx, |view, cx| view.set_active(active, cx));
    }
    fn operations(&self) -> crate::views::reader::contract::ReaderOperations<'_> {
        crate::views::reader::contract::ReaderOperations {
            playback: Some(self),
            ..Default::default()
        }
    }
}
impl super::surface::ReaderSurface for super::surface::SurfaceEntity<NativeReader> {
    fn refresh_in_place(
        &self,
        content: &glim_services::documents::Content,
        on_failure: super::surface::RefreshFailureHandler,
        cx: &mut App,
    ) -> bool {
        let glim_services::documents::Content::Native(path, false, false) = content else {
            return false;
        };
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            return false;
        }
        self.update(cx, |view, cx| view.refresh_pdf(on_failure, cx))
    }

    fn refresh_request(
        &self,
        path: &std::path::Path,
        _force: bool,
        cx: &App,
    ) -> Option<glim_services::documents::refresh::RefreshRequest> {
        // PDFs share the workspace watcher, including atomic file replacements.
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            return Some(glim_services::documents::refresh::RefreshRequest::Reload(
                path.to_path_buf(),
            ));
        }
        self.read(cx).image_stamp().map(|stamp| {
            glim_services::documents::refresh::RefreshRequest::NativeImage {
                path: path.to_path_buf(),
                stamp,
            }
        })
    }
}
impl crate::views::reader::contract::PlaybackState for super::surface::SurfaceEntity<NativeReader> {
    fn playlist_shuffle(&self, cx: &App) -> Option<bool> {
        self.read(cx).playlist_shuffle()
    }
}
