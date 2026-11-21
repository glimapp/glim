//! Native entry points for Markdown export.
#![allow(unsafe_code)]

use anyhow::{Context, Result, ensure};
use futures_channel::oneshot;
use std::{
    ffi::{CString, c_void},
    os::unix::ffi::OsStrExt,
    path::Path,
};

unsafe extern "C" {
    fn glim_print_markdown_pdf(
        input: *const std::ffi::c_char,
        output: *const std::ffi::c_char,
    ) -> i32;
    fn glim_open_markdown_in_browser(
        path: *const std::ffi::c_char,
        context: *mut c_void,
        complete: unsafe extern "C" fn(*mut c_void, i32),
    );
}

pub fn run_print_subprocess() -> Result<()> {
    let mut args = std::env::args_os().skip(2);
    let input = args.next().context("Missing print input")?;
    let output = args.next().context("Missing print output")?;
    let input = CString::new(input.as_bytes())?;
    let output = CString::new(output.as_bytes())?;
    // Swift borrows both NUL-terminated arguments for the duration of the call.
    let status = unsafe { glim_print_markdown_pdf(input.as_ptr(), output.as_ptr()) };
    ensure!(status == 0, "Markdown PDF printing failed");
    Ok(())
}

pub(crate) async fn open_in_browser(path: &Path) -> Result<()> {
    use glim_services::locale::tr;

    unsafe extern "C" fn complete(context: *mut c_void, status: i32) {
        // Swift consumes this sender exactly once, including immediate failures.
        let sender = unsafe { Box::from_raw(context.cast::<oneshot::Sender<i32>>()) };
        let _ = sender.send(status);
    }

    let path = CString::new(path.as_os_str().as_bytes())?;
    let (sender, status) = oneshot::channel();
    let context = Box::into_raw(Box::new(sender)).cast();
    // Called on the UI executor. Swift copies the path before returning.
    unsafe { glim_open_markdown_in_browser(path.as_ptr(), context, complete) };
    match status.await? {
        0 => Ok(()),
        1 => anyhow::bail!("{}", tr("No default browser is available")),
        _ => anyhow::bail!("{}", tr("The default browser could not open the HTML file")),
    }
}
