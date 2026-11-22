use super::{PreviewState, Surface};
use futures_channel::oneshot;
use glim_services::documents::ReadFailureKind;
use std::{path::Path, rc::Rc};
use windows::{
    Data::Pdf::PdfDocument,
    Storage::Streams::IRandomAccessStream,
    Win32::System::WinRT::{
        CreateRandomAccessStreamOnFile, RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize,
    },
    core::HSTRING,
};

pub(in crate::views::reader) struct PdfReloadFailure(ReadFailureKind);

impl PdfReloadFailure {
    pub fn failure_kind(self) -> Option<ReadFailureKind> {
        Some(self.0)
    }
}

type ReloadResult = Result<(), PdfReloadFailure>;

// Check the replacement off-thread before asking the visible viewer to reload.
// A partially written or password-protected file should leave the readable page on screen.
fn validate(path: &Path) -> ReloadResult {
    fn read(path: &Path) -> windows::core::Result<u32> {
        // SAFETY: the returned stream owns its handle and the input string lives through the call.
        let stream: IRandomAccessStream =
            unsafe { CreateRandomAccessStreamOnFile(&HSTRING::from(path.as_os_str()), 0)? };
        PdfDocument::LoadFromStreamAsync(&stream)?
            .join()?
            .PageCount()
    }
    // SAFETY: this runs on a background worker. Balance this thread's initialization
    // after all WinRT objects have been released, including on a decoding error.
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
        .map_err(|_| PdfReloadFailure(ReadFailureKind::Other))?;
    let result = read(path);
    unsafe { RoUninitialize() };
    match result {
        Ok(1..) => Ok(()),
        Ok(0) => Err(PdfReloadFailure(ReadFailureKind::Other)),
        Err(error) => {
            let kind = match error.code().0 as u32 {
                0x80070002 | 0x80070003 => ReadFailureKind::NotFound,
                0x80070005 => ReadFailureKind::PermissionDenied,
                _ => ReadFailureKind::Other,
            };
            Err(PdfReloadFailure(kind))
        }
    }
}

pub(super) fn reload(preview: &Rc<PreviewState>) -> Option<oneshot::Receiver<ReloadResult>> {
    if preview.content["kind"] != "pdf" || preview.view.borrow().is_none() {
        return None;
    }
    let path = std::path::PathBuf::from(preview.content["source"].as_str()?);
    let (foreground, background) = preview.executors.get()?;
    let validation = background.spawn(async move { validate(&path) });
    let (sender, receiver) = oneshot::channel();
    let weak = Rc::downgrade(preview);
    foreground
        .spawn(async move {
            let result = validation.await;
            let Some(preview) = weak.upgrade() else {
                return;
            };
            if sender.is_canceled() {
                return;
            }
            let result = result.and_then(|()| {
                let view = preview.view.borrow();
                let Some((_, Surface::Browser(web))) = view.as_ref() else {
                    return Err(PdfReloadFailure(ReadFailureKind::Other));
                };
                web.reload()
                    .map_err(|_| PdfReloadFailure(ReadFailureKind::Other))
            });
            let _ = sender.send(result);
        })
        .detach();
    Some(receiver)
}
