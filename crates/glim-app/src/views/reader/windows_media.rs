use super::PreviewParent;
use raw_window_handle::Win32WindowHandle;
use serde_json::{Value, json};
use std::{borrow::Cow, path::Path};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY_CORS, ICoreWebView2_3,
};
use windows::{
    Win32::System::Com::CoCreateGuid,
    core::{HSTRING, Interface},
};
use wry::{WebView, WebViewBuilder, WebViewExtWindows, http::Response};

const PLAYER: &str = include_str!("../../../../../assets/windows/media.html");

pub(super) fn build(
    builder: WebViewBuilder<'_>,
    parent: Win32WindowHandle,
    content: &Value,
) -> wry::Result<WebView> {
    let missing = || std::io::Error::other("Invalid media playlist");
    let files = content["files"].as_array().ok_or_else(missing)?;
    let folder = Path::new(files.first().and_then(Value::as_str).ok_or_else(missing)?)
        .parent()
        .ok_or_else(missing)?;
    let mut names = Vec::with_capacity(files.len());
    for file in files {
        let path = Path::new(file.as_str().ok_or_else(missing)?);
        if path.parent() != Some(folder) {
            return Err(missing().into());
        }
        names.push(
            path.file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(missing)?,
        );
    }
    let folder = glim_services::files::windows_shell_path(folder)?;
    let id = unsafe { CoCreateGuid()? }.to_u128();
    let media = format!("media-{id:x}.invalid");
    let player = format!("http://glim-player.{id:x}.invalid/player");
    let page = PLAYER
        .replace("MEDIA_ORIGIN", &format!("http://{media}"))
        .into_bytes();
    let queue = json!({
        "names": names, "shuffle": content["shuffle"], "autoplay": content["autoplay"],
        "label": glim_services::locale::tr("Playlist"),
        "error": glim_services::locale::tr("Cannot play this format or codec"),
    })
    .to_string()
    .into_bytes();
    let allowed = player.clone();
    let view = builder
        .with_navigation_handler(move |url| url == allowed || url == "about:blank")
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_custom_protocol("glim-player".into(), move |_, request| {
            let (status, mime, body) = match request.uri().path() {
                "/player" => (200, "text/html; charset=utf-8", page.clone()),
                "/queue" => (200, "application/json", queue.clone()),
                _ => (404, "text/plain", Vec::new()),
            };
            Response::builder()
                .status(status)
                .header("Content-Type", mime)
                .header("Cache-Control", "no-store")
                .body(Cow::Owned(body))
                .expect("static headers")
        })
        .build_as_child(&PreviewParent(parent))?;
    // WebView2 streams and seeks media directly from the selected folder. Cross-origin
    // scripts cannot read its files. Only the bundled player can navigate in this view.
    unsafe {
        view.webview()
            .cast::<ICoreWebView2_3>()?
            .SetVirtualHostNameToFolderMapping(
                &HSTRING::from(media),
                &HSTRING::from(folder.as_os_str()),
                COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY_CORS,
            )?;
    }
    view.load_url(&player)?;
    Ok(view)
}
