use crate::app::reader_jobs::{self, JobClass, Priority};
use glim_services::paged::{PageSource, TextPage};
use gpui_kit::{
    component::{ActiveTheme, Disableable, button::Button, h_flex, v_flex},
    prelude::FluentBuilder,
    *,
};
use std::ops::Range;

type PreviewAction = std::rc::Rc<dyn Fn(&mut App)>;

/// Virtual rows contain slices into one buffer, not one allocation per line.
/// Paging replaces the buffer, so browsing an unbounded file stays bounded.
pub(super) struct PagedReader {
    memory: glim_core::resources::ResourceLease,
    suspended: bool,
    expanded: bool,
    resume_scroll: Point<Pixels>,
    pub source_editor: Option<super::source::SourceReader>,
    pub return_to_preview: Option<PreviewAction>,
    pub saving: std::rc::Rc<std::cell::Cell<bool>>,
    page: TextPage,
    revision: u64,
    _source_owner: Option<glim_services::work::CancelOnDrop>,
    report_source: Option<PageSource>,
    previous: Vec<u64>,
    task: Option<Task<()>>,
    error: Option<String>,
    scroll: UniformListScrollHandle,
}
impl PagedReader {
    pub fn new(page: TextPage, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let source_editor = page.source.is_editable().then(|| {
            super::source::SourceReader::new(
                &page.text,
                "plain",
                false,
                ScrollHandle::new(),
                window,
                cx,
            )
        });
        if let Some(editor) = &source_editor {
            editor
                .find
                .update(cx, |find, cx| find.current_page(window, cx));
        }
        Self::with_editor(page, source_editor)
    }
    pub fn readonly(page: TextPage) -> Self {
        Self::with_editor(page, None)
    }
    pub fn text_only(page: TextPage) -> Self {
        let mut view = Self::readonly(page);
        view.report_source = None;
        view
    }
    fn with_editor(mut page: TextPage, source_editor: Option<super::source::SourceReader>) -> Self {
        let memory_class = if source_editor.is_some() {
            glim_core::resources::ResourceClass::State
        } else {
            glim_core::resources::ResourceClass::Working
        };
        let mut reader = Self {
            memory: glim_services::resources::pool().track(0, memory_class),
            suspended: false,
            expanded: page.end.saturating_sub(page.start) > glim_services::paged::PAGE_BYTES,
            resume_scroll: point(px(0.), px(0.)),
            _source_owner: page.owner.take(),
            source_editor,
            return_to_preview: None,
            saving: std::rc::Rc::new(std::cell::Cell::new(false)),
            report_source: page.source.is_generated().then(|| page.source.clone()),
            page,
            revision: 0,
            previous: Vec::new(),
            task: None,
            error: None,
            scroll: UniformListScrollHandle::new(),
        };
        reader.account();
        reader
    }
    fn account(&mut self) {
        if self.source_editor.is_some() {
            self.page.memory = None;
        }
        self.memory.resize(if self.page.memory.is_some() {
            0
        } else {
            (self.page.text.capacity()
                + self.page.rows.capacity() * std::mem::size_of::<Range<usize>>())
                as u64
        });
    }
    pub fn suspend(&mut self) {
        if self.source_editor.is_some() || self.suspended {
            return;
        }
        if !self.page.rows.is_empty() {
            self.resume_scroll = self.scroll.0.borrow().base_handle.offset();
        }
        self.task = None;
        self.suspended = true;
        self.page.text = String::new();
        self.page.rows = Vec::new();
        self.page.memory = None;
        self.page.widest_row = None;
        self.account();
    }
    pub fn resume(&mut self, cx: &mut Context<Self>) {
        if !self.suspended {
            return;
        }
        self.suspended = false;
        let source = self.page.source.clone();
        let start = self.page.start;
        let expanded = self.expanded;
        let stamp = (self.page.total, self.page.modified, self.page.identity);
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |cancel| {
                let page = source.read_cancellable(start, expanded, cancel)?;
                anyhow::ensure!(
                    page.source.is_generated()
                        || (page.total, page.modified, page.identity) == stamp,
                    "File changed on disk. Reopen or refresh before paging."
                );
                Ok(page)
            },
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                match result {
                    Ok(page) => {
                        view.page = page;
                        view.error = None;
                        view.scroll
                            .0
                            .borrow()
                            .base_handle
                            .set_offset(view.resume_scroll);
                    }
                    Err(error) => view.error = Some(format!("{error:#}")),
                }
                view.account();
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn can_move_to_window(&self) -> bool {
        self.task.is_none()
    }

    pub fn refresh_revision(&self) -> u64 {
        self.revision
    }

    pub fn refresh_request(&self) -> Option<glim_services::paged::PageRefresh> {
        self.source_editor.as_ref()?;
        Some(glim_services::paged::PageRefresh {
            source: self.page.source.clone(),
            start: self.page.start,
            total: self.page.total,
            modified: self.page.modified,
            identity: self.page.identity,
        })
    }
    pub fn request_save_before_preview(&mut self, cx: &mut Context<Self>) {
        self.error = Some(glim_services::locale::tr(
            "Save this page before navigating.",
        ));
        cx.notify();
    }
    pub fn save_request(&mut self) -> Option<glim_services::paged::PageEdit> {
        self.source_editor.as_ref()?;
        self.saving.set(true);
        Some(glim_services::paged::PageEdit {
            path: self.page.path.clone(),
            start: self.page.start,
            end: self.page.end,
            total: self.page.total,
            modified: self.page.modified,
            identity: self.page.identity,
            expected: self.page.text.clone(),
        })
    }
    pub fn saved(
        &mut self,
        text: String,
        saved: glim_services::paged::PageSaved,
        cx: &mut Context<Self>,
    ) {
        self.revision += 1;
        self.saving.set(false);
        if let Some(editor) = &mut self.source_editor {
            editor.mark_saved(text.clone(), cx);
        }
        self.page.memory = None;
        self.page.text = text;
        self.page.end = saved.end;
        self.page.total = saved.total;
        self.page.modified = saved.modified;
        self.page.identity = saved.identity;
        self.account();
        cx.notify();
    }
    fn toggle_raw(&mut self, cx: &mut Context<Self>) {
        glim_services::telemetry::track("paged_mode_changed", []);
        if self.task.is_some() {
            return;
        }
        let Some(report) = self.report_source.clone() else {
            return;
        };
        let source = if self.page.source.is_generated() {
            report.raw()
        } else {
            report
        };
        self.revision += 1;
        let telemetry_started = std::time::Instant::now();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |cancel| source.read_cancellable(0, false, cancel),
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result("paged_load", None, telemetry_started, &result);
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                view.revision += 1;
                match result {
                    Ok(page) => {
                        view.page = page;
                        view.previous.clear();
                        view.expanded = false;
                        view.error = None;
                        view.scroll = UniformListScrollHandle::new();
                    }
                    Err(error) => {
                        glim_services::telemetry::operation_failed("paged_load", None, &error);
                        view.error = Some(format!("{error:#}"));
                    }
                }
                view.account();
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn load(
        &mut self,
        start: u64,
        all: bool,
        back: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        glim_services::telemetry::reader_feature(
            "paged",
            if all {
                "expand_page"
            } else if back {
                "previous_page"
            } else {
                "next_page"
            },
            [(
                "binary",
                glim_services::telemetry::flag(self.page.source.binary),
            )],
        );
        if self.task.is_some() || self.saving.get() {
            return;
        }
        if self
            .source_editor
            .as_ref()
            .is_some_and(|editor| editor.is_dirty())
        {
            self.error = Some(glim_services::locale::tr(
                "Save this page before navigating.",
            ));
            cx.notify();
            return;
        }
        let source = self.page.source.clone();
        let stamp = (self.page.total, self.page.modified, self.page.identity);
        self.revision += 1;
        let telemetry_started = std::time::Instant::now();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        self.task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = reader_jobs::run(
                cx.background_executor().clone(),
                JobClass::Projection,
                Priority::Interactive,
                cancel,
                move |cancel| {
                    let page = source.read_cancellable(start, all, cancel)?;
                    anyhow::ensure!(
                        page.source.is_generated()
                            || (page.total, page.modified, page.identity) == stamp,
                        "File changed on disk. Reopen or refresh before paging."
                    );
                    Ok::<_, anyhow::Error>(page)
                },
            )
            .await;
            glim_services::telemetry::result("paged_load", None, telemetry_started, &result);
            let _ = view.update_in(cx, |v, window, cx| {
                v.task = None;
                v.revision += 1;
                match result {
                    Ok(page) => {
                        if back {
                            v.previous.pop();
                        } else if page.start != v.page.start {
                            v.previous.push(v.page.start);
                        }
                        if let Some(editor) = &mut v.source_editor {
                            editor.edit.editor().update(cx, |state, cx| {
                                state.set_value(page.text.clone(), window, cx)
                            });
                            editor.mark_saved(page.text.clone(), cx);
                        }
                        v.page = page;
                        v.expanded = all;
                        v.error = None;
                        v.scroll = UniformListScrollHandle::new();
                    }
                    Err(e) => {
                        glim_services::telemetry::operation_failed("paged_load", None, &e);
                        v.error = Some(format!("{e:#}"));
                    }
                }
                v.account();
                cx.notify();
            });
        }));
        cx.notify();
    }
}
impl Render for PagedReader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.task.is_some() || self.saving.get();
        let previous = self.previous.last().copied();
        let next = self.page.end;
        v_flex()
            .relative()
            .size_full()
            .min_h_0()
            .children(self.error.iter().map(|e| {
                div()
                    .px_3()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(e.clone())
            }))
            .child(if let Some(editor) = &mut self.source_editor {
                editor.render(cx)
            } else {
                uniform_list(
                    "large-text",
                    self.page.rows.len() + 4,
                    cx.processor(|v, range: Range<usize>, _, cx| {
                        range
                            .map(|i| {
                                if i >= v.page.rows.len() {
                                    return div().h(px(22.));
                                }
                                div()
                                    .h(px(22.))
                                    .px_3()
                                    .text_sm()
                                    .font_family("Menlo")
                                    .whitespace_nowrap()
                                    .text_color(cx.theme().foreground)
                                    .child(v.page.text[v.page.rows[i].clone()].to_owned())
                            })
                            .collect()
                    }),
                )
                .with_width_from_item(self.page.widest_row)
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .track_scroll(&self.scroll)
                .flex_1()
                .min_h_0()
                .w_full()
                .into_any_element()
            })
            .child(super::controls::floating(
                "paged-controls",
                h_flex()
                    .px_0()
                    .py_0()
                    .gap_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(div().min_w_0().truncate().child(format!(
                        "{} · bytes {}–{} / {}{}",
                        glim_services::locale::tr(if self.source_editor.is_some() {
                            "Editable page"
                        } else {
                            "Read-only preview"
                        }),
                        self.page.start,
                        self.page.end,
                        self.page.total,
                        if self.page.total_known { "" } else { "+" }
                    )))
                    .when(self.error.is_some(), |bar| {
                        bar.child(
                            Button::new("retry-reader-page")
                                .label(glim_services::locale::tr("Retry"))
                                .disabled(busy)
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.load(view.page.start, view.expanded, false, window, cx);
                                })),
                        )
                    })
                    .when_some(self.return_to_preview.clone(), |bar, action| {
                        bar.child(
                            Button::new("page-return-preview")
                                .label(glim_services::locale::tr("Preview"))
                                .on_click(move |_, _, cx| action(cx)),
                        )
                    })
                    .when(self.source_editor.is_none(), |bar| {
                        bar.child(
                            crate::views::icon_button(
                                "copy-reader-page",
                                gpui_kit::assets::IconName::Copy,
                                glim_services::locale::tr("Copy page"),
                            )
                            .disabled(busy)
                            .on_click(cx.listener(|view, _, _, cx| {
                                glim_services::telemetry::reader_feature("paged", "copy_page", []);
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    view.page.text.clone(),
                                ));
                            })),
                        )
                    })
                    .when(self.report_source.is_some(), |bar| {
                        bar.child(
                            Button::new("page-raw")
                                .label(glim_services::locale::tr(
                                    if self.page.source.is_generated() {
                                        "File bytes"
                                    } else {
                                        "Report"
                                    },
                                ))
                                .disabled(busy)
                                .on_click(cx.listener(|view, _, _, cx| view.toggle_raw(cx))),
                        )
                    })
                    .child(
                        crate::views::icon_button(
                            "page-back",
                            gpui_kit::assets::IconName::ChevronLeft,
                            glim_services::locale::tr("Previous"),
                        )
                        .disabled(busy || previous.is_none())
                        .on_click(cx.listener(move |v, _, w, cx| {
                            if let Some(offset) = previous {
                                v.load(offset, false, true, w, cx);
                            }
                        })),
                    )
                    .child(
                        crate::views::icon_button(
                            "page-next",
                            gpui_kit::assets::IconName::ChevronRight,
                            glim_services::locale::tr("Next page"),
                        )
                        .disabled(busy || !self.page.more)
                        .on_click(
                            cx.listener(move |v, _, w, cx| v.load(next, false, false, w, cx)),
                        ),
                    )
                    .child(
                        Button::new("page-all")
                            .label(glim_services::locale::tr("Expand this page"))
                            .disabled(
                                busy || self.page.source.binary
                                    || (self.page.start == 0 && !self.page.more),
                            )
                            .on_click(
                                cx.listener(|v, _, w, cx| v.load(v.page.start, true, false, w, cx)),
                            ),
                    ),
                cx,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::PagedReader;
    use gpui_kit::{AppContext, TestAppContext, point, px};

    #[gpui_kit::test]
    fn cancelled_restoration_preserves_generated_source_and_position(cx: &mut TestAppContext) {
        let content = "αβ\n".repeat(512);
        let original = glim_services::paged::text_value(
            &content,
            &glim_services::work::Cancellation::default(),
        )
        .unwrap();
        let page = original.source.read(140, false).unwrap();
        let expected = page.text.clone();
        drop(original);
        let offset = point(px(0.), px(-64.));
        let view = cx.update(|cx| {
            cx.new(|_| {
                let mut view = PagedReader::readonly(page);
                view.previous.push(0);
                view.scroll.0.borrow().base_handle.set_offset(offset);
                view.account();
                view
            })
        });
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                assert!(
                    view.page
                        .memory
                        .as_ref()
                        .is_some_and(|memory| memory.bytes() > 0)
                );
                view.suspend();
                assert!(view.page.memory.is_none());
                view.resume(cx);
                view.suspend();
                view.resume(cx);
            })
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let view = view.read(cx);
            assert!(view.error.is_none(), "{:?}", view.error);
            assert_eq!(view.page.start, 140);
            assert_eq!(view.page.text, expected);
            assert_eq!(view.previous, [0]);
            assert_eq!(view.scroll.0.borrow().base_handle.offset(), offset);
            assert!(
                view.page
                    .memory
                    .as_ref()
                    .is_some_and(|memory| memory.bytes() > 0)
            );
        });
    }
}
