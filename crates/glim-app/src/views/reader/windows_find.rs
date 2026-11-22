use super::FindEvent;
use std::{cell::Cell, rc::Rc};
use webview2_com::{
    AcceleratorKeyPressedEventHandler, FindStartCompletedHandler,
    Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, ICoreWebView2_28, ICoreWebView2Environment15,
        ICoreWebView2Find,
    },
};
use windows::{
    Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_F, VK_MENU, VK_SHIFT},
    core::{HSTRING, Interface, Result},
};
use wry::{WebView, WebViewExtWindows};

pub(super) struct Session {
    finder: ICoreWebView2Find,
    environment: ICoreWebView2Environment15,
    pub open: Cell<bool>,
    pub command: Rc<Cell<i32>>,
    // Replacing the flag on each query keeps old callbacks from finishing a later search.
    pending: Cell<Option<Rc<Cell<bool>>>>,
    last: Cell<Option<(i32, i32, bool)>>,
}

impl Session {
    pub fn new(view: &WebView) -> Result<Self> {
        let environment = view.environment().cast()?;
        let web = view.webview().cast::<ICoreWebView2_28>()?;
        let command = Rc::new(Cell::new(0));
        let hotkey = command.clone();
        // SAFETY: the controller and its event callbacks belong to the window thread.
        let finder = unsafe {
            let finder = web.Find()?;
            view.controller().add_AcceleratorKeyPressed(
                &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let mut key = 0;
                    args.VirtualKey(&mut key)?;
                    if key == u32::from(VK_F.0)
                        && GetKeyState(VK_CONTROL.0.into()) < 0
                        && GetKeyState(VK_MENU.0.into()) >= 0
                        && GetKeyState(VK_SHIFT.0.into()) >= 0
                    {
                        let mut kind = Default::default();
                        args.KeyEventKind(&mut kind)?;
                        args.SetHandled(true)?;
                        if kind == COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN {
                            hotkey.set(1);
                        }
                    }
                    Ok(())
                })),
                &mut 0,
            )?;
            finder
        };
        Ok(Self {
            finder,
            environment,
            command,
            open: Cell::new(false),
            pending: Cell::default(),
            last: Cell::default(),
        })
    }

    pub fn set_open(&self, open: bool) {
        self.open.set(open);
        self.last.set(None);
        if !open {
            self.search("", false, false, true);
        }
    }

    pub fn search(&self, query: &str, sensitive: bool, backwards: bool, changed: bool) {
        self.last.set(None);
        // SAFETY: Glim's find controls call this only on the controller's UI thread.
        let run = || unsafe {
            if query.is_empty() {
                self.pending.set(None);
                return self.finder.Stop();
            }
            if !changed {
                return if backwards {
                    self.finder.FindPrevious()
                } else {
                    self.finder.FindNext()
                };
            }
            let options = self.environment.CreateFindOptions()?;
            options.SetFindTerm(&HSTRING::from(query))?;
            options.SetIsCaseSensitive(sensitive)?;
            options.SetSuppressDefaultFindDialog(true)?;
            options.SetShouldHighlightAllMatches(true)?;
            let pending = Rc::new(Cell::new(true));
            self.pending.set(Some(pending.clone()));
            self.finder.Start(
                &options,
                &FindStartCompletedHandler::create(Box::new(move |_| {
                    pending.set(false);
                    Ok(())
                })),
            )
        };
        if let Err(error) = run() {
            self.pending.set(None);
            log::debug!("WebView2 find: {error}");
        }
    }

    pub fn event(&self) -> Option<FindEvent> {
        let command = self.command.replace(0);
        let open = self.open.get();
        if !open && command == 0 {
            return None;
        }
        let pending = self.pending.take();
        let searching = pending.as_ref().is_some_and(|p| p.get());
        self.pending.set(pending);
        let mut count = 0;
        let mut index = -1;
        // SAFETY: queried by the reader on the owning UI thread.
        unsafe {
            let _ = self.finder.MatchCount(&mut count);
            let _ = self.finder.ActiveMatchIndex(&mut index);
        }
        let status = (count, index, searching);
        if self.last.replace(Some(status)) == Some(status) && command == 0 {
            return None;
        }
        Some(FindEvent {
            command,
            open,
            label: if count > 0 {
                format!("{index} / {count}")
            } else {
                "0 / 0".into()
            },
            found: count > 0,
            searching,
        })
    }
}
