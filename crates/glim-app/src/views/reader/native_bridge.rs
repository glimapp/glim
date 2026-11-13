//! Main-thread ownership boundary for AppKit views, plus synchronized numeric
//! telemetry preference and reader resource accounting.
#![allow(unsafe_code)]

use gpui_kit::{Bounds, Pixels, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc};

// Native owners retain numeric handles, never Rust pointers. The registry may
// be called by decode/OCR workers as well as the main-thread preview controller.
extern "C" fn reader_resource(id: u64, bytes: u64, admit: bool) -> u64 {
    use glim_services::resources::{self, ResourceClass, ResourceLease, WorkingReservation};
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
    };
    enum NativeResource {
        Tracked { _lease: ResourceLease },
        Admitted { _reservation: WorkingReservation },
    }
    struct Registry {
        next: u64,
        leases: HashMap<u64, NativeResource>,
    }
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    let mut registry = REGISTRY
        .get_or_init(|| {
            Mutex::new(Registry {
                next: 0,
                leases: HashMap::new(),
            })
        })
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if id != 0 {
        registry.leases.remove(&id);
        return 0;
    }
    let Some(id) = registry.next.checked_add(1) else {
        return 0;
    };
    let resource = if admit {
        let Ok(reservation) = resources::pool().reserve_working(bytes, bytes) else {
            return 0;
        };
        NativeResource::Admitted {
            _reservation: reservation,
        }
    } else {
        NativeResource::Tracked {
            _lease: resources::pool().track(bytes, ResourceClass::Working),
        }
    };
    registry.next = id;
    registry.leases.insert(id, resource);
    id
}

extern "C" fn reader_telemetry(
    kind: i32,
    action: i32,
    error: i32,
    source: i32,
    value: u64,
    secondary: u64,
    count: u64,
    duration: u64,
) {
    use glim_services::telemetry::{self, number, text};
    let shortcut = action >= 100;
    let action = if shortcut { action - 100 } else { action };
    let kind = match kind {
        1 => "image",
        2 => "pdf",
        3 => "word",
        4 => "workbook",
        5 => "media",
        6 => "presentation",
        7 => "html",
        8 => "font",
        0 => "native",
        _ => return,
    };
    let action = match action {
        0 => "load",
        1 => "zoom_in",
        2 => "zoom_out",
        3 => "zoom_reset",
        4 => "fit",
        5 => "next",
        6 => "previous",
        7 => "copy",
        8 => "extract_text",
        9 => "toggle_mode",
        10 => "play_pause",
        11 => "open_external",
        12 => "layout",
        13 => "slideshow_start",
        14 => "slideshow_exit",
        15 => "select_sheet",
        16 => "select_font",
        17 => "select_all",
        18 => "previous_columns",
        19 => "next_columns",
        20 => "copy_selection",
        21 => "load_more_sheets",
        22 => "playhead_jump_observed",
        23 => "volume",
        24 => "ocr_completed",
        25 => "copy_completed",
        26 => "load_completed",
        27 => "zoom_changed",
        28 => "open_link",
        29 => "text_selection",
        30 => "scroll",
        31 => "context_menu_opened",
        32 => "scroll_lines",
        33 => "page_position_observed",
        34 => "playback_position_observed",
        35 => "playback_state_observed",
        36 => "mute_state_observed",
        37 => "find_opened",
        38 => "pan",
        39 => "actual_size",
        40 => "fit_width",
        41 => "fit_page",
        42 => "fit_image",
        43 => "find_next",
        44 => "find_previous",
        45 => "find_closed",
        46 => "find_case_changed",
        _ => return,
    };
    let source = match source {
        1 => "pointer",
        2 => "keyboard",
        3 => "menu",
        _ => "unknown",
    };
    if shortcut {
        let token = telemetry::native_shortcut_token(count);
        let mut props = telemetry::shortcut_properties(
            token.unwrap_or_else(|| telemetry::native_shortcut_key(secondary)),
            value & (1 << 20) != 0,
            value & (1 << 18) != 0,
            value & (1 << 19) != 0,
            value & (1 << 17) != 0,
        );
        props.extend([
            ("reader_kind", text(kind)),
            ("action", text(action)),
            ("key_code", number(secondary)),
            (
                "key_basis",
                text(if token.is_some() {
                    "recognized_command"
                } else {
                    "macos_virtual_key_us"
                }),
            ),
        ]);
        telemetry::track("native_shortcut_used", props);
    } else if matches!(action, "scroll" | "scroll_lines") {
        telemetry::track(
            "reader_scroll",
            [
                ("reader_kind", text(kind)),
                ("action", text(action)),
                ("input_source", text(source)),
                ("capture", text("native_document_input")),
                (
                    "delta_unit",
                    text(if action == "scroll_lines" {
                        "lines"
                    } else {
                        "points"
                    }),
                ),
                ("horizontal_delta_x100", number(value)),
                ("vertical_delta_x100", number(secondary)),
                ("input_events", number(count)),
                ("duration_ms", number(duration)),
            ],
        );
    } else {
        telemetry::native_event(kind, action, error.max(0) as u32, source, value, secondary);
    }
}
pub(crate) fn set_telemetry_consent(consent: glim_services::telemetry::Consent) {
    // SAFETY: numeric-only synchronized Swift preference; no retained arguments.
    unsafe {
        glim_set_telemetry_consent(i32::from(consent.usage), i32::from(consent.diagnostics));
    }
}
// Stable C ABI values are decoded only here; app code consumes typed results.
#[derive(Clone, Copy)]
pub(super) enum PdfReloadFailure {
    Cancelled,
    NotFound,
    PermissionDenied,
    InvalidDocument,
    Locked,
}
impl PdfReloadFailure {
    pub fn failure_kind(self) -> Option<glim_services::documents::ReadFailureKind> {
        use glim_services::documents::ReadFailureKind;
        match self {
            Self::Cancelled => None,
            Self::NotFound => Some(ReadFailureKind::NotFound),
            Self::PermissionDenied => Some(ReadFailureKind::PermissionDenied),
            Self::InvalidDocument | Self::Locked => Some(ReadFailureKind::Other),
        }
    }
}
type PdfReloadResult = Result<(), PdfReloadFailure>;
type PdfReloadSender = futures_channel::oneshot::Sender<PdfReloadResult>;

extern "C" fn pdf_reload_complete(context: *mut c_void, status: i32) {
    // SAFETY: accepted contexts are consumed exactly once by Swift completion,
    // supersession, or close. They contain only a sender, never a GPUI pointer.
    let sender = *unsafe { Box::from_raw(context.cast::<PdfReloadSender>()) };
    let result = match status {
        0 => Ok(()),
        1 => Err(PdfReloadFailure::Cancelled),
        2 => Err(PdfReloadFailure::NotFound),
        3 => Err(PdfReloadFailure::PermissionDenied),
        5 => Err(PdfReloadFailure::Locked),
        _ => Err(PdfReloadFailure::InvalidDocument),
    };
    let _ = sender.send(result);
}

unsafe extern "C" {
    fn glim_set_reader_resources(callback: Option<extern "C" fn(u64, u64, bool) -> u64>);
    fn glim_set_telemetry_consent(usage: i32, diagnostics: i32);
    fn glim_set_reader_telemetry(
        callback: Option<extern "C" fn(i32, i32, i32, i32, u64, u64, u64, u64)>,
    );
    fn glim_preview_create(bytes: *const u8, length: usize) -> *mut c_void;
    fn glim_preview_layout(
        handle: *mut c_void,
        parent: *mut c_void,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    );
    fn glim_preview_hide(handle: *mut c_void);
    fn glim_preview_reload_pdf(
        handle: *mut c_void,
        context: *mut c_void,
        complete: extern "C" fn(*mut c_void, i32),
    ) -> bool;
    fn glim_preview_find(handle: *mut c_void, show: bool) -> bool;
    fn glim_preview_release(handle: *mut c_void);
    fn glim_preview_take_navigation(handle: *mut c_void) -> *mut std::ffi::c_char;
    fn glim_preview_free_string(string: *mut std::ffi::c_char);
    fn glim_preview_reveal_anchor(handle: *mut c_void, bytes: *const u8, length: usize);
}

pub(super) struct NativePreview {
    handle: NonNull<c_void>,
    // GPUI entities are created/updated/dropped on the main thread. Prevent this
    // owner from ever being moved into a background task.
    _main_thread: PhantomData<Rc<()>>,
}
impl NativePreview {
    pub fn start_pdf_reload(&self) -> Option<futures_channel::oneshot::Receiver<PdfReloadResult>> {
        // The channel carries exactly one completion. Swift owns the boxed sender
        // only after acceptance, including when Rust drops the receiving task.
        let (completion_tx, completion_rx) = futures_channel::oneshot::channel();
        let context = Box::into_raw(Box::new(completion_tx)).cast::<c_void>();
        // SAFETY: main-thread retained controller; Swift takes the sender only on true.
        let accepted =
            unsafe { glim_preview_reload_pdf(self.handle.as_ptr(), context, pdf_reload_complete) };
        if accepted {
            Some(completion_rx)
        } else {
            // SAFETY: rejected requests never transfer context ownership to Swift.
            drop(unsafe { Box::from_raw(context.cast::<PdfReloadSender>()) });
            None
        }
    }

    pub fn find(&self, show: bool) -> bool {
        unsafe { glim_preview_find(self.handle.as_ptr(), show) }
    }
    pub fn new(configuration: &[u8]) -> Self {
        // SAFETY: Swift copies this valid slice before returning a retained handle.
        let handle = unsafe {
            glim_set_reader_resources(Some(reader_resource));
            glim_set_reader_telemetry(Some(reader_telemetry));
            glim_preview_create(configuration.as_ptr(), configuration.len())
        };
        Self {
            handle: NonNull::new(handle).expect("Swift returned a null preview"),
            _main_thread: PhantomData,
        }
    }
    pub fn layout(&self, bounds: Bounds<Pixels>, window: &Window) {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: the live GPUI window owns the parent NSView. Swift retains the
        // child in that view; both it and our preview handle remain alive here.
        unsafe {
            glim_preview_layout(
                self.handle.as_ptr(),
                handle.ns_view.as_ptr(),
                f32::from(bounds.origin.x) as f64,
                f32::from(bounds.origin.y) as f64,
                f32::from(bounds.size.width) as f64,
                f32::from(bounds.size.height) as f64,
            )
        }
    }
    pub fn take_navigation(&self) -> Option<String> {
        // SAFETY: called on the main thread with a retained handle. Swift returns
        // an owned, NUL-terminated UTF-8 string, freed through its matching API.
        unsafe {
            let pointer = glim_preview_take_navigation(self.handle.as_ptr());
            if pointer.is_null() {
                return None;
            }
            let url = std::ffi::CStr::from_ptr(pointer)
                .to_string_lossy()
                .into_owned();
            glim_preview_free_string(pointer);
            Some(url)
        }
    }
    pub fn reveal_anchor(&self, anchor: &str) {
        // SAFETY: Swift copies this valid UTF-8 slice during the main-thread call.
        unsafe { glim_preview_reveal_anchor(self.handle.as_ptr(), anchor.as_ptr(), anchor.len()) }
    }
    pub fn hide(&self) {
        // SAFETY: this owner retains the Swift controller, on the main thread.
        unsafe { glim_preview_hide(self.handle.as_ptr()) }
    }
}
impl Drop for NativePreview {
    fn drop(&mut self) {
        // SAFETY: release the sole retained handle exactly once, detaching its view
        // and stopping playback before the prepared SQLite snapshot is dropped.
        unsafe { glim_preview_release(self.handle.as_ptr()) }
    }
}
