//! Application adapter for structured-tree loading and source reconstruction.
mod view;
use self::view::JsonView;
use crate::app::reader_jobs::{self, JobClass, Priority};
use glim_core::resources::TextSnapshot;
use gpui_kit::{
    component::{ActiveTheme, button::Button, v_flex},
    prelude::FluentBuilder,
    *,
};
use std::sync::Arc;

pub(super) struct JsonTree {
    view: Entity<JsonView>,
    suspended: bool,
    task: Option<Task<()>>,
    error: Option<String>,
    format: String,
    loaded_text: Option<Arc<TextSnapshot>>,
}
impl JsonTree {
    pub fn new(format: String, cx: &mut Context<Self>) -> Self {
        Self {
            view: cx.new(|_| {
                JsonView::new(
                    super::host(),
                    glim_services::limits::JSON_AUTO_EXPAND_CHILDREN,
                    super::source::show_text_dialog,
                )
            }),
            suspended: false,
            task: None,
            error: None,
            format,
            loaded_text: None,
        }
    }
    pub fn input_snapshot(&self) -> Option<&Arc<TextSnapshot>> {
        self.loaded_text.as_ref()
    }
    pub fn load(&mut self, text: Arc<TextSnapshot>, cx: &mut Context<Self>) {
        let same = self
            .loaded_text
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &text) || old.as_str() == text.as_str());
        if same && !self.suspended {
            return;
        }
        self.suspended = false;
        let preserve_expansion = same && self.view.read(cx).initialized;
        self.task = None;
        self.loaded_text = Some(text.clone());
        self.view.update(cx, |view, cx| {
            view.clear(!same);
            cx.notify();
        });
        self.error = None;
        let format = self.format.clone();
        let pool = glim_services::resources::pool().clone();
        let telemetry_started = std::time::Instant::now();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |cancel| {
                glim_services::json::parse_format_cancellable(text.as_str(), &format, cancel, &pool)
            },
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result("data_tree_load", None, telemetry_started, &result);
            let _ = view.update(cx, |v, cx| {
                v.task = None;
                let result = result.and_then(|nodes| {
                    v.view.update(cx, |view, cx| {
                        let result = view
                            .publish(nodes, preserve_expansion)
                            .map_err(anyhow::Error::from);
                        cx.notify();
                        result
                    })
                });
                if let Err(error) = result {
                    v.error = Some(format!(
                        "Cannot show JSON tree: {error}. Switch to Source to view or edit."
                    ));
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub fn suspend(&mut self, cx: &mut App) {
        let remember_scroll = !self.suspended && self.task.is_none() && self.error.is_none();
        self.suspended = true;
        self.task = None;
        self.view.update(cx, |view, cx| {
            view.suspend(remember_scroll);
            cx.notify();
        });
    }
    pub fn resume(&mut self, cx: &mut Context<Self>) {
        if self.suspended
            && let Some(text) = self.loaded_text.clone()
        {
            self.load(text, cx);
        }
    }
}
impl Render for JsonTree {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .min_h_0()
            .when(self.task.is_some(), |v| {
                v.child(
                    div()
                        .p_4()
                        .child(glim_services::locale::tr("Loading JSON tree…")),
                )
            })
            .when_some(self.error.clone(), |v, error| {
                v.child(
                    div()
                        .p_4()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(error)
                        .child(
                            Button::new("json-tree-retry")
                                .label(glim_services::locale::tr("Retry"))
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.suspended = true;
                                    view.resume(cx);
                                })),
                        ),
                )
            })
            .child(self.view.clone())
    }
}
