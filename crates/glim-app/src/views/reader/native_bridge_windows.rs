use gpui_kit::{Bounds, Pixels, Window};
use std::{cell::RefCell, path::Path};
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

pub(super) struct NativePreview {
    content: serde_json::Value,
    view: RefCell<Option<WebView>>,
}

impl NativePreview {
    pub fn new(configuration: &[u8]) -> Self {
        let configuration: serde_json::Value =
            serde_json::from_slice(configuration).unwrap_or_default();
        Self {
            content: configuration["content"].clone(),
            view: RefCell::new(None),
        }
    }

    pub fn layout(&self, bounds: Bounds<Pixels>, window: &Window) {
        let bounds = Rect {
            position: LogicalPosition::new(f32::from(bounds.origin.x), f32::from(bounds.origin.y))
                .into(),
            size: LogicalSize::new(
                f32::from(bounds.size.width).max(0.0),
                f32::from(bounds.size.height).max(0.0),
            )
            .into(),
        };
        let mut view = self.view.borrow_mut();
        if let Some(view) = view.as_ref() {
            let _ = view.set_bounds(bounds);
            let _ = view.set_visible(true);
            return;
        }
        let kind = self.content["kind"].as_str().unwrap_or_default();
        let mut builder = WebViewBuilder::new()
            .with_bounds(bounds)
            .with_incognito(true)
            .with_devtools(false)
            .with_javascript_disabled()
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_download_started_handler(|_, _| false);
        let file = match kind {
            "image" => self.content["prepared"]
                .as_str()
                .or_else(|| self.content["source"].as_str()),
            "office" => self.content["reading"].as_str(),
            "pdf" | "html" => self.content["source"].as_str(),
            "media" => self.content["files"]
                .get(0)
                .and_then(serde_json::Value::as_str),
            _ => None,
        };
        let file_url = file.and_then(|file| url::Url::from_file_path(Path::new(file)).ok());
        let text = (kind == "html")
            .then(|| self.content["text"].as_str())
            .flatten();
        if let Some(text) = text {
            builder = builder
                .with_html(text)
                .with_navigation_handler(|url| url == "about:blank");
        } else if let Some(url) = file_url {
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
        match builder.build_as_child(window) {
            Ok(child) => *view = Some(child),
            Err(error) => log::warn!("Cannot open Windows document preview: {error}"),
        }
    }

    pub fn hide(&self) {
        if let Some(view) = self.view.borrow().as_ref() {
            let _ = view.set_visible(false);
        }
    }

    pub fn toggle_find(&self) {}
    pub fn update_find(&self, _open: bool, _focus: bool) {}
    pub fn search(&self, _query: &str, _sensitive: bool, _backwards: bool, _changed: bool) {}
    pub fn take_find_event(&self) -> Option<FindEvent> {
        None
    }
    pub fn find_overlay(&self, _bounds: Bounds<Pixels>, _radius: f32, _window: &Window) {}
    pub fn start_pdf_reload(
        &self,
    ) -> Option<futures_channel::oneshot::Receiver<Result<(), PdfReloadFailure>>> {
        None
    }
    pub fn find(&self, _show: bool) -> bool {
        false
    }
    pub fn take_navigation(&self) -> Option<String> {
        None
    }
    pub fn reveal_anchor(&self, _anchor: &str) {}
}
