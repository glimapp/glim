#![allow(unsafe_code)]

#[path = "windows_find.rs"]
mod find;
#[path = "windows_html.rs"]
mod html;

use gpui_kit::{App, BackgroundExecutor, Bounds, ForegroundExecutor, Pixels, Window};
use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use std::{
    cell::{Cell, OnceCell, RefCell},
    path::Path,
    rc::Rc,
};
use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
use wry::{
    Rect, WebView, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize},
};

pub(crate) fn set_telemetry_consent(_consent: glim_services::telemetry::Consent) {}
pub(crate) fn begin_native_preview_shutdown() {}

pub(super) enum PdfReloadFailure {}

impl PdfReloadFailure {
    pub fn failure_kind(self) -> Option<glim_services::documents::ReadFailureKind> {
        match self {}
    }
}

pub(super) struct FindEvent {
    pub command: i32,
    pub open: bool,
    pub label: String,
    pub found: bool,
    pub searching: bool,
}

#[derive(Clone, Copy, PartialEq)]
struct Placement {
    parent: Win32WindowHandle,
    bounds: Rect,
}

struct PreviewParent(Win32WindowHandle);

impl HasWindowHandle for PreviewParent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: WebView creation uses this wrapper on the parent window's thread.
        // The owning preview supplies the handle, and the window is checked below.
        unsafe { Ok(WindowHandle::borrow_raw(RawWindowHandle::Win32(self.0))) }
    }
}

struct PreviewState {
    content: serde_json::Value,
    executors: OnceCell<(ForegroundExecutor, BackgroundExecutor)>,
    navigation: Rc<RefCell<Option<String>>>,
    anchor: RefCell<Option<String>>,
    find: RefCell<Option<find::Session>>,
    desired: Cell<Option<Placement>>,
    scheduled: Cell<bool>,
    view: RefCell<Option<(Win32WindowHandle, WebView)>>,
}

impl PreviewState {
    fn request(self: &Rc<Self>, placement: Option<Placement>) {
        if self.desired.replace(placement) != placement {
            self.schedule();
        }
    }

    fn schedule(self: &Rc<Self>) {
        let Some((executor, _)) = self.executors.get() else {
            return;
        };
        if self.scheduled.replace(true) {
            return;
        }
        let preview = Rc::downgrade(self);
        // WebView2 creation pumps Win32 messages. Give GPUI back its App borrow
        // before any of those messages can cause input or another frame.
        executor
            .spawn(async move {
                let Some(preview) = preview.upgrade() else {
                    return;
                };
                let placement = preview.desired.get();
                preview.apply(placement);
                preview.scheduled.set(false);
                if preview.desired.get() != placement {
                    preview.schedule();
                }
            })
            .detach();
    }

    fn apply(&self, placement: Option<Placement>) {
        let mut child = self.view.take();
        if let Some(placement) = placement {
            if child
                .as_ref()
                .is_some_and(|(parent, _)| *parent != placement.parent)
            {
                child = None;
                self.find.take();
            }
            if let Some((_, view)) = &child {
                let _ = view.set_bounds(placement.bounds);
                let _ = view.set_visible(self.desired.get().is_some());
            } else {
                // SAFETY: this queries the liveness of a Win32 window on its UI thread.
                if unsafe { IsWindow(placement.parent.hwnd.get() as _) } != 0 {
                    match self.build(placement) {
                        Ok(view) => {
                            if self.content["kind"] == "html"
                                || self.content["kind"] == "office"
                                    && self.content["reading"].is_string()
                            {
                                self.find.replace(find::Session::new(&view).ok());
                            }
                            let _ = view.set_visible(self.desired.get().is_some());
                            child = Some((placement.parent, view));
                        }
                        Err(error) => log::warn!("Cannot open Windows document preview: {error}"),
                    }
                }
            }
        } else if let Some((_, view)) = &child {
            let _ = view.set_visible(false);
        }
        self.view.replace(child);
    }

    fn build(&self, placement: Placement) -> wry::Result<WebView> {
        let kind = self.content["kind"].as_str().unwrap_or_default();
        let mut builder = WebViewBuilder::new()
            .with_bounds(placement.bounds)
            .with_incognito(true)
            .with_devtools(false)
            .with_download_started_handler(|_, _| false);
        if kind == "html" {
            let (foreground, background) = self.executors.get().expect("preview has a placement");
            let view = builder.build_as_child(&PreviewParent(placement.parent))?;
            html::load(
                &view,
                Path::new(self.content["source"].as_str().unwrap_or_default()),
                self.content["text"].as_str(),
                self.anchor.borrow().as_deref(),
                self.navigation.clone(),
                foreground.clone(),
                background.clone(),
            )?;
            return Ok(view);
        }
        builder = builder
            .with_javascript_disabled()
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny);
        let file = match kind {
            "image" => self.content["prepared"]
                .as_str()
                .or_else(|| self.content["source"].as_str()),
            "office" => self.content["reading"].as_str(),
            "pdf" => self.content["source"].as_str(),
            "media" => self.content["files"]
                .get(0)
                .and_then(serde_json::Value::as_str),
            _ => None,
        };
        let file_url = file.and_then(|file| url::Url::from_file_path(Path::new(file)).ok());
        if let Some(url) = file_url {
            let mut allowed = url.clone();
            allowed.set_fragment(None);
            builder = builder
                .with_url(url.as_str())
                .with_navigation_handler(move |target| {
                    url::Url::parse(&target).is_ok_and(|mut target| {
                        target.set_fragment(None);
                        target == allowed
                    })
                });
        } else {
            builder = builder
                .with_html("<p>Embedded preview is unavailable for this format on Windows.</p>");
        }
        builder.build_as_child(&PreviewParent(placement.parent))
    }
}

pub(super) struct NativePreview(Rc<PreviewState>);

impl NativePreview {
    pub fn new(configuration: &[u8]) -> Self {
        let configuration: serde_json::Value =
            serde_json::from_slice(configuration).unwrap_or_default();
        Self(Rc::new(PreviewState {
            content: configuration["content"].clone(),
            executors: OnceCell::new(),
            navigation: Rc::default(),
            anchor: RefCell::default(),
            find: RefCell::default(),
            desired: Cell::new(None),
            scheduled: Cell::new(false),
            view: RefCell::new(None),
        }))
    }

    pub fn layout(&self, bounds: Bounds<Pixels>, window: &Window, cx: &App) {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::Win32(parent) = handle.as_raw() else {
            return;
        };
        let bounds = Rect {
            position: LogicalPosition::new(f32::from(bounds.origin.x), f32::from(bounds.origin.y))
                .into(),
            size: LogicalSize::new(
                f32::from(bounds.size.width).max(0.0),
                f32::from(bounds.size.height).max(0.0),
            )
            .into(),
        };
        self.0.executors.get_or_init(|| {
            (
                cx.foreground_executor().clone(),
                cx.background_executor().clone(),
            )
        });
        self.0.request(Some(Placement { parent, bounds }));
    }

    pub fn hide(&self) {
        self.0.request(None);
    }

    pub fn toggle_find(&self) {
        if let Some(session) = &*self.0.find.borrow() {
            session.command.set(if session.open.get() { 2 } else { 1 });
        }
    }
    pub fn update_find(&self, open: bool, focus: bool) {
        if let Some(session) = &*self.0.find.borrow() {
            session.set_open(open);
            if focus && let Some((_, view)) = &*self.0.view.borrow() {
                if open {
                    let _ = view.focus_parent();
                } else {
                    let _ = view.focus();
                }
            }
        }
    }
    pub fn search(&self, query: &str, sensitive: bool, backwards: bool, changed: bool) {
        if let Some(session) = &*self.0.find.borrow() {
            session.search(query, sensitive, backwards, changed);
        }
    }
    pub fn take_find_event(&self) -> Option<FindEvent> {
        self.0.find.borrow().as_ref()?.event()
    }
    pub fn find_overlay(&self, _bounds: Bounds<Pixels>, _radius: f32, _window: &Window) {}
    pub fn start_pdf_reload(
        &self,
    ) -> Option<futures_channel::oneshot::Receiver<Result<(), PdfReloadFailure>>> {
        None
    }
    pub fn find(&self, show: bool) -> bool {
        let session = self.0.find.borrow();
        let Some(session) = session.as_ref() else {
            return false;
        };
        if show {
            session.command.set(1);
        }
        true
    }
    pub fn take_navigation(&self) -> Option<String> {
        self.0.navigation.take()
    }
    pub fn reveal_anchor(&self, anchor: &str) {
        self.0.anchor.replace(Some(anchor.into()));
        if let Some((_, view)) = &*self.0.view.borrow() {
            let id = serde_json::Value::String(anchor.to_owned());
            let _ = view.evaluate_script(&format!("location.hash = encodeURIComponent({id});"));
        }
    }
}
