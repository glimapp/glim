use super::Workspace;
use glim_services::{limits, locale::tr};
use gpui_kit::{
    assets::IconName,
    component::{
        ActiveTheme, Disableable, Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
        h_flex,
        input::Input,
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};
use std::{collections::HashSet, path::PathBuf};

impl Workspace {
    pub(super) fn start_remote_search_index(
        &mut self,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(remote) = self.remote.clone() else {
            return;
        };
        let roots = self.roots.clone();
        let scope = remote.search_scope(&roots);
        if !force
            && self.remote_index_task.is_some()
            && self.remote_index_scope.as_ref() == Some(&scope)
        {
            return;
        }
        self.remote_index_task = None;
        self.remote_index_scope = Some(scope);
        self.search_indexing = true;
        self.search_error = None;
        let (cancel, guard) = glim_services::work::Cancellation::task();
        self.remote_index_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let mut force = force;
            loop {
                let worker = remote.clone();
                let roots = roots.clone();
                let result = crate::app::reader_jobs::run(
                    cx.background_executor().clone(),
                    crate::app::reader_jobs::JobClass::RemoteSearch,
                    crate::app::reader_jobs::Priority::Background,
                    cancel.clone(),
                    move |cancel| worker.refresh_search_index(&roots, force, cancel),
                )
                .await;
                let busy = matches!(result, Ok(false));
                let keep = view
                    .update(cx, |view, cx| {
                        view.search_indexing = busy;
                        if let Err(error) = &result {
                            view.search_error = Some(format!("{error:#}").into());
                        }
                        let keep = result.is_ok() && (busy || view.search_visible);
                        if !keep {
                            view.remote_index_task = None;
                        }
                        cx.notify();
                        keep
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
                if !busy {
                    force = false;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(if busy {
                        limits::REMOTE_SEARCH_POLL_MS
                    } else {
                        limits::REMOTE_SEARCH_REFRESH_MS
                    }))
                    .await;
                if !view
                    .update(cx, |view, cx| {
                        if !view.search_visible {
                            view.remote_index_task = None;
                            return false;
                        }
                        view.search_indexing = true;
                        cx.notify();
                        true
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
        }));
        cx.notify();
    }

    fn retry_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_remote_search_index(true, window, cx);
        self.search_files(window, cx);
    }

    pub(super) fn show_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.remote_dialog.is_some() {
            return;
        }
        if !self.search_visible {
            self.search_previous_focus = window.focused(cx);
            self.search_visible = true;
            self.start_remote_search_index(false, window, cx);
            self.search_files(window, cx);
        }
        self.search.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.search_visible {
            return;
        }
        self.search_visible = false;
        self.search_task = None;
        self.search_loading = false;
        self.search_error = None;
        self.results.clear();
        self.search_offset = 0;
        self.search_more = false;
        if let Some(focus) = self.search_previous_focus.take() {
            window.focus(&focus, cx);
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    fn search_paths(&self, cx: &App) -> Vec<PathBuf> {
        if !self.search.read(cx).value().trim().is_empty() {
            return self.results.clone();
        }
        let mut seen = HashSet::new();
        self.history
            .iter()
            .rev()
            .map(|target| &target.path)
            .chain(
                self.tabs
                    .iter()
                    .rev()
                    .filter(|reader| reader.document.draft_directory.is_none())
                    .map(|reader| &reader.document.path),
            )
            .filter(|path| seen.insert((*path).clone()))
            .take(limits::WORKSPACE_SEARCH_RESULTS)
            .cloned()
            .collect()
    }

    fn open_search_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.close_search(window, cx);
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.sidebar_visible = true;
        self.open_path(path, window, cx);
    }

    pub(super) fn search_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input_focused = self.search.read(cx).focus_handle(cx).is_focused(window);
        if input_focused
            && self.search.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            })
        {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.close_search(window, cx);
            cx.stop_propagation();
            return;
        }
        if !input_focused || event.keystroke.modifiers.modified() {
            return;
        }
        let paths = self.search_paths(cx);
        match key {
            "up" | "down" if !paths.is_empty() => {
                self.search_selected = if key == "up" {
                    self.search_selected.saturating_sub(1)
                } else {
                    (self.search_selected + 1).min(paths.len() - 1)
                };
                self.search_scroll.scroll_to_item(self.search_selected);
                cx.stop_propagation();
                cx.notify();
            }
            "enter" => {
                if let Some(path) = paths.get(self.search_selected) {
                    self.open_search_path(path.clone(), window, cx);
                }
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    pub(super) fn search_dialog(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let paths = self.search_paths(cx);
        let empty_query = self.search.read(cx).value().trim().is_empty();
        let owner = cx.entity().downgrade();
        let mut list = v_flex()
            .id("workspace-search-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.search_scroll)
            .px_2()
            .pb_2();

        if let Some(error) = self
            .search_error
            .as_ref()
            .filter(|_| paths.is_empty() && !empty_query)
        {
            let detail = error.clone();
            list = list.child(
                v_flex()
                    .p_3()
                    .gap_3()
                    .child(div().text_sm().child(tr("Search failed")))
                    .child(div().text_sm().whitespace_normal().child(error.clone()))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("search-retry")
                                    .small()
                                    .label(tr("Retry"))
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.retry_search(window, cx)
                                    })),
                            )
                            .child(
                                Button::new("search-copy-error")
                                    .small()
                                    .ghost()
                                    .label(tr("Copy error"))
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            detail.to_string(),
                                        ))
                                    }),
                            ),
                    ),
            );
        } else if paths.is_empty() {
            let message = if self.search_loading || self.search_indexing {
                "Searching files…"
            } else if empty_query {
                "No recently opened files"
            } else {
                "No matching files"
            };
            list = list.child(
                v_flex()
                    .p_3()
                    .gap_3()
                    .child(
                        div()
                            .id("workspace-search-status")
                            .role(Role::Status)
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(message)),
                    )
                    .when(empty_query && self.roots.is_empty(), |view| {
                        view.child(
                            Button::new("search-open-folder")
                                .small()
                                .label(tr("Open folder"))
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.close_search(window, cx);
                                    view.choose_path(true, window, cx);
                                })),
                        )
                    }),
            );
        } else {
            list = list.children(paths.iter().enumerate().map(|(index, path)| {
                let target = path.clone();
                let name = path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
                    .into_owned();
                let parent = path.parent().unwrap_or(path);
                let root = self
                    .roots
                    .iter()
                    .filter(|root| path.starts_with(root))
                    .max_by_key(|root| root.components().count());
                let location = root
                    .and_then(|root| root.parent())
                    .and_then(|base| parent.strip_prefix(base).ok())
                    .unwrap_or(parent)
                    .to_string_lossy()
                    .into_owned();
                Button::new(("workspace-search-result", index))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .hover(cx.theme().secondary)
                            .active(cx.theme().secondary),
                    )
                    .small()
                    .w_full()
                    .h(px(48.))
                    .flex_shrink_0()
                    .px_3()
                    .rounded(px(8.))
                    .when(index == self.search_selected, |row| {
                        row.bg(crate::views::selection_background(cx))
                    })
                    .tooltip(path.to_string_lossy().into_owned())
                    .accessibility_label(format!("{} {}", tr("Open"), path.display()))
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .gap_3()
                            .child(crate::views::file_icons::file_icon(path, cx))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().truncate().text_sm().child(name))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(location),
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.open_search_path(target.clone(), window, cx)
                    }))
            }));
        }

        gpui_kit::base::Dialog::new(cx)
            .focus_handle(self.search_focus.clone())
            .on_ok(|_, _, _| false)
            .request_close(move |_, window, cx| {
                let _ = owner.update(cx, |view, cx| view.close_search(window, cx));
            })
            .backdrop(div().size_full().bg(cx.theme().overlay))
            .popup(
                v_flex()
                    .id("workspace-search-dialog")
                    .w(px(680.))
                    .max_w((window.viewport_size().width - px(32.)).max(px(0.)))
                    .h(px(420.))
                    .max_h((window.viewport_size().height - px(48.)).max(px(0.)))
                    .rounded(px(16.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .shadow_lg()
                    .occlude()
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .p_3()
                            .gap_2()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(
                                Input::new(&self.search)
                                    .h(px(36.))
                                    .text_size(px(16.))
                                    .appearance(false)
                                    .flex_1()
                                    .min_w_0(),
                            )
                            .child(
                                crate::views::icon_button("search-close", IconName::X, tr("Close"))
                                    .ghost()
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.close_search(window, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .px_5()
                            .py_2()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(if empty_query {
                                "Recently opened files"
                            } else {
                                "Files"
                            })),
                    )
                    .child(list)
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .h(px(40.))
                            .px_4()
                            .gap_2()
                            .border_t_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .min_w_0()
                                    .truncate()
                                    .child(tr(if self.search_error.is_some() {
                                        "Remote search is incomplete"
                                    } else if self.search_indexing || self.search_partial {
                                        "Scanning remote files…"
                                    } else {
                                        "↑↓ Select · ↵ Open · Esc Close"
                                    })),
                            )
                            .when(self.remote.is_some(), |footer| {
                                footer.child(
                                    crate::views::icon_button(
                                        "refresh-remote-search",
                                        IconName::RefreshCw,
                                        self.search_error
                                            .as_ref()
                                            .map(|error| format!("{}: {error}", tr("Retry")))
                                            .unwrap_or_else(|| tr("Refresh file search")),
                                    )
                                    .ghost()
                                    .on_click(cx.listener(
                                        |view, _, window, cx| view.retry_search(window, cx),
                                    )),
                                )
                            })
                            .when(self.search_offset > 0 || self.search_more, |footer| {
                                footer
                                    .child(
                                        crate::views::icon_button(
                                            "search-previous",
                                            IconName::ChevronLeft,
                                            tr("Previous"),
                                        )
                                        .ghost()
                                        .disabled(self.search_offset == 0 || self.search_loading)
                                        .on_click(
                                            cx.listener(|view, _, window, cx| {
                                                view.search_offset =
                                                    view.search_offset.saturating_sub(
                                                        limits::WORKSPACE_SEARCH_RESULTS,
                                                    );
                                                view.load_search_page(window, cx);
                                            }),
                                        ),
                                    )
                                    .child(
                                        crate::views::icon_button(
                                            "search-next",
                                            IconName::ChevronRight,
                                            tr("Next page"),
                                        )
                                        .ghost()
                                        .disabled(!self.search_more || self.search_loading)
                                        .on_click(
                                            cx.listener(|view, _, window, cx| {
                                                view.search_offset =
                                                    view.search_offset.saturating_add(
                                                        limits::WORKSPACE_SEARCH_RESULTS,
                                                    );
                                                view.load_search_page(window, cx);
                                            }),
                                        ),
                                    )
                            }),
                    ),
            )
            .into_any_element()
    }
}
