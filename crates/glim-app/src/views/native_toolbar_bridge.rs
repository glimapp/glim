//! Main-thread ownership of the shared AppKit reader capsule.
#![allow(unsafe_code)]
use gpui_kit::{Bounds, Pixels, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    cell::RefCell,
    collections::VecDeque,
    ffi::{CStr, c_char, c_void},
    ptr::NonNull,
    rc::Rc,
    task::{Poll, Waker},
};

#[derive(Default)]
pub(super) struct ToolbarEvents {
    pending: VecDeque<String>,
    waker: Option<Waker>,
}
impl ToolbarEvents {
    pub async fn next(events: &Rc<RefCell<Self>>) -> String {
        std::future::poll_fn(|cx| {
            let mut events = events.borrow_mut();
            if let Some(action) = events.pending.pop_front() {
                Poll::Ready(action)
            } else {
                events.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await
    }
}
extern "C" fn invoke(context: *mut c_void, action: *const c_char) {
    // SAFETY: the bridge owns this boxed Rc until after the native view detaches;
    // Swift lends a valid UTF-8 C string only for this synchronous callback.
    let events = unsafe { &*context.cast::<Rc<RefCell<ToolbarEvents>>>() };
    let action = unsafe { CStr::from_ptr(action) }
        .to_string_lossy()
        .into_owned();
    let waker = {
        let mut events = events.borrow_mut();
        events.pending.push_back(action);
        events.waker.take()
    };
    if let Some(waker) = waker {
        waker.wake();
    }
}
unsafe extern "C" {
    fn glim_reader_toolbar_create(
        context: *mut c_void,
        callback: extern "C" fn(*mut c_void, *const c_char),
    ) -> *mut c_void;
    fn glim_reader_toolbar_update(
        handle: *mut c_void,
        parent: *mut c_void,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        bytes: *const u8,
        length: usize,
        visible: bool,
        collapsed: bool,
    );
    fn glim_reader_toolbar_release(handle: *mut c_void);
}
pub(super) struct NativeToolbar {
    handle: Option<NonNull<c_void>>,
    // Rc also keeps this owner confined to the UI thread.
    _events: Box<Rc<RefCell<ToolbarEvents>>>,
}
impl NativeToolbar {
    pub fn new(events: Rc<RefCell<ToolbarEvents>>, window: &Window) -> Self {
        let mut events = Box::new(events);
        // Simulated GPUI windows have no AppKit host and cannot own native views.
        let handle = HasWindowHandle::window_handle(window)
            .ok()
            .filter(|handle| matches!(handle.as_raw(), RawWindowHandle::AppKit(_)))
            .map(|_| {
                // SAFETY: stable boxed context remains owned until native release.
                let handle =
                    unsafe { glim_reader_toolbar_create((&raw mut *events).cast(), invoke) };
                NonNull::new(handle).expect("Native toolbar")
            });
        Self {
            handle,
            _events: events,
        }
    }
    pub fn update(
        &self,
        bounds: Bounds<Pixels>,
        items: &[u8],
        visible: bool,
        collapsed: bool,
        window: &Window,
    ) {
        let Some(toolbar) = self.handle else {
            return;
        };
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: called on the UI thread with a live window and owned controller;
        // Swift copies the serialized actions before returning.
        unsafe {
            glim_reader_toolbar_update(
                toolbar.as_ptr(),
                handle.ns_view.as_ptr(),
                f32::from(bounds.origin.x) as f64,
                f32::from(bounds.origin.y) as f64,
                f32::from(bounds.size.width) as f64,
                f32::from(bounds.size.height) as f64,
                items.as_ptr(),
                items.len(),
                visible,
                collapsed,
            );
        }
    }
}
impl Drop for NativeToolbar {
    fn drop(&mut self) {
        if let Some(handle) = self.handle {
            // SAFETY: detach native actions before dropping their callback context.
            unsafe { glim_reader_toolbar_release(handle.as_ptr()) }
        }
    }
}
