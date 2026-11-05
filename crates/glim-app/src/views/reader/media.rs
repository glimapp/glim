use crate::app::reader_jobs::{self, JobClass, Priority};
use glim_services::documents::{ReadFailure, ReadFailureKind};
use glim_services::limits as reader_limits;
use gpui_kit::base::Disableable;
use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        h_flex, v_flex,
    },
    *,
};
use std::{path::Path, sync::Arc};

pub(super) struct ImageReader {
    pages: Vec<Entity<PreparedImage>>,
    page: Entity<usize>,
    _subscriptions: Vec<Subscription>,
}
impl ImageReader {
    pub fn new(pages: Vec<Vec<u8>>, cx: &mut App) -> Self {
        let pages: Vec<_> = pages
            .into_iter()
            .map(|bytes| cx.new(|cx| PreparedImage::new(bytes, cx)))
            .collect();
        let subscriptions = pages
            .iter()
            .map(|page| cx.observe(page, |_, cx| cx.refresh_windows()))
            .collect();
        Self {
            pages,
            page: cx.new(|_| 0),
            _subscriptions: subscriptions,
        }
    }
    pub fn render(&self, cx: &mut App) -> AnyElement {
        let page = *self.page.read(cx);
        for (index, image) in self.pages.iter().enumerate() {
            if index != page {
                image.update(cx, |image, cx| image.suspend(cx));
            }
        }
        let previous = self.page.clone();
        let next = self.page.clone();
        let count = self.pages.len();
        let retry = self.pages[page].clone();
        let failed = retry.read(cx).error.is_some();
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(
                // Give Contain a symmetric, definite viewport. Padding combined
                // with a percentage-sized image can offset its painting bounds.
                div()
                    .absolute()
                    .inset(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .child(
                        img(PreparedImage::source(&self.pages[page]))
                            .size_full()
                            .min_w_0()
                            .min_h_0()
                            .object_fit(ObjectFit::Contain),
                    ),
            )
            .child(if count > 1 {
                super::controls::floating(
                    "image-page-controls",
                    h_flex()
                        .gap_3()
                        .child(
                            crate::views::icon_button(
                                "image-previous-page",
                                gpui_kit::assets::IconName::ChevronLeft,
                                glim_services::locale::tr("Previous page"),
                            )
                            .disabled(page == 0)
                            .on_click(move |_, _, cx| {
                                previous.update(cx, |index, cx| {
                                    *index = index.saturating_sub(1);
                                    cx.refresh_windows();
                                })
                            }),
                        )
                        .child(format!("{} / {}", page + 1, count))
                        .child(
                            crate::views::icon_button(
                                "image-next-page",
                                gpui_kit::assets::IconName::ChevronRight,
                                glim_services::locale::tr("Next page"),
                            )
                            .disabled(page + 1 == count)
                            .on_click(move |_, _, cx| {
                                next.update(cx, |index, cx| {
                                    *index = (*index + 1).min(count - 1);
                                    cx.refresh_windows();
                                })
                            }),
                        ),
                    cx,
                )
                .into_any_element()
            } else {
                div().into_any_element()
            })
            .when(failed, |root| {
                root.child(
                    div().absolute().top(px(24.)).right(px(24.)).child(
                        Button::new("image-retry")
                            .label(glim_services::locale::tr("Retry"))
                            .on_click(move |_, _, cx| {
                                retry.update(cx, |view, cx| {
                                    view.error = None;
                                    view.request(cx);
                                    cx.notify();
                                })
                            }),
                    ),
                )
            })
            .into_any_element()
    }
}

struct ManagedFrame {
    image: Arc<RenderImage>,
    _lease: glim_core::resources::WorkingReservation,
}
impl ManagedFrame {
    fn new(frame: glim_services::media::RasterFrame) -> Self {
        let (frame, memory) = frame.into_bgra();
        Self {
            image: Arc::new(RenderImage::new(vec![frame])),
            _lease: memory,
        }
    }
}
fn release_image(frame: ManagedFrame, cx: &mut App) {
    // During render the current window is temporarily absent from App.windows.
    // Defer eviction until it has been reinserted so its atlas is cleared too.
    cx.defer(move |cx| {
        cx.drop_image(frame.image, None);
        drop(frame._lease);
    });
}

/// Encoded preview data and its on-demand raster have the same reader owner.
pub(super) struct PreparedImage {
    encoded: Arc<Image>,
    encoded_memory: glim_services::resources::ResourceLease,
    frame: Option<ManagedFrame>,
    task: Option<Task<()>>,
    error: Option<String>,
}
impl PreparedImage {
    pub fn new(bytes: Vec<u8>, cx: &mut Context<Self>) -> Self {
        cx.on_release(|view, cx| view.suspend(cx)).detach();
        Self {
            encoded_memory: glim_services::resources::pool().track(
                bytes.capacity() as u64,
                glim_services::resources::ResourceClass::Working,
            ),
            encoded: Arc::new(Image::from_bytes(ImageFormat::Png, bytes)),
            frame: None,
            task: None,
            error: None,
        }
    }
    pub fn source(entity: &Entity<Self>) -> ImageSource {
        let entity = entity.clone();
        ImageSource::Custom(Arc::new(move |_, cx| {
            entity.update(cx, |view, cx| {
                view.request(cx);
                view.frame
                    .as_ref()
                    .map(|frame| Ok(frame.image.clone()))
                    .or_else(|| {
                        view.error.as_ref().map(|error| {
                            Err(ImageCacheError::Other(Arc::new(anyhow::anyhow!(
                                error.clone()
                            ))))
                        })
                    })
            })
        }))
    }
    pub fn suspend(&mut self, cx: &mut App) {
        self.task = None;
        self.error = None;
        if let Some(frame) = self.frame.take() {
            release_image(frame, cx);
        }
    }
    fn request(&mut self, cx: &mut Context<Self>) {
        if self.frame.is_some() || self.task.is_some() || self.error.is_some() {
            return;
        }
        let encoded = self.encoded.clone();
        let input_memory = self.encoded_memory.clone();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Decode,
            Priority::Viewport,
            cancel,
            move |cancel| {
                let _input_memory = input_memory;
                glim_services::media::decode_png(encoded.bytes(), cancel).map(ManagedFrame::new)
            },
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                match result {
                    Ok(frame) => view.frame = Some(frame),
                    Err(error) => view.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        }));
    }
}

/// A bounded inline preview that releases pixels and texture when no longer requested.
pub(super) struct InlineImageReader {
    url: url::Url,
    frame: Option<ManagedFrame>,
    error: Option<String>,
    active: bool,
    task: Option<Task<()>>,
    refresh_task: Option<Task<()>>,
    file_stamp: Option<(u64, Option<std::time::SystemTime>)>,
}

impl InlineImageReader {
    pub fn new(url: url::Url, cx: &mut Context<Self>) -> Self {
        cx.on_release(|view, cx| {
            view.task = None;
            view.refresh_task = None;
            if let Some(frame) = view.frame.take() {
                release_image(frame, cx);
            }
        })
        .detach();
        Self {
            url,
            frame: None,
            error: None,
            active: false,
            task: None,
            refresh_task: None,
            file_stamp: None,
        }
    }

    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        if !active {
            self.task = None;
            self.refresh_task = None;
            self.error = None;
            if let Some(frame) = self.frame.take() {
                release_image(frame, cx);
            }
            cx.notify();
            return;
        }
        self.start_load(cx);
    }

    // Workspace change notifications also cover referenced local image files.
    // Compare metadata on a worker; retain the current frame until its replacement is ready.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.active && self.error.is_some() && self.task.is_none() {
            self.start_load(cx);
            return;
        }
        if !self.active
            || self.task.is_some()
            || self.refresh_task.is_some()
            || self.url.scheme() != "file"
        {
            return;
        }
        let Ok(path) = self.url.to_file_path() else {
            return;
        };
        let work = cx.background_executor().spawn(async move {
            std::fs::metadata(path)
                .ok()
                .map(|m| (m.len(), m.modified().ok()))
        });
        self.refresh_task = Some(cx.spawn(async move |view, cx| {
            let stamp = work.await;
            let _ = view.update(cx, |view, cx| {
                view.refresh_task = None;
                if view.active && stamp != view.file_stamp {
                    view.start_load(cx);
                }
            });
        }));
    }

    fn start_load(&mut self, cx: &mut Context<Self>) {
        let url = self.url.clone();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Decode,
            Priority::Viewport,
            cancel,
            move |cancel| {
                let stamp = url
                    .to_file_path()
                    .ok()
                    .and_then(|path| std::fs::metadata(path).ok())
                    .map(|m| (m.len(), m.modified().ok()));
                let frame = match url.scheme() {
                    "file" => glim_services::inline_images::from_file_cancellable(
                        &url.to_file_path()
                            .map_err(|_| anyhow::anyhow!("Invalid inline image path"))?,
                        cancel,
                    ),
                    "http" | "https" => {
                        glim_services::markup_images::fetch_inline_image_cancellable(
                            url.as_str(),
                            cancel,
                        )
                    }
                    _ => anyhow::bail!("Unsupported inline image URL"),
                }?;
                Ok::<_, anyhow::Error>((ManagedFrame::new(frame), stamp))
            },
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                match result {
                    Ok((frame, stamp)) => {
                        if let Some(old) = view.frame.replace(frame) {
                            release_image(old, cx);
                        }
                        view.file_stamp = stamp;
                        view.error = None;
                        view.refresh(cx);
                    }
                    Err(error) => {
                        if let Some(old) = view.frame.take() {
                            release_image(old, cx);
                        }
                        view.error = Some(format!("{error:#}"));
                    }
                }
                cx.notify();
            });
        }));
    }

    pub fn image(&self) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.frame
            .as_ref()
            .map(|frame| Ok(frame.image.clone()))
            .or_else(|| {
                self.error.as_ref().map(|error| {
                    Err(ImageCacheError::Other(Arc::new(anyhow::anyhow!(
                        error.clone()
                    ))))
                })
            })
    }
}

enum GifSource {
    File(std::path::PathBuf),
    Url(url::Url),
}
/// Keeps one displayed frame and two small, byte-bounded lookahead batches.
/// Hidden animations release decoded pixels; disk-backed input can be replayed on activation.
pub(super) struct GifReader {
    source: GifSource,
    frame: Option<ManagedFrame>,
    error: Option<String>,
    input: Option<glim_services::media::GifInput>,
    active: bool,
    load_task: Option<Task<()>>,
    task: Option<Task<()>>,
}
impl GifReader {
    pub fn from_file(path: std::path::PathBuf, cx: &mut Context<Self>) -> Self {
        Self::new(GifSource::File(path), cx)
    }
    pub fn from_url(url: url::Url, cx: &mut Context<Self>) -> Self {
        Self::new(GifSource::Url(url), cx)
    }
    fn new(source: GifSource, cx: &mut Context<Self>) -> Self {
        cx.on_release(|view, cx| {
            view.load_task = None;
            view.task = None;
            if let Some(frame) = view.frame.take() {
                release_image(frame, cx);
            }
        })
        .detach();
        Self {
            source,
            frame: None,
            error: None,
            input: None,
            active: false,
            load_task: None,
            task: None,
        }
    }

    fn start_load(&mut self, cx: &mut Context<Self>) {
        if self.input.is_some() {
            self.start_playback(cx);
            return;
        }
        let url = match &self.source {
            GifSource::File(path) => {
                self.input = Some(glim_services::media::GifInput::File(path.clone()));
                self.start_playback(cx);
                return;
            }
            GifSource::Url(url) => url.clone(),
        };
        self.error = None;
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let data = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Decode,
            Priority::Viewport,
            cancel,
            move |cancel| match url.scheme() {
                "http" | "https" => {
                    glim_services::markup_images::fetch_image_file(url.as_str(), cancel)
                        .map(|file| glim_services::media::GifInput::Temporary(Arc::new(file)))
                }
                "file" => Ok(glim_services::media::GifInput::File(
                    url.to_file_path()
                        .map_err(|_| anyhow::anyhow!("Invalid image path"))?,
                )),
                _ => anyhow::bail!("Unsupported GIF URL"),
            },
        );
        self.load_task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = data.await;
            let _ = view.update(cx, |view, cx| {
                view.load_task = None;
                match result {
                    Ok(input) => {
                        view.input = Some(input);
                        if view.active {
                            view.start_playback(cx);
                        }
                    }
                    Err(error) => view.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        }));
    }

    fn start_playback(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.input.clone() else {
            return;
        };
        let executor = cx.background_executor().clone();
        self.error = None;
        let (cancel, guard) = glim_services::work::Cancellation::task();
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = async {
                let playback = reader_jobs::run(
                    executor.clone(),
                    JobClass::Decode,
                    Priority::Viewport,
                    cancel.clone(),
                    move |cancel| {
                        glim_services::media::GifPlayback::from_input_cancellable(
                            input,
                            cancel.clone(),
                        )
                    },
                )
                .await?;
                // Each batch is at most 16 MiB and three frames. Start preparing
                // the next batch before displaying this one, keeping decode work
                // independent from frame presentation without unbounded buffering.
                let batch_size = (reader_limits::GIF_BATCH_BYTES / playback.preview_frame_bytes())
                    .clamp(1, reader_limits::GIF_BATCH_FRAMES);
                let prepare = |mut playback: glim_services::media::GifPlayback| {
                    reader_jobs::spawn(
                        executor.clone(),
                        JobClass::Decode,
                        Priority::Viewport,
                        cancel.clone(),
                        move |_cancel| {
                            let mut frames = std::collections::VecDeque::new();
                            let result = (|| {
                                for _ in 0..batch_size {
                                    let next = match playback.next_preview_frame() {
                                        Err(error)
                                            if error
                                                .is::<glim_core::resources::WorkingPressure>()
                                                && !frames.is_empty() =>
                                        {
                                            break;
                                        }
                                        next => next?,
                                    };
                                    let Some((frame, delay)) = next else {
                                        break;
                                    };
                                    frames.push_back((ManagedFrame::new(frame), delay));
                                }
                                Ok::<_, anyhow::Error>(frames)
                            })();
                            result.map(|frames| (playback, frames))
                        },
                    )
                };
                let mut pending = prepare(playback);
                let mut deadline = std::time::Instant::now();
                let mut started = false;
                loop {
                    let (decoder, mut frames) = pending.await?;
                    if frames.is_empty() {
                        break;
                    }
                    pending = prepare(decoder);
                    if !started {
                        deadline = std::time::Instant::now();
                        started = true;
                    }
                    while let Some((frame, delay)) = frames.pop_front() {
                        // Absolute deadlines avoid accumulating executor/UI overhead.
                        // Expired intermediate frames are decoded for disposal but
                        // never uploaded, so a slow frame does not slow the animation.
                        let end = deadline + delay;
                        if end > std::time::Instant::now() || frames.is_empty() {
                            cx.background_executor()
                                .timer(
                                    deadline.saturating_duration_since(std::time::Instant::now()),
                                )
                                .await;
                            view.update(cx, |view, cx| {
                                if let Some(previous) = view.frame.replace(frame) {
                                    release_image(previous, cx);
                                }
                                cx.notify();
                            })?;
                        }
                        deadline = end;
                    }
                }
                cx.background_executor()
                    .timer(deadline.saturating_duration_since(std::time::Instant::now()))
                    .await;
                Ok::<(), anyhow::Error>(())
            }
            .await;
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                match result {
                    Ok(()) => {}
                    Err(error) => view.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        }));
    }

    pub fn image(&self) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        if let Some(frame) = &self.frame {
            Some(Ok(frame.image.clone()))
        } else {
            self.error.as_ref().map(|error| {
                Err(ImageCacheError::Other(Arc::new(anyhow::anyhow!(
                    error.clone()
                ))))
            })
        }
    }

    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        if active {
            self.start_load(cx);
        } else {
            self.task = None;
            self.load_task = None;
            self.error = None;
            if let Some(frame) = self.frame.take() {
                release_image(frame, cx);
            }
            cx.notify();
        }
    }
}
impl Render for GifReader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = div().relative().size_full().overflow_hidden();
        if let Some(frame) = &self.frame {
            content = content.child(
                div().absolute().inset(px(24.)).overflow_hidden().child(
                    img(frame.image.clone())
                        .size_full()
                        .min_w_0()
                        .min_h_0()
                        .object_fit(ObjectFit::Contain),
                ),
            );
        }
        if let Some(error) = &self.error {
            content = content.child(
                div().absolute().bottom(px(8.)).left(px(24.)).child(
                    h_flex().gap_3().child(error.clone()).child(
                        Button::new("gif-retry")
                            .label(glim_services::locale::tr("Retry"))
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.task = None;
                                view.load_task = None;
                                if let Some(frame) = view.frame.take() {
                                    release_image(frame, cx);
                                }
                                view.start_load(cx);
                                cx.notify();
                            })),
                    ),
                ),
            );
        }
        content
    }
}

pub(super) struct UnavailableReader {
    issue_url: String,
    reason: ReadFailure,
    path: std::path::PathBuf,
}
impl UnavailableReader {
    pub fn new(path: &Path, reason: ReadFailure) -> Self {
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .or_else(|| path.file_name().and_then(|s| s.to_str()))
            .unwrap_or("unknown");
        let mut url = url::Url::parse("https://github.com/glimapp/glim/issues/new").unwrap();
        url.query_pairs_mut().append_pair("title", &format!("Unsupported file format: {extension}"))
            .append_pair("body", &format!("Format: {extension}\nGlim: {}\nPlatform: {} / {}\n\nWhat happened:\nCould not display this file.\n\nExpected behavior:\n\nAdditional details (optional):\n", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH));
        Self {
            issue_url: url.into(),
            reason,
            path: path.to_path_buf(),
        }
    }
    pub fn render(&self, cx: &mut App) -> AnyElement {
        use crate::app::actions::{CloseWindow, Refresh};
        use glim_services::locale::tr;
        use gpui_kit::component::{
            Sizable, button::ButtonCustomVariant, clipboard::Clipboard, tooltip::Tooltip,
        };
        let accent = crate::views::accent(cx);
        let (title, explanation) = match self.reason.kind {
            ReadFailureKind::NotFound => (
                "File not found",
                Some("Check the file location or reconnect its drive."),
            ),
            ReadFailureKind::PermissionDenied => (
                "Permission needed",
                Some("Check read permissions in Finder, then retry."),
            ),
            ReadFailureKind::Other => ("Cannot open this file", None),
        };
        let url = self.issue_url.clone();
        let detail = self.reason.detail.clone();
        let path = self.path.display().to_string();
        let path_hint = path.clone();
        let filename = self
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let filename_hint = filename.clone();
        v_flex()
            .id("unavailable-reader")
            .size_full()
            .overflow_y_scroll()
            .items_center()
            .p_6()
            .child(
                v_flex()
                    .w_full()
                    .my_auto()
                    .flex_shrink_0()
                    .max_w(px(380.))
                    .gap_4()
                    .font_family(cx.theme().font_family.clone())
                    .text_color(cx.theme().foreground)
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr(title)),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .gap_3()
                            .child(crate::views::file_icons::file_icon(&self.path, cx))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1()
                                    .child(
                                        div()
                                            .id("unavailable-filename")
                                            .truncate()
                                            .text_size(px(14.))
                                            .child(filename)
                                            .tooltip(move |window, cx| {
                                                Tooltip::new(filename_hint.clone())
                                                    .build(window, cx)
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id("unavailable-path")
                                            .truncate()
                                            .text_size(px(12.))
                                            .text_color(cx.theme().muted_foreground)
                                            .child(
                                                self.path
                                                    .parent()
                                                    .unwrap_or(Path::new(""))
                                                    .display()
                                                    .to_string(),
                                            )
                                            .tooltip(move |window, cx| {
                                                Tooltip::new(path_hint.clone()).build(window, cx)
                                            }),
                                    ),
                            )
                            .child(
                                Clipboard::new("copy-unavailable-path")
                                    .with_size(px(28.))
                                    .tooltip(tr("Copy path"))
                                    .accessibility_label(tr("Copy path"))
                                    .value(path),
                            ),
                    )
                    .when_some(explanation, |view, explanation| {
                        view.child(
                            div()
                                .text_size(px(13.))
                                .whitespace_normal()
                                .text_color(cx.theme().muted_foreground)
                                .child(tr(explanation)),
                        )
                    })
                    .child(
                        h_flex()
                            .w_full()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                Button::new("retry-unavailable")
                                    .custom(
                                        ButtonCustomVariant::new(cx)
                                            .color(accent)
                                            .foreground(cx.theme().background)
                                            .hover(accent.opacity(0.9))
                                            .active(accent.opacity(0.8)),
                                    )
                                    .small()
                                    .h(px(30.))
                                    .px_3()
                                    .label(tr("Retry"))
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(Refresh), cx)
                                    }),
                            )
                            .child(
                                Button::new("close-unavailable")
                                    .small()
                                    .h(px(30.))
                                    .px_3()
                                    .label(tr("Close tab"))
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(CloseWindow), cx)
                                    }),
                            )
                            .child(div().flex_1())
                            .when(self.reason.kind == ReadFailureKind::Other, |row| {
                                row.child(
                                    Clipboard::new("copy-read-error")
                                        .with_size(px(28.))
                                        .tooltip(tr("Copy error details"))
                                        .accessibility_label(tr("Copy error details"))
                                        .value(detail),
                                )
                                .child(
                                    crate::views::icon_button(
                                        "unsupported-issue",
                                        gpui_kit::assets::IconName::ArrowUpRight,
                                        tr("Report an issue on GitHub"),
                                    )
                                    .ghost()
                                    .on_click(move |_, _, cx| cx.open_url(&url)),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }
}

impl crate::views::reader::contract::ReaderView for ImageReader {
    fn kind(&self) -> &'static str {
        "image"
    }
    fn render(&mut self, cx: &mut App) -> AnyElement {
        ImageReader::render(self, cx)
    }
    fn operations(&self) -> crate::views::reader::contract::ReaderOperations<'_> {
        crate::views::reader::contract::ReaderOperations {
            image_copy: Some(self),
            ..Default::default()
        }
    }
    fn set_active(&self, active: bool, cx: &mut App) {
        if !active {
            for image in &self.pages {
                image.update(cx, |image, cx| image.suspend(cx));
            }
        }
    }
}
impl super::surface::ReaderSurface for ImageReader {}
impl crate::views::reader::contract::CopyImage for ImageReader {
    fn copy_image(&self, cx: &mut App) -> bool {
        if let Some(page) = self.pages.get(*self.page.read(cx)) {
            cx.write_to_clipboard(ClipboardItem::new_image(&page.read(cx).encoded));
            true
        } else {
            false
        }
    }
}
impl crate::views::reader::contract::ReaderView for super::surface::SurfaceEntity<GifReader> {
    fn kind(&self) -> &'static str {
        "animated_image"
    }
    fn render(&mut self, cx: &mut App) -> AnyElement {
        self.update(cx, |view, cx| view.set_active(true, cx));
        self.0.clone().into_any_element()
    }
    fn set_active(&self, active: bool, cx: &mut App) {
        self.update(cx, |view, cx| view.set_active(active, cx));
    }
}
impl super::surface::ReaderSurface for super::surface::SurfaceEntity<GifReader> {}
impl crate::views::reader::contract::ReaderView for UnavailableReader {
    fn kind(&self) -> &'static str {
        "unavailable"
    }
    fn render(&mut self, cx: &mut App) -> AnyElement {
        UnavailableReader::render(self, cx)
    }
    fn is_unavailable(&self) -> bool {
        true
    }
}
impl super::surface::ReaderSurface for UnavailableReader {}

#[cfg(test)]
mod tests {
    use super::UnavailableReader;
    #[test]
    fn dotfiles_and_extensionless_files_use_their_actual_names() {
        for name in [".DS_Store", ".env", "README"] {
            let issue = UnavailableReader::new(
                &std::path::Path::new("/private/project").join(name),
                "unsupported".to_string().into(),
            );
            let url = url::Url::parse(&issue.issue_url).unwrap();
            let query = url
                .query_pairs()
                .collect::<std::collections::HashMap<_, _>>();
            assert_eq!(query["title"], format!("Unsupported file format: {name}"));
            assert!(query["body"].contains(&format!("Format: {name}")));
            assert!(!issue.issue_url.contains("private"));
            assert!(!issue.issue_url.contains("no+extension"));
        }
    }
    #[test]
    fn issue_draft_contains_format_and_version_but_not_private_path() {
        let issue = UnavailableReader::new(
            std::path::Path::new("/private/project/customer.pdf"),
            "Could not decode /private/project/customer.pdf"
                .to_string()
                .into(),
        );
        let url = url::Url::parse(&issue.issue_url).unwrap();
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert!(query["body"].contains("Format: pdf"));
        assert!(query["body"].contains(env!("CARGO_PKG_VERSION")));
        assert!(!issue.issue_url.contains("customer"));
        assert!(!issue.issue_url.contains("private"));
    }
}
