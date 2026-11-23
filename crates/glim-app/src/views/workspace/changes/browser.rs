use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    Changes,
    History,
}

impl Changes {
    pub(super) fn cancel_browse(&mut self) {
        self.browse_generation += 1;
        self.browse_cancel = None;
        self.browse_task = None;
        self.browsing = false;
    }

    pub(super) fn activate_root(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.active_root.as_ref() == Some(&root) {
            return;
        }
        self.reset_selection(cx);
        self.active_root = Some(root);
        self.history_path = None;
        self.load(self.roots.clone(), self.remote.clone(), cx);
    }

    fn reset_selection(&mut self, cx: &mut Context<Self>) {
        self.cancel_browse();
        self.patch_cancel = None;
        self.patch_task = None;
        self.patch_loading = false;
        self.selection = None;
        self.comparison = None;
        self.selected_commit = None;
        self.commits = Arc::default();
        self.reset_history();
        self.clear_patch(cx);
    }

    pub(super) fn switch_section(&mut self, section: Section, cx: &mut Context<Self>) {
        if self.section == section {
            return;
        }
        self.reset_selection(cx);
        self.history_path = None;
        self.section = section;
        if section == Section::History {
            self.browse(cx);
        } else if let Some(repository) = self
            .repositories
            .iter()
            .find(|repo| Some(&repo.root) == self.active_root.as_ref())
            && let Some(change) = repository.changes.first().cloned()
        {
            self.select(repository.root.clone(), change, cx);
        }
        cx.notify();
    }

    pub(in crate::views::workspace) fn open_file_history(
        &mut self,
        root: PathBuf,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.reset_selection(cx);
        self.section = Section::History;
        let needs_status = self.active_root.as_ref() != Some(&root) || self.loading;
        self.active_root = Some(root);
        self.history_path = Some(path);
        if needs_status {
            self.refresh(cx);
        } else {
            self.browse(cx);
        }
        cx.emit(ShowDiff);
    }

    pub(super) fn open_comparison(&mut self, comparison: Comparison, cx: &mut Context<Self>) {
        self.show_comparison(comparison, cx);
        cx.emit(ShowDiff);
    }

    pub(super) fn show_comparison(&mut self, comparison: Comparison, cx: &mut Context<Self>) {
        let Some(root) = self.active_root.clone() else {
            return;
        };
        self.selection = None;
        self.comparison = Some((root.clone(), comparison.clone()));
        self.load_patch(root, None, Some(comparison), cx);
    }

    pub(super) fn reset_history(&mut self) {
        self.history_limit = git::HISTORY_PAGE;
        self.history_more = false;
        self.history_scroll = UniformListScrollHandle::new();
    }

    fn append_history(&mut self, generation: u64, cx: &mut Context<Self>) {
        if self.section != Section::History
            || self.browse_generation != generation
            || self.browsing
            || !self.history_more
            || self.browser_error.is_some()
        {
            return;
        }
        let (Some(root), Some(last)) = (self.active_root.clone(), self.commits.last()) else {
            return;
        };
        let last_id = last.id.clone();
        let offset = self.commits.len() - 1;
        let search = self.history_search.clone();
        let path = self.history_path.clone();
        let remote = self.remote.clone();
        self.browsing = true;
        self.browse_generation += 1;
        let generation = self.browse_generation;
        let (cancel, guard) = Cancellation::task();
        self.browse_cancel = Some(guard);
        let query = cx.background_executor().spawn(async move {
            git::history(
                &root,
                remote.as_ref(),
                offset,
                git::HISTORY_PAGE + 1,
                &search,
                path.as_deref(),
                &cancel,
            )
        });
        self.browse_task = Some(cx.spawn(async move |view, cx| {
            let result = query.await;
            let _ = view.update(cx, |view, cx| {
                if view.browse_generation != generation {
                    return;
                }
                view.browsing = false;
                match result {
                    Ok(page)
                        if page
                            .commits
                            .first()
                            .is_some_and(|commit| commit.id == last_id) =>
                    {
                        view.history_more = page.more;
                        Arc::make_mut(&mut view.commits).extend(page.commits.into_iter().skip(1));
                        view.history_limit = view.commits.len().max(git::HISTORY_PAGE);
                    }
                    Ok(_) => view.browse(cx),
                    Err(error) => {
                        view.browser_error =
                            Some(format!("{}\n{error:#}", tr("Git query failed")).into())
                    }
                }
                cx.notify();
            });
        }));
    }

    pub(super) fn browse(&mut self, cx: &mut Context<Self>) {
        self.cancel_browse();
        let Some(root) = self.active_root.clone() else {
            return;
        };
        let generation = self.browse_generation;
        let remote = self.remote.clone();
        let limit = self.history_limit;
        let search = self.history_search.clone();
        let path = self.history_path.clone();
        self.browsing = true;
        self.browser_error = None;
        let (cancel, guard) = Cancellation::task();
        self.browse_cancel = Some(guard);
        let query = cx.background_executor().spawn(async move {
            git::history(
                &root,
                remote.as_ref(),
                0,
                limit,
                &search,
                path.as_deref(),
                &cancel,
            )
        });
        self.browse_task = Some(cx.spawn(async move |view, cx| {
            let result = query.await;
            let _ = view.update(cx, |view, cx| {
                if view.browse_generation != generation || view.section != Section::History {
                    return;
                }
                view.browsing = false;
                match result {
                    Ok(history) => {
                        view.history_more = history.more;
                        view.commits = Arc::new(history.commits);
                        if view.selected_commit.as_ref().is_some_and(|selected| {
                            !view.commits.iter().any(|commit| commit.id == selected.id)
                        }) {
                            view.selected_commit = None;
                        }
                    }
                    Err(error) => view.browser_error = Some(format!("{error:#}").into()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn select_history_commit(&mut self, commit: Commit, cx: &mut Context<Self>) {
        let comparison = match &self.history_path {
            Some(path) => Comparison::FileHistory(commit.id.clone(), path.clone()),
            None => Comparison::Commit(commit.id.clone()),
        };
        self.selected_commit = Some(commit);
        self.open_comparison(comparison, cx);
    }

    pub(super) fn history_view(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let toolbar = h_flex().gap_1().child(self.revision_menu(cx)).child(
            super::controls::icon_action(
                Button::new("git-history-search"),
                IconName::Search,
                "Search",
            )
            .selected(self.search_open)
            .on_click(cx.listener(|view, _, _, cx| {
                view.search_open = !view.search_open;
                cx.notify();
            })),
        );
        let mut panel = v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .gap_1()
            .p_2()
            .child(toolbar);
        if let Some(path) = &self.history_path {
            panel = panel.child(
                h_flex()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .child(path.to_string_lossy().into_owned()),
                    )
                    .child(
                        super::controls::icon_action(
                            Button::new("git-history-clear-file"),
                            IconName::X,
                            "All files",
                        )
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.history_path = None;
                            view.reset_selection(cx);
                            view.browse(cx);
                        })),
                    )
                    .child({
                        let path = path.clone();
                        super::controls::icon_action(
                            Button::new("git-history-blame"),
                            IconName::List,
                            "Blame",
                        )
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.open_comparison(Comparison::Blame(path.clone()), cx);
                        }))
                    }),
            );
        }
        if self.search_open {
            panel = panel.child(
                h_flex()
                    .gap_1()
                    .child(
                        div().flex_1().min_w_0().child(
                            crate::views::form_input(&self.search, window, cx)
                                .aria_label(tr("Search commit messages")),
                        ),
                    )
                    .child(
                        super::controls::icon_action(
                            Button::new("git-search-run"),
                            IconName::Search,
                            "Search",
                        )
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.history_search = view.search.read(cx).value().to_string();
                            view.reset_selection(cx);
                            view.browse(cx);
                        })),
                    )
                    .child(
                        super::controls::icon_action(
                            Button::new("git-search-clear"),
                            IconName::X,
                            "Clear",
                        )
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.search
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            view.history_search.clear();
                            view.search_open = false;
                            view.reset_selection(cx);
                            view.browse(cx);
                        })),
                    ),
            );
        }
        if let Some(error) = &self.browser_error {
            let error = error.clone();
            panel = panel.child(
                super::controls::icon_action(
                    Button::new("git-history-error"),
                    IconName::Copy,
                    "Copy error details",
                )
                .label(tr("Git query failed"))
                .tooltip(error.clone())
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(error.to_string()))
                }),
            );
        }
        let owner = cx.entity();
        let commits = self.commits.clone();
        let count = commits.len();
        if count == 0 {
            return panel
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(if self.browsing {
                            "Loading…"
                        } else {
                            "No entries"
                        })),
                )
                .into_any_element();
        }
        let selected = self
            .selected_commit
            .as_ref()
            .map(|commit| commit.id.clone());
        panel
            .child(
                uniform_list("git-history-list", count, move |range, _, cx| {
                    if range.end >= count {
                        let view = owner.read(cx);
                        let generation = view.browse_generation;
                        if view.history_more && !view.browsing && view.browser_error.is_none() {
                            let owner = owner.clone();
                            cx.defer(move |cx| {
                                owner.update(cx, |view, cx| view.append_history(generation, cx))
                            });
                        }
                    }
                    range
                        .map(|index| {
                            let commit = commits[index].clone();
                            let id = commit.id.clone();
                            let active = selected.as_ref() == Some(&id);
                            let description = format!(
                                "{} · {} · {}",
                                commit.author,
                                commit.date,
                                &id[..id.len().min(12)]
                            );
                            let owner = owner.clone();
                            let details_owner = owner.clone();
                            div()
                                .id(("git-history-menu", index))
                                .child(
                                    Button::new(("git-commit-row", index))
                                        .ghost()
                                        .w_full()
                                        .h(px(44.))
                                        .justify_start()
                                        .selected(active)
                                        .tooltip(commit.message.clone())
                                        .child(
                                            v_flex()
                                                .min_w_0()
                                                .items_start()
                                                .child(
                                                    div()
                                                        .truncate()
                                                        .text_sm()
                                                        .child(commit.subject.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .truncate()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(description),
                                                ),
                                        )
                                        .on_click(move |_, _, cx| {
                                            owner.update(cx, |view, cx| {
                                                view.select_history_commit(commit.clone(), cx)
                                            })
                                        }),
                                )
                                .context_menu(move |menu, _, _| {
                                    let copy_id = id.clone();
                                    let details = Comparison::Details(id.clone());
                                    let owner = details_owner.clone();
                                    menu.item(PopupMenuItem::new(tr("Commit details")).on_click(
                                        move |_, _, cx| {
                                            owner.update(cx, |view, cx| {
                                                view.open_comparison(details.clone(), cx);
                                            })
                                        },
                                    ))
                                    .item(
                                        PopupMenuItem::new(tr("Copy commit ID")).on_click(
                                            move |_, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    copy_id.clone(),
                                                ));
                                            },
                                        ),
                                    )
                                })
                        })
                        .collect::<Vec<_>>()
                })
                .track_scroll(&self.history_scroll)
                .flex_1()
                .min_h_0(),
            )
            .into_any_element()
    }
}
