use super::Workspace;
use glim_services::{limits, locale::tr};
use gpui_kit::{
    assets::IconName,
    component::{
        ActiveTheme, Disableable, Icon, Selectable, Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
        h_flex,
        input::Input,
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};
use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};

impl Workspace {
    pub(super) fn load_content_search(
        &mut self,
        query: String,
        debounce: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_loading = true;
        let roots = self.roots.clone();
        let remote = self.remote.clone();
        let offset = self.search_offset;
        let (cancel, guard) = glim_services::work::Cancellation::task();
        self.search_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            if debounce {
                cx.background_executor()
                    .timer(Duration::from_millis(limits::WORKSPACE_SEARCH_DEBOUNCE_MS))
                    .await;
            }
            let result = crate::app::reader_jobs::run(
                cx.background_executor().clone(),
                crate::app::reader_jobs::JobClass::ContentSearch,
                crate::app::reader_jobs::Priority::Interactive,
                cancel,
                move |cancel| match remote {
                    Some(remote) => remote.search_content(&roots, &query, offset, cancel),
                    None => glim_services::content_search::search(&roots, &query, offset, cancel),
                },
            )
            .await;
            let _ = view.update(cx, |view, cx| {
                view.search_loading = false;
                match result {
                    Ok(page) => {
                        view.results = page.matches.iter().map(|hit| hit.path.clone()).collect();
                        view.content_results = page.matches;
                        view.search_more = page.more;
                        view.search_error = page.warning.map(Into::into);
                    }
                    Err(error) => view.search_error = Some(format!("{error:#}").into()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn set_content_search(&mut self, content: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_content == content {
            return;
        }
        self.search_content = content;
        self.search_files(window, cx);
        self.search.update(cx, |input, cx| input.focus(window, cx));
    }

    fn watch_search_index_progress(
        &mut self,
        progress: Arc<glim_services::remote::SearchIndexProgress>,
        cx: &mut Context<Self>,
    ) {
        self.remote_index_progress_task = Some(cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(limits::REMOTE_SEARCH_POLL_MS))
                    .await;
                let files = progress.files();
                let cached_ready = progress.cache_ready();
                if !view
                    .update(cx, |view, cx| {
                        if cached_ready && !view.search_index_ready {
                            view.search_index_ready = true;
                            view.search_indexing = false;
                            cx.notify();
                        }
                        if let Some(files) = files
                            && (!view.search_indexing || view.search_indexed_files != Some(files))
                        {
                            view.search_indexing = true;
                            view.search_index_ready = false;
                            view.search_indexed_files = Some(files);
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
        }));
    }

    fn search_is_scanning(&self) -> bool {
        self.search_indexing
            || (self.search_visible
                && self.search_partial
                && !self.search_index_ready
                && self.search_index_error.is_none()
                && self.search_error.is_none())
    }

    fn search_status(&self) -> Option<String> {
        if self.search_content {
            return Some(tr(if self.search_loading {
                "Searching file contents…"
            } else if self.search_error.is_some() {
                "Content search is incomplete"
            } else {
                "Case-sensitive · Text files"
            }));
        }
        if self.search_indexing {
            Some(self.search_indexed_files.map_or_else(
                || tr("Preparing file index…"),
                |count| {
                    tr("Indexing files: {count} scanned").replace("{count}", &count.to_string())
                },
            ))
        } else if self.search_error.is_some() {
            Some(tr("Remote search is incomplete"))
        } else if self.search_index_error.is_some() {
            Some(tr("File index will retry automatically"))
        } else if self.search_is_scanning() {
            Some(tr("Scanning remote files…"))
        } else if self.search_loading {
            Some(tr("Searching files…"))
        } else if self.search_index_ready {
            Some(self.search_indexed_files.map_or_else(
                || tr("File index is ready"),
                |count| tr("File index: {count} files").replace("{count}", &count.to_string()),
            ))
        } else {
            None
        }
    }

    pub(super) fn search_button(&self, cx: &mut Context<Self>) -> Button {
        let hint_bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::default()));
        let painted_bounds = hint_bounds.clone();
        let scanning = self.search_is_scanning() || self.search_loading;
        let mut label = format!("{} · ⌘P", tr("Search workspace files"));
        if let Some(status) = self.search_status() {
            label.push('\n');
            label.push_str(&status);
        }
        let color = crate::views::accent(cx);
        let icon = div()
            .relative()
            .size(px(16.))
            .flex_shrink_0()
            .child(Icon::new(IconName::Search).size(px(16.)))
            .when(scanning, |icon| {
                icon.child(div().absolute().inset_0().size(px(16.)).with_animation(
                    "workspace-search-progress",
                    Animation::new(Duration::from_millis(1200)).repeat(),
                    move |this, phase| {
                        this.child(
                            canvas(
                                |_, _, _| {},
                                move |bounds, _, window, _| {
                                    // The existing 24-unit search icon has its lens at (11, 11).
                                    let unit = bounds.size.width / 24.;
                                    let center = bounds.origin + point(unit * 11., unit * 11.);
                                    let radius = unit * 6.;
                                    let angle =
                                        phase * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                                    let edge = |angle: f32| {
                                        center + point(radius * angle.cos(), radius * angle.sin())
                                    };
                                    let mut sector = PathBuilder::fill();
                                    sector.move_to(center);
                                    sector.line_to(edge(angle));
                                    sector.arc_to(
                                        point(radius, radius),
                                        px(0.),
                                        false,
                                        true,
                                        edge(angle + std::f32::consts::FRAC_PI_2),
                                    );
                                    sector.close();
                                    if let Ok(path) = sector.build() {
                                        window.paint_path(path, color);
                                    }
                                },
                            )
                            .size(px(16.)),
                        )
                    },
                ))
            });
        Button::new("workspace-search")
            .custom(
                ButtonCustomVariant::new(cx)
                    .hover(cx.theme().foreground.opacity(0.05))
                    .active(cx.theme().foreground.opacity(0.09)),
            )
            .small()
            .w(px(28.))
            .h(px(28.))
            .p_0()
            .flex_shrink_0()
            .rounded(px(7.))
            .child(icon)
            .child(
                canvas(
                    move |bounds, _, _| painted_bounds.set(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .accessibility_label(label.clone())
            .on_hover(cx.listener(move |view, hovered: &bool, window, _| {
                if *hovered {
                    view.path_hint
                        .show(&label, "", false, hint_bounds.get(), window);
                } else {
                    view.path_hint.hide_for("", hint_bounds.get());
                }
            }))
            .selected(self.search_visible)
            .on_click(cx.listener(|view, _, window, cx| view.show_search(window, cx)))
    }

    pub(super) fn start_remote_search_index(
        &mut self,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.roots.is_empty() {
            self.remote_index_task = None;
            self.remote_index_progress_task = None;
            self.remote_index_scope = None;
            self.search_indexing = false;
            self.search_index_ready = false;
            self.search_indexed_files = None;
            self.search_index_error = None;
            return;
        }
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
        self.remote_index_progress_task = None;
        self.remote_index_scope = Some(scope);
        self.search_indexing = true;
        self.search_indexed_files = None;
        self.search_index_ready = false;
        self.search_index_error = None;
        self.search_error = None;
        let (cancel, guard) = glim_services::work::Cancellation::task();
        self.remote_index_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let mut force = force;
            let mut failures: u32 = 0;
            loop {
                let progress = Arc::new(glim_services::remote::SearchIndexProgress::default());
                if view
                    .update(cx, |view, cx| {
                        view.watch_search_index_progress(progress.clone(), cx);
                    })
                    .is_err()
                {
                    break;
                }
                let worker = remote.clone();
                let roots = roots.clone();
                let worker_progress = progress.clone();
                let result = crate::app::reader_jobs::run(
                    cx.background_executor().clone(),
                    crate::app::reader_jobs::JobClass::RemoteSearch,
                    crate::app::reader_jobs::Priority::Background,
                    cancel.clone(),
                    move |cancel| {
                        worker.refresh_search_index(&roots, force, cancel, &worker_progress)
                    },
                )
                .await;
                let busy = matches!(result, Ok(glim_services::remote::SearchIndexUpdate::Busy));
                failures = if result.is_err() {
                    failures.saturating_add(1)
                } else {
                    0
                };
                if view
                    .update(cx, |view, cx| {
                        view.search_indexing = busy && !view.search_index_ready;
                        view.remote_index_progress_task = None;
                        if !busy {
                            view.search_indexed_files = progress.files();
                            view.search_index_ready = matches!(
                                result,
                                Ok(glim_services::remote::SearchIndexUpdate::Ready(_))
                            );
                            let previous_error = view.search_index_error.take();
                            if !view.search_content && view.search_error == previous_error {
                                view.search_error = None;
                            }
                            let warning = match &result {
                                Ok(
                                    glim_services::remote::SearchIndexUpdate::Ready(warning)
                                    | glim_services::remote::SearchIndexUpdate::Pending(warning),
                                ) => warning.clone(),
                                Err(error) => Some(format!("{error:#}")),
                                _ => None,
                            };
                            if let Some(warning) = warning {
                                let message: SharedString = warning.into();
                                view.search_index_error = Some(message);
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                if !busy {
                    force = false;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(if failures > 0 {
                        glim_services::remote::search_retry_delay(failures)
                    } else if busy {
                        limits::REMOTE_SEARCH_POLL_MS
                    } else {
                        limits::REMOTE_SEARCH_DIRECTORY_POLL_MS
                    }))
                    .await;
            }
        }));
        cx.notify();
    }

    fn retry_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.search_content {
            self.start_remote_search_index(true, window, cx);
        }
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
        self.content_results.clear();
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
        if self.search_content {
            return Vec::new();
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

    fn open_search_result(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.search_paths(cx).get(index).cloned() else {
            return;
        };
        let location = self
            .content_results
            .get(index)
            .filter(|_| self.search_content)
            .map(|hit| hit.offset);
        self.close_search(window, cx);
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.sidebar_visible = true;
        self.pending_search_location = location.map(|offset| (path.clone(), offset));
        self.open_path(path, window, cx);
        self.reveal_search_location(window, cx);
    }

    pub(crate) fn reveal_content_at(
        &mut self,
        path: &std::path::Path,
        offset: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_search_location = Some((path.to_path_buf(), offset));
        self.reveal_search_location(window, cx);
    }

    pub(super) fn reveal_search_location(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, offset)) = &self.pending_search_location else {
            return;
        };
        let Some(index) = self
            .active
            .filter(|index| self.tabs[*index].document.path == *path)
        else {
            return;
        };
        self.tabs[index].reveal_content_offset(*offset, window, cx);
        self.pending_search_location = None;
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
                self.open_search_result(self.search_selected, window, cx);
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
            let message = if self.search_loading {
                if self.search_content {
                    "Searching file contents…"
                } else {
                    "Searching files…"
                }
            } else if empty_query && self.search_content {
                "Enter text to search file contents"
            } else if !self.search_content && self.search_index_error.is_some() {
                "No matches in cached files. Indexing will retry automatically."
            } else if self.search_indexing && !self.search_content {
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
                let hit = self
                    .content_results
                    .get(index)
                    .filter(|_| self.search_content);
                let mut name = path
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
                if let Some(hit) = hit {
                    name = format!("{name}:{}", hit.line);
                }
                Button::new(("workspace-search-result", index))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .hover(cx.theme().secondary)
                            .active(cx.theme().secondary),
                    )
                    .small()
                    .w_full()
                    .h(px(if hit.is_some() { 66. } else { 48. }))
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
                                    )
                                    .when_some(hit, |column, hit| {
                                        column.child(
                                            div().truncate().text_xs().child(hit.snippet.clone()),
                                        )
                                    }),
                            ),
                    )
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.open_search_result(index, window, cx)
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
                        h_flex()
                            .flex_shrink_0()
                            .px_3()
                            .py_2()
                            .gap_1()
                            .child(
                                Button::new("search-mode-filenames")
                                    .small()
                                    .ghost()
                                    .label(tr("File names"))
                                    .selected(!self.search_content)
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.set_content_search(false, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("search-mode-content")
                                    .small()
                                    .ghost()
                                    .label(tr("File contents"))
                                    .selected(self.search_content)
                                    .tooltip(tr("Case-sensitive · Text files"))
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.set_content_search(true, window, cx)
                                    })),
                            ),
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
                                    .child(
                                        self.search_status().unwrap_or_else(|| {
                                            tr("↑↓ Select · ↵ Open · Esc Close")
                                        }),
                                    ),
                            )
                            .when(self.remote.is_some() || self.search_content, |footer| {
                                footer.child(
                                    crate::views::icon_button(
                                        "refresh-remote-search",
                                        IconName::RefreshCw,
                                        self.search_error
                                            .as_ref()
                                            .or(self.search_index_error.as_ref())
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
