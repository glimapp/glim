#![allow(unsafe_code)]

use anyhow::{Result, ensure};
use std::{os::windows::ffi::OsStrExt, path::Path, ptr};
use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

pub fn run_print_subprocess() -> Result<()> {
    anyhow::bail!("Direct Markdown PDF export is unavailable on Windows")
}

pub(crate) async fn open_in_browser(path: &Path) -> Result<()> {
    let path: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: the path is NUL-terminated and remains valid for the call.
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            ptr::null(),
            path.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    ensure!(
        result as isize > 32,
        "The default browser could not open the HTML file"
    );
    Ok(())
}
