use super::native::NativeReader;
use crate::app::reader_jobs::{self, JobClass, Priority};
use glim_core::resources::TextSnapshot;
use gpui_kit::{component::button::Button, *};
use std::{path::PathBuf, sync::Arc};

pub(super) struct OpenApiReader {
    // Release WebKit before its file-backed index and snapshot.
    browser: Option<Entity<NativeReader>>,
    index: Option<glim_services::openapi::Index>,
    path: PathBuf,
    text: Option<Arc<TextSnapshot>>,
    task: Option<Task<()>>,
    error: Option<String>,
}
impl OpenApiReader {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            text: None,
            task: None,
            browser: None,
            index: None,
            error: None,
        }
    }
    pub fn input_snapshot(&self) -> Option<&Arc<TextSnapshot>> {
        self.text.as_ref()
    }
    pub fn load(&mut self, text: Option<Arc<TextSnapshot>>, cx: &mut Context<Self>) {
        let same = match (&self.text, &text) {
            (Some(old), Some(new)) => Arc::ptr_eq(old, new) || old.as_str() == new.as_str(),
            _ => false,
        };
        self.text = text;
        if same && self.index.is_some() {
            self.set_active(true, cx);
            return;
        }
        self.browser = None;
        self.index = None;
        self.start(cx);
    }
    fn start(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        self.error = None;
        let path = self.path.clone();
        let text = self.text.clone();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |cancel| {
                glim_services::openapi::prepare(&path, text.as_ref().map(|s| s.as_str()), cancel)
            },
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                match result {
                    Ok(index) => {
                        let request = glim_services::preview::request::NativeRequest::new(
                            glim_services::preview::request::NativeContent::OpenApi {
                                source: view.path.clone(),
                                database: index.database.clone(),
                            },
                        );
                        match request.encode() {
                            Ok(configuration) => {
                                view.browser = Some(cx.new(|_| {
                                    NativeReader::openapi(view.path.clone(), &configuration)
                                }));
                                view.index = Some(index);
                            }
                            Err(error) => view.error = Some(format!("{error:#}")),
                        }
                    }
                    Err(error) => view.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if !active {
            self.task = None;
            self.browser = None;
        }
        if let Some(browser) = &self.browser {
            browser.update(cx, |browser, cx| browser.set_active(active, cx));
        } else if active && self.task.is_none() && self.error.is_none() {
            if let Some(index) = &self.index {
                let request = glim_services::preview::request::NativeRequest::new(
                    glim_services::preview::request::NativeContent::OpenApi {
                        source: self.path.clone(),
                        database: index.database.clone(),
                    },
                );
                match request.encode() {
                    Ok(configuration) => {
                        self.browser = Some(
                            cx.new(|_| NativeReader::openapi(self.path.clone(), &configuration)),
                        )
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
                cx.notify();
            } else {
                self.start(cx);
            }
        }
    }
    pub fn find(&self, show: bool, cx: &App) -> bool {
        self.browser
            .as_ref()
            .is_some_and(|browser| browser.read(cx).find(show))
    }
}
impl Render for OpenApiReader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(browser) = &self.browser {
            return browser.clone().into_any_element();
        }
        if let Some(error) = self.error.clone() {
            return div()
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .child(glim_services::locale::tr(
                    "Cannot load API reference. Switch to Source to view or edit the document.",
                ))
                .child(div().text_sm().child(error))
                .child(
                    Button::new("retry-api-reference")
                        .label(glim_services::locale::tr("Retry"))
                        .on_click(cx.listener(|view, _, _, cx| view.start(cx))),
                )
                .into_any_element();
        }
        super::preview_loading(cx)
    }
}
