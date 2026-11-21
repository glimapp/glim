//! Main-thread entry point for the private WebKit print subprocess.
#![allow(unsafe_code)]

use anyhow::{Context, Result, ensure};
use std::{ffi::CString, os::unix::ffi::OsStrExt};

unsafe extern "C" {
    fn glim_print_markdown_pdf(
        input: *const std::ffi::c_char,
        output: *const std::ffi::c_char,
    ) -> i32;
}

pub fn run() -> Result<()> {
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
