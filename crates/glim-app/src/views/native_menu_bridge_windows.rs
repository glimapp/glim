#![allow(unsafe_code)]

use gpui_kit::{AsyncWindowContext, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ptr;
use windows_sys::Win32::{
    Foundation::{HWND, POINT},
    UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HMENU, MF_CHECKED, MF_GRAYED,
        MF_SEPARATOR, MF_STRING, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
    },
};

pub(super) struct NativeMenu {
    handle: HMENU,
    owner: HWND,
    point: POINT,
}

pub(super) struct MenuItem {
    title: Option<String>,
    enabled: bool,
    checked: bool,
}

impl MenuItem {
    pub fn new(title: String) -> Self {
        Self {
            title: Some(title),
            enabled: true,
            checked: false,
        }
    }

    pub fn separator() -> Self {
        Self {
            title: None,
            enabled: false,
            checked: false,
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

    pub fn symbol(self, _symbol: &'static str) -> Self {
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
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return None;
        };
        let mut point = POINT { x: 0, y: 0 };
        // SAFETY: the Win32 APIs receive an initialized point and return a menu we own.
        if unsafe { GetCursorPos(&mut point) } == 0 {
            return None;
        }
        let menu = unsafe { CreatePopupMenu() };
        if menu.is_null() {
            return None;
        }
        let menu = Self {
            handle: menu,
            owner: handle.hwnd.get() as HWND,
            point,
        };
        for (index, item) in items.iter().enumerate() {
            let mut flags = if item.title.is_some() {
                MF_STRING
            } else {
                MF_SEPARATOR
            };
            if !item.enabled {
                flags |= MF_GRAYED;
            }
            if item.checked {
                flags |= MF_CHECKED;
            }
            let title = item.title.as_ref().map(|title| {
                title
                    .encode_utf16()
                    .filter(|unit| *unit != 0)
                    .chain([0])
                    .collect::<Vec<_>>()
            });
            // SAFETY: the menu is owned here and Windows copies the item label.
            if unsafe {
                AppendMenuW(
                    menu.handle,
                    flags,
                    index + 1,
                    title.as_ref().map_or(ptr::null(), |title| title.as_ptr()),
                )
            } == 0
            {
                return None;
            }
        }
        Some(menu)
    }

    pub async fn choose(self, _cx: &mut AsyncWindowContext) -> Option<usize> {
        // SAFETY: the owning window and menu both belong to this UI thread.
        let command = unsafe {
            TrackPopupMenu(
                self.handle,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                self.point.x,
                self.point.y,
                0,
                self.owner,
                ptr::null(),
            )
        };
        usize::try_from(command).ok()?.checked_sub(1)
    }
}

impl Drop for NativeMenu {
    fn drop(&mut self) {
        // SAFETY: this is the sole owner of the popup menu.
        unsafe { DestroyMenu(self.handle) };
    }
}
