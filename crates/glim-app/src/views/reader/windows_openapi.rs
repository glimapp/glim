use super::PreviewParent;
use glim_services::{limits, locale, openapi, work::Cancellation};
use gpui_kit::BackgroundExecutor;
use raw_window_handle::Win32WindowHandle;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};
use windows::Win32::System::Com::CoCreateGuid;
use wry::{WebView, WebViewBuilder, http::Response};

const VIEWER: &str = include_str!("../../../../../assets/openapi/viewer.html");

pub(super) fn build(
    builder: WebViewBuilder<'_>,
    parent: Win32WindowHandle,
    database: PathBuf,
    background: BackgroundExecutor,
) -> wry::Result<WebView> {
    // Each document has its own origin. The only HTML served is the compiled viewer.
    let host = format!("{:x}.invalid", unsafe { CoCreateGuid()? }.to_u128());
    let page = format!("http://glim-api.{host}/index.html");
    let allowed = page.clone();
    let database = Arc::new(database);
    let (token, guard) = Cancellation::task();
    builder
        .with_navigation_handler(move |url| url == allowed)
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_asynchronous_custom_protocol("glim-api".into(), move |_, request, responder| {
            let _keep_until_close = &guard;
            let url = url::Url::parse(&request.uri().to_string());
            let token = token.clone();
            let database = database.clone();
            let local = request.method() == "GET" && url.as_ref().is_ok_and(|url| url.host_str() == Some(host.as_str()));
            background.spawn(async move {
                let (status, mime, body) = match url {
                    Ok(url) if local && url.path() == "/index.html" => (200, "text/html", VIEWER.replace("connect-src glim-api:", "connect-src 'self'").into_bytes()),
                    Ok(url) if local => {
                        let result = if url.path() == "/config" {
                            Ok(json!({ "translations": locale::translations(), "colors": {},
                                "pageSize": limits::OPENAPI_PAGE_NODES, "depth": limits::OPENAPI_DEPTH,
                                "fieldBytes": limits::OPENAPI_FIELD_BYTES }))
                        } else {
                            openapi::query::resource(&database, &url, &token)
                        };
                        match result {
                            Ok(value) => (200, "application/json", value.to_string().into_bytes()),
                            Err(error) => (500, "application/json", json!({"error": error.to_string()}).to_string().into_bytes()),
                        }
                    }
                    _ => (404, "text/plain", Vec::new()),
                };
                responder.respond(Response::builder().status(status)
                    .header("Content-Type", format!("{mime}; charset=utf-8"))
                    .header("Cache-Control", "no-store")
                    .header("X-Content-Type-Options", "nosniff")
                    .body(body).expect("static response headers"));
            }).detach();
        })
        .with_url(&page)
        .build_as_child(&PreviewParent(parent))
}
