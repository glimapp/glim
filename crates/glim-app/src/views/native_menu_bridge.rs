//! Main-thread AppKit context-menu ownership, independent of document readers.
#![allow(unsafe_code)]
use gpui_kit::{AsyncWindowContext, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc};

unsafe extern "C" {
    fn glim_context_menu_create(
        parent: *mut c_void,
        x: f64,
        y: f64,
        bytes: *const u8,
        length: usize,
    ) -> *mut c_void;
    fn glim_context_menu_selection(handle: *mut c_void) -> i64;
    fn glim_context_menu_release(handle: *mut c_void);
}
pub(super) struct NativeMenu {
    handle: NonNull<c_void>,
    _main_thread: PhantomData<Rc<()>>,
}
pub(super) struct MenuItem {
    title: Option<String>,
    enabled: bool,
    checked: bool,
    symbol: Option<&'static str>,
}
impl MenuItem {
    pub fn new(title: String) -> Self {
        let symbol = menu_symbol(&title);
        Self {
            title: Some(title),
            enabled: true,
            checked: false,
            symbol,
        }
    }
    pub fn separator() -> Self {
        Self {
            title: None,
            enabled: false,
            checked: false,
            symbol: None,
        }
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
    pub fn symbol(mut self, symbol: &'static str) -> Self {
        self.symbol = Some(symbol);
        self
    }
}
impl NativeMenu {
    pub fn new(items: &[(Option<String>, bool)], window: &Window) -> Option<Self> {
        let items = items
            .iter()
            .map(|(title, enabled)| {
                title.as_ref().map_or_else(MenuItem::separator, |title| {
                    MenuItem::new(title.clone()).enabled(*enabled)
                })
            })
            .collect::<Vec<_>>();
        Self::with_items(&items, window)
    }
    pub fn with_items(items: &[MenuItem], window: &Window) -> Option<Self> {
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        let position = window.mouse_position();
        let items = items.iter().map(|item| serde_json::json!({
            "title": item.title, "enabled": item.enabled, "checked": item.checked, "symbol": item.symbol,
        })).collect::<Vec<_>>();
        let bytes = serde_json::to_vec(&items).expect("Context menu items");
        // SAFETY: AppKit's live parent view is owned by this window. Swift copies
        // titles and schedules menu tracking after the current Rust update ends.
        let pointer = unsafe {
            glim_context_menu_create(
                handle.ns_view.as_ptr(),
                f32::from(position.x) as f64,
                f32::from(position.y) as f64,
                bytes.as_ptr(),
                bytes.len(),
            )
        };
        Some(Self {
            handle: NonNull::new(pointer)?,
            _main_thread: PhantomData,
        })
    }
    pub async fn choose(self, cx: &mut AsyncWindowContext) -> Option<usize> {
        loop {
            if let Some(selection) = self.selection() {
                return selection;
            }
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
        }
    }
    /// None while tracking; Some(None) on dismissal; otherwise the item index.
    pub fn selection(&self) -> Option<Option<usize>> {
        // SAFETY: the retained controller is accessed only on the UI thread.
        match unsafe { glim_context_menu_selection(self.handle.as_ptr()) } {
            -2 => None,
            -1 => Some(None),
            index => Some(usize::try_from(index).ok()),
        }
    }
}
impl Drop for NativeMenu {
    fn drop(&mut self) {
        // SAFETY: release this sole retained controller and cancel open tracking.
        unsafe { glim_context_menu_release(self.handle.as_ptr()) }
    }
}

fn menu_symbol(title: &str) -> Option<&'static str> {
    use glim_services::locale::tr;
    [
        ("Move Tab to New Window", "macwindow.badge.plus"),
        ("New File", "doc.badge.plus"),
        ("New Folder…", "folder.badge.plus"),
        ("Rename…", "pencil"),
        ("Rename Workspace…", "pencil"),
        ("Open in Current Window", "macwindow"),
        ("Open in New Window", "macwindow.badge.plus"),
        ("Remove from Recent Workspaces", "clock.badge.xmark"),
        ("Add Folder to Workspace…", "folder.badge.plus"),
        ("Open", "arrow.up.forward.app"),
        ("Open Preview / Play", "play"),
        ("Play in Order", "play"),
        ("Shuffle Play", "shuffle"),
        ("Reveal in Finder", "folder"),
        ("Copy Relative Path", "doc.on.doc"),
        ("Copy Absolute Path", "link"),
        ("Move to Folder…", "folder"),
        ("Move to Trash…", "trash"),
        ("Remove Folder from Workspace", "folder.badge.minus"),
        ("Close", "xmark"),
        ("Close Tabs to the Right", "arrow.right.to.line"),
        ("Close Other Tabs", "rectangle.stack"),
        ("Close All Tabs", "xmark.rectangle"),
        ("Cut", "scissors"),
        ("Copy", "doc.on.doc"),
        ("Paste", "doc.on.clipboard"),
        ("Select All", "selection.pin.in.out"),
    ]
    .into_iter()
    .find_map(|(label, symbol)| (title == tr(label)).then_some(symbol))
}
