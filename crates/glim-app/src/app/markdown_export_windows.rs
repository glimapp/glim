#![allow(unsafe_code)]

use anyhow::{Context, Result, ensure};
use std::path::Path;

pub fn run_print_subprocess() -> Result<()> {
    use std::{path::PathBuf, sync::mpsc};
    use tao::{event_loop::EventLoop, window::WindowBuilder};
    use webview2_com::{
        Microsoft::Web::WebView2::Win32::{ICoreWebView2_7, ICoreWebView2Environment6},
        NavigationCompletedEventHandler, PrintToPdfCompletedHandler, ProcessFailedEventHandler,
        wait_with_pump,
    };
    use windows::core::{BOOL, HSTRING, Interface};
    use wry::{WebViewBuilder, WebViewExtWindows};

    let mut args = std::env::args_os().skip(2);
    let input = PathBuf::from(args.next().context("Missing print input")?).canonicalize()?;
    let output = std::path::absolute(args.next().context("Missing print output")?)?;
    let source =
        url::Url::from_file_path(input).map_err(|_| anyhow::anyhow!("Invalid print input path"))?;
    let allowed = source.clone();
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_visible(false)
        .build(&event_loop)?;
    let view = WebViewBuilder::new()
        .with_incognito(true)
        .with_devtools(false)
        .with_javascript_disabled()
        .with_navigation_handler(move |url| url::Url::parse(&url).is_ok_and(|url| url == allowed))
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_download_started_handler(|_, _| false)
        .build(&window)?;
    let web = view.webview().cast::<ICoreWebView2_7>()?;
    let environment = view.environment().cast::<ICoreWebView2Environment6>()?;
    let (loaded, navigation) = mpsc::channel();
    let (printed, printing) = mpsc::channel();
    let load_failed = loaded.clone();
    let print_failed = printed.clone();
    let mut token = 0;
    // SAFETY: this helper owns the window and pumps its STA through both callbacks.
    unsafe {
        web.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, _| {
                let _ = load_failed.send(false);
                let _ = print_failed.send(false);
                Ok(())
            })),
            &mut token,
        )?;
        web.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                let mut success = BOOL::default();
                let ok = args
                    .is_some_and(|args| args.IsSuccess(&mut success).is_ok() && success.as_bool());
                let _ = loaded.send(ok);
                Ok(())
            })),
            &mut token,
        )?;
        view.load_url(source.as_str())?;
        ensure!(
            wait_with_pump(navigation)?,
            "Cannot load prepared Markdown for printing"
        );
        // Match the A4 sheet and 42-point margins of the published macOS reader.
        let settings = environment.CreatePrintSettings()?;
        settings.SetPageWidth(210.0 / 25.4)?;
        settings.SetPageHeight(297.0 / 25.4)?;
        let margin = 42.0 / 72.0;
        settings.SetMarginTop(margin)?;
        settings.SetMarginBottom(margin)?;
        settings.SetMarginLeft(margin)?;
        settings.SetMarginRight(margin)?;
        settings.SetShouldPrintBackgrounds(true)?;
        settings.SetShouldPrintHeaderAndFooter(false)?;
        web.PrintToPdf(
            &HSTRING::from(output.as_os_str()),
            &settings,
            &PrintToPdfCompletedHandler::create(Box::new(move |result, ok| {
                let _ = printed.send(result.is_ok() && ok);
                Ok(())
            })),
        )?;
        ensure!(wait_with_pump(printing)?, "Cannot print Markdown PDF");
    }
    Ok(())
}

pub(crate) async fn open_in_browser(path: &Path) -> Result<()> {
    open_html_file(path)
}

pub(super) fn open_html_file(path: &Path) -> Result<()> {
    use windows::{
        Win32::UI::{Shell::*, WindowsAndMessaging::SW_SHOWNORMAL},
        core::{HSTRING, PCWSTR, PWSTR, w},
    };
    let url =
        url::Url::from_file_path(path).map_err(|_| anyhow::anyhow!("Invalid preview path"))?;
    let url = HSTRING::from(url.as_str());
    // An HTML file association may point to Glim or another editor. Use the
    // registered HTTPS browser for the explicit Open in Browser action.
    let mut length = 0;
    unsafe {
        AssocQueryStringW(
            ASSOCF_IS_PROTOCOL,
            ASSOCSTR_PROGID,
            w!("https"),
            None,
            None,
            &mut length,
        )
        .ok()?;
        ensure!(length > 1, "No default browser is available");
        let mut class = vec![0; length as usize];
        AssocQueryStringW(
            ASSOCF_IS_PROTOCOL,
            ASSOCSTR_PROGID,
            w!("https"),
            None,
            Some(PWSTR(class.as_mut_ptr())),
            &mut length,
        )
        .ok()?;
        let mut launch = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_CLASSNAME | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpClass: PCWSTR(class.as_ptr()),
            lpVerb: w!("open"),
            lpFile: PCWSTR(url.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        ShellExecuteExW(&mut launch).context("The default browser could not open the HTML file")?;
    }
    Ok(())
}
