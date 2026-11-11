use super::Workspace;
use gpui_kit::{
    component::{
        ActiveTheme, Sizable, TitleBar,
        button::{Button, ButtonVariants},
        h_flex, v_flex,
    },
    prelude::FluentBuilder,
    *,
};
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Target {
    pub path: PathBuf,
}
impl Target {
    pub(super) fn from_reader(reader: &crate::views::reader::Reader) -> Self {
        Self {
            path: reader.document.path.clone(),
        }
    }
}
impl Workspace {
    pub(super) fn record_history(&mut self, cx: &mut Context<Self>) {
        self.remember_session(cx);
        if self.navigating {
            self.navigating = false;
            return;
        }
        let Some(r) = self.active_reader() else {
            return;
        };
        if r.document.draft_directory.is_some() {
            return;
        }
        let target = Target::from_reader(r);
        if self.history_cursor.and_then(|i| self.history.get(i)) == Some(&target) {
            return;
        }
        self.history
            .truncate(self.history_cursor.map_or(0, |i| i + 1));
        self.history.push(target);
        self.history_cursor = Some(self.history.len() - 1);
    }
    pub(super) fn navigate(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(cursor) = self.history_cursor else {
            return;
        };
        let next = if forward {
            cursor + 1
        } else {
            cursor.saturating_sub(1)
        };
        let Some(target) = self.history.get(next).cloned() else {
            return;
        };
        if next == cursor {
            return;
        }
        self.history_cursor = Some(next);
        self.navigating = true;
        self.open_path(target.path, window, cx);
    }
    pub(super) fn search_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_offset = 0;
        self.search_selected = 0;
        self.search_scroll.set_offset(point(px(0.), px(0.)));
        self.load_search_page(window, cx);
    }
    pub(super) fn load_search_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().to_string();
        self.results.clear();
        self.search_task = None;
        self.search_more = false;
        self.search_loading = false;
        self.search_error = None;
        self.search_selected = 0;
        self.search_scroll.set_offset(point(px(0.), px(0.)));
        if !self.search_visible || query.trim().is_empty() {
            self.search_offset = 0;
            cx.notify();
            return;
        }
        if glim_services::remote::is_remote_id(&self.recent_id) && self.remote.is_none() {
            self.search_error = Some(
                glim_services::locale::tr("Connect to this workspace to search files.").into(),
            );
            cx.notify();
            return;
        }
        self.search_loading = true;
        let roots = self.roots.clone();
        let remote = self.remote.clone();
        let offset = self.search_offset;
        let (cancel, guard) = glim_services::work::Cancellation::task();
        self.search_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            cx.background_executor()
                .timer(std::time::Duration::from_millis(150))
                .await;
            let result = cx
                .background_executor()
                .spawn(async move {
                    match remote {
                        Some(remote) => remote.search(&roots, &query, offset, &cancel),
                        None => {
                            glim_services::workspace::search_page(&roots, &query, offset, &cancel)
                        }
                    }
                })
                .await;
            let _ = view.update(cx, |view, cx| {
                view.search_loading = false;
                match result {
                    Ok(page) => {
                        glim_services::telemetry::track(
                            "workspace_search_completed",
                            [
                                (
                                    "result_count",
                                    glim_services::telemetry::number(page.paths.len() as u64),
                                ),
                                ("has_more", glim_services::telemetry::flag(page.more)),
                            ],
                        );
                        view.results = page.paths;
                        view.search_more = page.more;
                    }
                    Err(error) => {
                        glim_services::telemetry::operation_failed(
                            "workspace_search",
                            None,
                            &error,
                        );
                        view.search_error = Some(format!("{error:#}").into());
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn toggle_word_wrap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.word_wrap = !self.word_wrap;
        glim_services::telemetry::track(
            "word_wrap_changed",
            [("enabled", glim_services::telemetry::flag(self.word_wrap))],
        );
        let enabled = self.word_wrap;
        cx.global_mut::<crate::app::ReaderPreferences>().word_wrap = enabled;
        for reader in &self.tabs {
            reader.set_word_wrap(enabled, window, cx);
        }
        let write = cx
            .background_executor()
            .spawn(async move { glim_services::preferences::save_word_wrap(enabled) });
        self.preference_task = Some(cx.spawn(async move |view, cx| {
            if let Err(error) = write.await {
                let _ = view.update(cx, |v, cx| {
                    v.error = Some(format!("Cannot remember word wrap: {error:#}").into());
                    cx.notify();
                });
            }
        }));
        cx.notify();
    }

    pub(super) fn set_reader_preview(
        &mut self,
        preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.active else {
            return;
        };
        let preference = self.tabs[index].preview_preference();
        self.tabs[index].set_preview(preview, cx);
        self.remember_session(cx);
        if (self.tabs[index].mode() == glim_core::ReaderMode::Preview) != preview {
            cx.notify();
            return;
        }
        glim_services::telemetry::file_event(
            "preview_mode_changed",
            &self.tabs[index].document.path,
            [("preview", glim_services::telemetry::flag(preview))],
        );
        if !preview {
            self.tabs[index].focus_source(window, cx);
        }
        if let Some(kind) = preference {
            let prefs = cx.global_mut::<crate::app::ReaderPreferences>();
            match kind {
                "html" => prefs.html_preview = preview,
                "csv" => prefs.csv_preview = preview,
                _ => prefs.markdown_preview = preview,
            }
            let previous = prefs.preview_save_task.take();
            let task = cx.spawn(async move |view, cx| {
                if let Some(previous) = previous {
                    previous.await;
                }
                let result = cx
                    .background_executor()
                    .spawn(
                        async move { glim_services::preferences::save_preview_mode(kind, preview) },
                    )
                    .await;
                if let Err(error) = result {
                    let _ = view.update(cx, |v, cx| {
                        v.error = Some(format!("Cannot remember reading mode: {error:#}").into());
                        cx.notify();
                    });
                }
            });
            cx.global_mut::<crate::app::ReaderPreferences>()
                .preview_save_task = Some(task);
        }
        cx.notify();
    }

    pub(super) fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.file_fullscreen {
            let name = self
                .active_reader()
                .map(|reader| {
                    if reader.document.draft_directory.is_some() {
                        return reader.title.to_string();
                    }
                    reader
                        .document
                        .path
                        .file_name()
                        .unwrap_or(reader.document.path.as_os_str())
                        .to_string_lossy()
                        .into_owned()
                })
                .unwrap_or_default();
            return workspace_title_bar(
                h_flex()
                    .size_full()
                    .pl(px(94.))
                    .pr_3()
                    .gap_3()
                    .child(div().flex_1().min_w_0().truncate().text_sm().child(name))
                    .child(self.workspace_toolbar(cx)),
                cx,
            );
        }
        v_flex()
            .relative()
            .w_full()
            .flex_shrink_0()
            .child(workspace_title_bar(
                h_flex()
                    .relative()
                    .w_full()
                    .h_full()
                    .justify_center()
                    .child(
                        h_flex()
                            .absolute()
                            .left(px(94.))
                            .top_0()
                            .h_full()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .h(px(28.))
                                    .flex()
                                    .items_center()
                                    .text_size(px(16.))
                                    .line_height(px(16.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Glim"),
                            )
                            .child(div().w(px(1.)).h(px(16.)).mx_1().bg(cx.theme().border))
                            .child(self.navigation_toolbar(cx)),
                    )
                    .child(
                        h_flex()
                            .absolute()
                            .right(px(12.))
                            .top_0()
                            .h_full()
                            .gap_1()
                            .child(self.workspace_toolbar(cx))
                            .when(self.saving.is_some() && self.save_cancel.is_some(), |row| {
                                row.child(
                                    Button::new("cancel-file-save")
                                        .ghost()
                                        .small()
                                        .label(glim_services::locale::tr("Cancel save"))
                                        .on_click(
                                            cx.listener(|view, _, _, cx| view.cancel_save(cx)),
                                        ),
                                )
                            }),
                    ),
                cx,
            ))
            .into_any_element()
    }
}

fn workspace_title_bar(content: impl IntoElement, cx: &App) -> AnyElement {
    TitleBar::new()
        .bg(cx.theme().sidebar)
        .border_b_0()
        .h(px(48.))
        .pl_0()
        .pr_0()
        .child(content)
        .into_any_element()
}
