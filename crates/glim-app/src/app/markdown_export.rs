//! Native entry points for Markdown export.
#![allow(unsafe_code)]

use anyhow::{Context, Result, ensure};
use futures_channel::oneshot;
use glim_services::{files::DraftDestination, work::Cancellation};
use gpui_kit::component::input::Rope;
use std::{
    ffi::{CString, c_void},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

pub(crate) struct MarkdownExportSnapshot {
    pub(crate) source: Rope,
    pub(crate) base: url::Url,
    pub(crate) format_path: Option<PathBuf>,
    pub(crate) title: String,
}

impl MarkdownExportSnapshot {
    fn markdown(&self) -> String {
        let text = self.source.to_string();
        match &self.format_path {
            Some(path) => glim_services::text_formats::markdown(path, &text),
            None => text,
        }
    }

    pub(crate) fn html(self, cancel: &Cancellation) -> Result<(PathBuf, usize)> {
        glim_services::markdown_export::export_html(
            &self.markdown(),
            &self.base,
            &self.title,
            cancel,
        )
    }

    pub(crate) fn pdf(
        self,
        destination: &DraftDestination,
        cancel: &Cancellation,
    ) -> Result<usize> {
        glim_services::markdown_export::export_pdf(
            &self.markdown(),
            &self.base,
            &self.title,
            destination,
            cancel,
        )
    }
}

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

#[repr(i32)]
#[derive(Clone, Copy)]
pub(crate) enum PrintContent {
    Preview,
    Pdf,
    Text,
    Image,
}

pub(crate) fn show_print_panel(
    window: &gpui_kit::Window,
    content: PrintContent,
    bytes: &[u8],
    title: &str,
    directory: Option<tempfile::TempDir>,
) -> anyhow::Result<()> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    unsafe extern "C" {
        fn glim_show_print_panel(
            parent: *mut c_void,
            kind: i32,
            bytes: *const u8,
            length: usize,
            title: *const std::ffi::c_char,
            context: *mut c_void,
            release: extern "C" fn(*mut c_void),
        ) -> bool;
    }
    extern "C" fn release(context: *mut c_void) {
        // AppKit keeps the temporary PDF until printing or cancellation finishes.
        drop(unsafe { Box::from_raw(context.cast::<Option<tempfile::TempDir>>()) });
    }
    let RawWindowHandle::AppKit(parent) = HasWindowHandle::window_handle(window)?.as_raw() else {
        anyhow::bail!("Missing document window");
    };
    let title = CString::new(title)?;
    let context = Box::into_raw(Box::new(directory)).cast();
    if !unsafe {
        glim_show_print_panel(
            parent.ns_view.as_ptr(),
            content as i32,
            bytes.as_ptr(),
            bytes.len(),
            title.as_ptr(),
            context,
            release,
        )
    } {
        release(context);
        anyhow::bail!(
            "{}",
            glim_services::locale::tr("Could not print this document")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_snapshot_preserves_markdown_and_converts_other_markup() {
        let mut snapshot = MarkdownExportSnapshot {
            source: Rope::from("= 中文 title\n"),
            base: url::Url::parse("file:///tmp/document.adoc").unwrap(),
            format_path: None,
            title: "Document".into(),
        };
        assert_eq!(snapshot.markdown(), "= 中文 title\n");

        snapshot.format_path = Some(PathBuf::from("document.adoc"));
        assert_eq!(snapshot.markdown(), "# 中文 title\n");
    }
}
