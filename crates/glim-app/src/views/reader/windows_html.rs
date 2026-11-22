//! A private WebView2 origin for each HTML preview. Only the document uses
//! the editor snapshot; stylesheets, scripts and images are file streams.
use gpui_kit::{BackgroundExecutor, ForegroundExecutor};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, OnceLock},
};
use url::Url;
use webview2_com::{
    Microsoft::Web::WebView2::Win32::COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
    NavigationStartingEventHandler, NewWindowRequestedEventHandler,
    WebResourceRequestedEventHandler, take_pwstr,
};
use windows::{
    Win32::System::Com::{
        COINIT_MULTITHREADED, CoCreateGuid, CoInitializeEx, CoUninitialize, IStream,
        STGM_SHARE_DENY_NONE, STREAM_SEEK_SET, StructuredStorage::CreateStreamOnHGlobal,
    },
    Win32::UI::Shell::SHCreateStreamOnFileEx,
    core::{AgileReference, BOOL, HSTRING, PWSTR, w},
};
use wry::{WebView, WebViewExtWindows};

struct Document {
    page: Url,
    file: Url,
    source: PathBuf,
    text: Option<String>,
    root: OnceLock<Option<PathBuf>>,
}

impl Document {
    fn local_url(&self, target: &Url) -> Option<Url> {
        if target.origin() != self.page.origin() {
            return None;
        }
        let mut local = self.file.clone();
        local.set_path(target.path());
        local.set_fragment(target.fragment());
        Some(local)
    }

    fn follow(&self, target: &Url, navigation: &RefCell<Option<String>>) {
        let link = self.local_url(target).or_else(|| {
            matches!(target.scheme(), "file" | "https" | "http" | "mailto").then(|| target.clone())
        });
        if let Some(link) = link {
            navigation.replace(Some(link.into()));
        }
    }

    fn resource_root(&self) -> Option<&Path> {
        self.root
            .get_or_init(|| {
                let directory = self.source.parent()?;
                let root = directory
                    .ancestors()
                    .find(|dir| dir.join(".git").exists())
                    .unwrap_or(directory);
                root.canonicalize().ok()
            })
            .as_deref()
    }

    fn stream(&self, target: &Url) -> Option<(AgileReference<IStream>, String)> {
        let local = self.local_url(target)?;
        let path = local.to_file_path().ok()?;
        let main = target.path() == self.page.path();
        // SAFETY: all worker COM objects are dropped before uninitializing this
        // apartment. Only the marshalled stream reference crosses threads.
        unsafe {
            let apartment = CoInitializeEx(None, COINIT_MULTITHREADED);
            let result = (|| {
                let stream = if main && let Some(text) = &self.text {
                    let stream = CreateStreamOnHGlobal(Default::default(), true).ok()?;
                    for chunk in text
                        .as_bytes()
                        .chunks(glim_services::limits::IO_BUFFER_BYTES)
                    {
                        stream
                            .Write(chunk.as_ptr().cast(), chunk.len() as u32, None)
                            .ok()
                            .ok()?;
                    }
                    stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
                    stream
                } else {
                    // Check the resolved path as junctions can leave the project.
                    let resolved = path.canonicalize().ok()?;
                    if !main && !resolved.starts_with(self.resource_root()?) {
                        return None;
                    }
                    if !resolved.is_file() {
                        return None;
                    }
                    SHCreateStreamOnFileEx(
                        &HSTRING::from(resolved.as_os_str()),
                        STGM_SHARE_DENY_NONE.0,
                        0,
                        false,
                        None,
                    )
                    .ok()?
                };
                let mime = if main && self.text.is_some() {
                    "text/html; charset=utf-8"
                } else if main {
                    "text/html"
                } else {
                    mime_guess::from_path(&path)
                        .first_raw()
                        .unwrap_or("application/octet-stream")
                };
                Some((AgileReference::new(&stream).ok()?, mime.to_owned()))
            })();
            if apartment.is_ok() {
                CoUninitialize();
            }
            result
        }
    }
}

pub(super) fn load(
    view: &WebView,
    source: &Path,
    text: Option<&str>,
    anchor: Option<&str>,
    navigation: Rc<RefCell<Option<String>>>,
    foreground: ForegroundExecutor,
    background: BackgroundExecutor,
) -> wry::Result<()> {
    let file =
        Url::from_file_path(source).map_err(|_| std::io::Error::other("Invalid HTML file path"))?;
    // SAFETY: CoCreateGuid writes a fresh identifier and has no pointer arguments.
    let id = unsafe { CoCreateGuid()? }.to_u128();
    // Plain HTTP on a reserved, unresolvable host also keeps Service Workers
    // from taking control of future resource requests.
    let mut page = Url::parse(&format!("http://{id:x}.glim.invalid/")).unwrap();
    page.set_path(file.path());
    page.set_fragment(anchor);
    let document = Arc::new(Document {
        page: page.clone(),
        file,
        source: source.to_owned(),
        text: text.map(str::to_owned),
        root: OnceLock::new(),
    });
    let web = view.webview();
    let env = view.environment();
    let mut token = 0;
    // SAFETY: WebView2 events and deferral completion run on the owning UI thread.
    unsafe {
        web.AddWebResourceRequestedFilter(
            &HSTRING::from(format!("{}/*", page.origin().ascii_serialization())),
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
        )?;
        let resources = document.clone();
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut uri = PWSTR::null();
            args.Request()?.Uri(&mut uri)?;
            let url = Url::parse(&take_pwstr(uri)).ok();
            let completion = args.GetDeferral()?;
            let resources = resources.clone();
            let work = background.spawn(async move { resources.stream(&url?) });
            let env = env.clone();
            foreground.spawn(async move {
                let resource = work.await.and_then(|(stream, mime)| stream.resolve().ok().map(|stream| (stream, mime)));
                let response = match resource {
                    Some((stream, mime)) => {
                        let headers = format!("Content-Type: {mime}\r\nCache-Control: no-store\r\nCross-Origin-Resource-Policy: same-origin");
                        env.CreateWebResourceResponse(&stream, 200, w!("OK"), &HSTRING::from(headers))
                    }
                    None => {
                        let message = glim_services::locale::tr("Document preview is unavailable");
                        let body = windows::Win32::UI::Shell::SHCreateMemStream(Some(message.as_bytes()));
                        env.CreateWebResourceResponse(body.as_ref(), 404, w!("Not Found"), w!("Content-Type: text/plain; charset=utf-8\r\nCache-Control: no-store"))
                    }
                };
                if let Ok(response) = response { let _ = args.SetResponse(&response); }
                let _ = completion.Complete();
            }).detach();
            Ok(())
        }));
        web.add_WebResourceRequested(&handler, &mut token)?;
        let links = navigation.clone();
        let current = document.clone();
        web.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let target = Url::parse(&take_pwstr(uri)).ok();
                if target.as_ref().is_some_and(|u| {
                    u.origin() == current.page.origin() && u.path() == current.page.path()
                }) {
                    return Ok(());
                }
                args.SetCancel(true)?;
                let mut user = BOOL::default();
                args.IsUserInitiated(&mut user)?;
                if user.as_bool()
                    && let Some(target) = target
                {
                    current.follow(&target, &links);
                }
                Ok(())
            })),
            &mut token,
        )?;
        web.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                args.SetHandled(true)?;
                let mut user = BOOL::default();
                args.IsUserInitiated(&mut user)?;
                if user.as_bool() {
                    let mut uri = PWSTR::null();
                    args.Uri(&mut uri)?;
                    if let Ok(target) = Url::parse(&take_pwstr(uri)) {
                        document.follow(&target, &navigation);
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    view.load_url(page.as_str())
}
