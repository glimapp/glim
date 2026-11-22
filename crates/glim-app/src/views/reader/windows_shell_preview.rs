//! Host the registered Explorer preview handler in its COM server, within
//! the same document bounds used for WebView2 readers.
use super::Placement;
use anyhow::{Context, Result};
use std::path::Path;
use windows::{
    Win32::{
        Foundation::{HWND, RECT},
        System::Com::{
            CLSCTX_LOCAL_SERVER, CoCreateInstance, IPersistFile, STGM_READ, STGM_SHARE_DENY_NONE,
        },
        UI::{
            Shell::{
                ASSOCF_NONE, ASSOCSTR_SHELLEXTENSION, AssocQueryStringW, IInitializeWithItem,
                IPreviewHandler, IShellItem,
                PropertiesSystem::{IInitializeWithFile, IInitializeWithStream},
                SHCreateItemFromParsingName, SHCreateStreamOnFileEx,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, GWL_STYLE, GetWindowLongW, SW_HIDE, SW_SHOWNA,
                SWP_NOACTIVATE, SWP_NOZORDER, SetWindowLongW, SetWindowPos, ShowWindow, WS_CHILD,
                WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
            },
        },
    },
    core::{GUID, HSTRING, Interface, PCWSTR, PWSTR, w},
};

pub(super) struct Preview {
    handler: IPreviewHandler,
    host: HWND,
}

impl Preview {
    pub(super) fn new(path: &Path, placement: Placement) -> Result<Self> {
        let extension = path.extension().context("Missing preview file extension")?;
        let extension = HSTRING::from(format!(".{}", extension.to_string_lossy()));
        let shell_path = glim_services::files::windows_shell_path(path)?;
        let path = HSTRING::from(shell_path.as_os_str());
        // The shell association is a braced CLSID, 38 characters plus NUL.
        let mut class = [0u16; 39];
        let mut length = class.len() as u32;
        // SAFETY: all COM calls run on the parent window's STA. Only the
        // registered out-of-process server receives the read-only document.
        unsafe {
            AssocQueryStringW(
                ASSOCF_NONE,
                ASSOCSTR_SHELLEXTENSION,
                &extension,
                w!("{8895b1c6-b41f-4c1c-a562-0d564250836f}"),
                Some(PWSTR(class.as_mut_ptr())),
                &mut length,
            )
            .ok()?;
            let id = String::from_utf16(
                &class[..class.iter().position(|c| *c == 0).unwrap_or(class.len())],
            )?;
            let clsid = GUID::try_from(id.trim_matches(['{', '}']))?;
            let handler: IPreviewHandler = CoCreateInstance(&clsid, None, CLSCTX_LOCAL_SERVER)?;
            if let Ok(stream_init) = handler.cast::<IInitializeWithStream>() {
                let stream = SHCreateStreamOnFileEx(&path, STGM_SHARE_DENY_NONE.0, 0, false, None)?;
                stream_init.Initialize(&stream, STGM_READ.0)?;
            } else if let Ok(item_init) = handler.cast::<IInitializeWithItem>() {
                let item: IShellItem = SHCreateItemFromParsingName(&path, None)?;
                item_init.Initialize(&item, STGM_READ.0)?;
            } else if let Ok(file_init) = handler.cast::<IInitializeWithFile>() {
                file_init.Initialize(&path, STGM_READ.0)?;
            } else {
                handler.cast::<IPersistFile>()?.Load(&path, STGM_READ)?;
            }
            let bounds = placement.bounds.size.to_physical::<u32>(placement.scale);
            let origin = placement
                .bounds
                .position
                .to_physical::<i32>(placement.scale);
            let rect = RECT {
                right: bounds.width as i32,
                bottom: bounds.height as i32,
                ..Default::default()
            };
            // Exclude the handler's child window when GPUI repaints the workspace.
            let parent = HWND(placement.parent.hwnd.get() as _);
            let style = GetWindowLongW(parent, GWL_STYLE);
            SetWindowLongW(parent, GWL_STYLE, style | WS_CLIPCHILDREN.0 as i32);
            let host = CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                PCWSTR::null(),
                WS_VISIBLE | WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                origin.x,
                origin.y,
                rect.right,
                rect.bottom,
                Some(parent),
                None,
                None,
                None,
            )?;
            let preview = Self { handler, host };
            preview.handler.SetWindow(host, &rect)?;
            preview.handler.DoPreview()?;
            Ok(preview)
        }
    }

    pub(super) fn place(&self, placement: Placement) {
        let size = placement.bounds.size.to_physical::<u32>(placement.scale);
        let origin = placement
            .bounds
            .position
            .to_physical::<i32>(placement.scale);
        let rect = RECT {
            right: size.width as i32,
            bottom: size.height as i32,
            ..Default::default()
        };
        // SAFETY: the handler and this child HWND are owned by this preview.
        unsafe {
            let _ = SetWindowPos(
                self.host,
                None,
                origin.x,
                origin.y,
                rect.right,
                rect.bottom,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let _ = self.handler.SetRect(&rect);
        }
    }
    pub(super) fn show(&self, visible: bool) {
        unsafe {
            let _ = ShowWindow(self.host, if visible { SW_SHOWNA } else { SW_HIDE });
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        // Release the handler's document and child windows before destroying its parent.
        unsafe {
            let _ = self.handler.Unload();
            let _ = DestroyWindow(self.host);
        }
    }
}
