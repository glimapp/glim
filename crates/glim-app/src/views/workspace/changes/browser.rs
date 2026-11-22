use super::controls::Form;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    Changes,
    History,
    Branches,
    Remotes,
    Stashes,
    Worktrees,
}
impl Section {
    pub(super) const PRIMARY: [Self; 4] = [
        Self::Changes,
        Self::History,
        Self::Branches,
        Self::Worktrees,
    ];
    pub(super) fn parent(self) -> Self {
        match self {
            Self::Stashes => Self::Changes,
            Self::Remotes => Self::Branches,
            other => other,
        }
    }
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Changes => "Changes",
            Self::History => "History",
            Self::Branches => "Branches & tags",
            Self::Remotes => "Remotes",
            Self::Stashes => "Stashes",
            Self::Worktrees => "Worktrees",
        }
    }
}

#[derive(Clone)]
pub(super) enum RowAction {
    Run(Operation),
    Open(PathBuf),
    Compare(Comparison),
    Form(Form),
    Copy(String),
}
impl Changes {
    pub(super) fn row_menu(
        &self,
        menu: gpui_kit::component::menu::PopupMenu,
        source: &ElementId,
        actions: &[(IconName, String, RowAction, bool)],
        root: &Option<PathBuf>,
        owner: &Entity<Self>,
        cx: &App,
    ) -> gpui_kit::component::menu::PopupMenu {
        let mut menu = menu;
        for (icon, label, action, disabled) in actions {
            let owner = owner.clone();
            let root = root.clone();
            let action = action.clone();
            let source = source.clone();
            menu = menu.item(
                PopupMenuItem::new(label.clone())
                    .icon(icon.clone())
                    .disabled(
                        *disabled
                            || match &action {
                                RowAction::Run(op) => self.operation_blocked(op, cx),
                                RowAction::Form(_) => self.busy(cx),
                                _ => false,
                            },
                    )
                    .on_click(move |_, window, cx| {
                        owner.update(cx, |view, cx| {
                            if view.active_root != root {
                                return;
                            }
                            match &action {
                                RowAction::Run(op) => {
                                    view.request(source.clone(), op.clone(), window, cx)
                                }
                                RowAction::Form(form) => {
                                    view.form(source.clone(), form.clone(), window, cx)
                                }
                                RowAction::Open(path) => cx.emit(OpenFile(path.clone())),
                                RowAction::Copy(id) => {
                                    cx.write_to_clipboard(ClipboardItem::new_string(id.clone()))
                                }
                                RowAction::Compare(comparison) => {
                                    view.show_comparison(comparison.clone(), cx);
                                    cx.emit(ShowDiff);
                                }
                            }
                        });
                    }),
            );
        }
        menu
    }
    pub(super) fn subsection_button(
        &self,
        section: Section,
        icon: IconName,
        cx: &mut Context<Self>,
    ) -> Button {
        super::controls::icon_action(
            Button::new(("git-subsection", section as usize)),
            icon,
            section.label(),
        )
        .selected(self.section == section)
        .on_click(cx.listener(move |view, _, _, cx| {
            let next = if view.section == section {
                section.parent()
            } else {
                section
            };
            view.switch_section(next, cx);
            cx.emit(ShowDiff);
        }))
    }

    fn clear_comparison(&mut self, cx: &mut Context<Self>) {
        self.comparison = None;
        self.patch_cancel = None;
        self.patch_task = None;
        self.patch_loading = false;
        self.clear_patch(cx);
    }

    pub(super) fn cancel_browse(&mut self) {
        self.browse_generation += 1;
        self.browse_cancel = None;
        self.browse_task = None;
        self.browsing = false;
    }
    pub(super) fn activate_root(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.busy(cx) || self.active_root.as_ref() == Some(&root) {
            return;
        }
        self.cancel_browse();
        self.patch_cancel = None;
        self.patch_task = None;
        self.patch_loading = false;
        self.selection = None;
        self.catalog_ready = false;
        self.catalog = Arc::default();
        self.reference_groups = Default::default();
        self.commits = Arc::default();
        self.active_root = Some(root.clone());
        self.comparison = None;
        self.selected_commit = None;
        self.selected_reference = None;
        self.selected_stash = None;
        self.reset_history();
        self.history_path = None;
        self.clear_patch(cx);
        if self.section == Section::Changes {
            let change = self
                .repositories
                .iter()
                .find(|repo| repo.root == root)
                .and_then(|repo| repo.changes.first().cloned());
            self.selection = None;
            if let Some(change) = change {
                self.select(root, change, cx);
            }
        } else {
            self.browse(cx);
        }
        cx.notify();
    }

    pub(super) fn switch_section(&mut self, section: Section, cx: &mut Context<Self>) {
        if self.section == section {
            return;
        }
        self.cancel_browse();
        self.section = section;
        self.catalog_ready = false;
        if section == Section::History {
            self.reset_history();
            self.history_path = None;
        }
        self.comparison = None;
        self.selected_commit = None;
        self.selected_reference = None;
        self.selected_stash = None;
        self.patch_cancel = None;
        self.patch_task = None;
        self.patch_loading = false;
        self.clear_patch(cx);
        if section == Section::Changes {
            let selection = self
                .selection
                .clone()
                .filter(|(root, _)| Some(root) == self.active_root.as_ref())
                .or_else(|| {
                    self.repositories
                        .iter()
                        .filter(|repo| Some(&repo.root) == self.active_root.as_ref())
                        .find_map(|repo| {
                            repo.changes
                                .first()
                                .map(|change| (repo.root.clone(), change.clone()))
                        })
                });
            self.selection = None;
            if let Some((root, change)) = selection {
                self.select(root, change, cx);
            }
        } else {
            self.browse(cx);
        }
        cx.notify();
    }

    pub(super) fn show_comparison(&mut self, comparison: Comparison, cx: &mut Context<Self>) {
        let Some(root) = self.active_root.clone() else {
            return;
        };
        self.comparison = Some((root.clone(), comparison.clone()));
        self.load_patch(root, None, Some(comparison), cx);
        cx.notify();
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
        self.browse_cancel = None;
        self.browse_task = None;
        self.browse_generation += 1;
        let generation = self.browse_generation;
        let Some(root) = self.active_root.clone() else {
            return;
        };
        let remote = self.remote.clone();
        let section = self.section;
        let limit = self.history_limit;
        let search = self.history_search.clone();
        let path = self.history_path.clone();
        self.browsing = true;
        let (cancel, guard) = Cancellation::task();
        self.browse_cancel = Some(guard);
        let work = cx.background_executor().spawn(async move {
            let catalog = git::catalog(&root, remote.as_ref(), &cancel)?;
            let history = if section == Section::History {
                Some(git::history(
                    &root,
                    remote.as_ref(),
                    0,
                    limit,
                    &search,
                    path.as_deref(),
                    &cancel,
                )?)
            } else {
                None
            };
            let revisions = if matches!(section, Section::Branches | Section::Worktrees)
                && !catalog.references.is_empty()
            {
                Some(
                    git::history(
                        &root,
                        remote.as_ref(),
                        0,
                        git::HISTORY_PAGE,
                        "",
                        None,
                        &cancel,
                    )?
                    .commits,
                )
            } else {
                None
            };
            Ok::<_, anyhow::Error>((catalog, history, revisions))
        });
        self.browse_task = Some(cx.spawn(async move |view, cx| {
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                if view.browse_generation != generation {
                    return;
                }
                view.browsing = false;
                match result {
                    Ok((catalog, history, revisions)) => {
                        view.catalog_ready = true;
                        view.browser_error = None;
                        if let Some(commits) = revisions {
                            view.commits = Arc::new(commits);
                        }
                        let reference_key =
                            view.selected_reference
                                .and_then(|index| match view.section {
                                    Section::Branches => {
                                        view.catalog.references.get(index).map(|r| r.full.clone())
                                    }
                                    Section::Remotes => {
                                        view.catalog.remotes.get(index).map(|r| r.name.clone())
                                    }
                                    Section::Worktrees => view
                                        .catalog
                                        .worktrees
                                        .get(index)
                                        .map(|r| r.path.to_string_lossy().into_owned()),
                                    _ => None,
                                });
                        let stash_key = view
                            .selected_stash
                            .and_then(|index| view.catalog.stashes.get(index))
                            .map(|s| s.id.clone());
                        view.reference_groups = Default::default();
                        for (index, reference) in catalog.references.iter().enumerate() {
                            view.reference_groups
                                [usize::from(reference.full.starts_with("refs/tags/"))]
                            .push(index);
                        }
                        view.catalog = Arc::new(catalog);
                        view.selected_reference =
                            reference_key.as_ref().and_then(|key| match view.section {
                                Section::Branches => {
                                    view.catalog.references.iter().position(|r| &r.full == key)
                                }
                                Section::Remotes => {
                                    view.catalog.remotes.iter().position(|r| &r.name == key)
                                }
                                Section::Worktrees => view
                                    .catalog
                                    .worktrees
                                    .iter()
                                    .position(|r| r.path.to_string_lossy().as_ref() == key),
                                _ => None,
                            });
                        view.selected_stash = stash_key
                            .as_ref()
                            .and_then(|key| view.catalog.stashes.iter().position(|s| &s.id == key));
                        if (reference_key.is_some()
                            && view.selected_reference.is_none()
                            && view.section == Section::Branches)
                            || (stash_key.is_some()
                                && view.selected_stash.is_none()
                                && view.section == Section::Stashes)
                        {
                            view.clear_comparison(cx);
                        }
                        if let Some(history) = history {
                            view.history_more = history.more;
                            view.commits = Arc::new(history.commits);
                            if view.commits.is_empty() {
                                view.selected_commit = None;
                                view.clear_comparison(cx);
                            }
                            if view.selected_commit.as_ref().is_none_or(|commit| {
                                !view.commits.iter().any(|item| item.id == commit.id)
                            }) {
                                view.selected_commit = view.commits.first().cloned();
                                if let Some(commit) = view.selected_commit.clone() {
                                    view.show_comparison(
                                        match view.history_path.clone() {
                                            Some(path) => Comparison::FileHistory(commit.id, path),
                                            None => Comparison::Commit(commit.id),
                                        },
                                        cx,
                                    );
                                }
                            }
                        }
                    }
                    Err(error) => {
                        view.browser_error =
                            Some(format!("{}\n{error:#}", tr("Git query failed")).into());
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn browser_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let controls = match self.section {
            Section::History => h_flex()
                .gap_1()
                .flex_wrap()
                .child(self.form_button("git-compare", Form::Compare, cx))
                .child(
                    super::controls::icon_action(
                        Button::new("git-history-all"),
                        gpui_kit::assets::IconName::Files,
                        "All files",
                    )
                    .selected(self.history_path.is_none())
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.history_path = None;
                        view.reset_history();
                        view.selected_commit = None;
                        view.browse(cx);
                    })),
                )
                .when_some(
                    self.selection.as_ref().map(|(_, c)| c.path.clone()),
                    |bar, path| {
                        bar.child(
                            super::controls::icon_action(
                                Button::new("git-file-history"),
                                gpui_kit::assets::IconName::FileClock,
                                "File history",
                            )
                            .selected(self.history_path.as_ref() == Some(&path))
                            .on_click(cx.listener(
                                move |view, _, _, cx| {
                                    view.history_path = Some(path.clone());
                                    view.reset_history();
                                    view.selected_commit = None;
                                    view.browse(cx);
                                },
                            )),
                        )
                    },
                )
                .child(
                    super::controls::icon_action(
                        Button::new("git-history-search"),
                        gpui_kit::assets::IconName::Search,
                        "Search",
                    )
                    .selected(self.search_open)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.search_open = !view.search_open;
                        cx.notify();
                    })),
                )
                .into_any_element(),
            Section::Branches => h_flex()
                .gap_1()
                .flex_wrap()
                .child(self.form_button("git-new-branch", Form::Branch, cx))
                .child(self.form_button("git-new-tag", Form::Tag, cx))
                .child(self.form_button("git-compare-refs", Form::Compare, cx))
                .child(self.subsection_button(Section::Remotes, IconName::Server, cx))
                .into_any_element(),
            Section::Remotes => h_flex()
                .gap_1()
                .child(self.subsection_button(Section::Remotes, IconName::Server, cx))
                .child(self.form_button("git-add-remote", Form::Remote, cx))
                .child(self.operation_button("git-fetch-all", Operation::Fetch(None), false, cx))
                .into_any_element(),
            Section::Stashes => h_flex()
                .gap_1()
                .child(self.subsection_button(Section::Stashes, IconName::Archive, cx))
                .child(self.form_button("git-stash-save", Form::Stash, cx))
                .into_any_element(),
            Section::Worktrees => self
                .form_button("git-worktree-add", Form::Worktree, cx)
                .into_any_element(),
            Section::Changes => div().into_any_element(),
        };
        let visible_refs = std::array::from_fn::<_, 2, _>(|group| {
            if self.collapsed_refs[group] {
                0
            } else {
                self.reference_groups[group].len()
            }
        });
        let count = match self.section {
            Section::History => self.commits.len(),
            Section::Branches => 2 + visible_refs.iter().sum::<usize>(),
            Section::Remotes => self.catalog.remotes.len(),
            Section::Stashes => self.catalog.stashes.len(),
            Section::Worktrees => self.catalog.worktrees.len(),
            Section::Changes => 0,
        };
        let owner = cx.entity();
        let section = self.section;
        let rows = uniform_list("git-browser-list", count, move |range, _, cx| {
            if section == Section::History && range.end >= count {
                let view = owner.read(cx);
                if view.history_more && !view.browsing && view.browser_error.is_none() {
                    let generation = view.browse_generation;
                    let owner = owner.clone();
                    cx.defer(move |cx| {
                        owner.update(cx, |view, cx| view.append_history(generation, cx))
                    });
                }
            }
            let view = owner.read(cx);
            range
                .map(|index| {
                    let index = if section == Section::Branches {
                        let group = usize::from(index > visible_refs[0]);
                        let offset = if group == 0 {
                            index
                        } else {
                            index - visible_refs[0] - 1
                        };
                        if offset == 0 {
                            let owner = owner.clone();
                            let (icon, label) = if group == 0 {
                                (IconName::GitBranch, "Git branches")
                            } else {
                                (IconName::Tag, "Git tags")
                            };
                            let arrow = if view.collapsed_refs[group] {
                                IconName::ChevronRight
                            } else {
                                IconName::ChevronDown
                            };
                            return Button::new(("git-reference-group", group))
                                .ghost()
                                .small()
                                .w_full()
                                .h(px(30.))
                                .px(px(5.))
                                .tooltip(tr(label))
                                .accessibility_label(tr(label))
                                .child(
                                    h_flex()
                                        .w_full()
                                        .justify_start()
                                        .gap_1()
                                        .child(gpui_kit::component::Icon::new(icon).size(px(14.)))
                                        .child(tr(label))
                                        .child(gpui_kit::component::Icon::new(arrow).size(px(12.))),
                                )
                                .on_click(move |_, _, cx| {
                                    owner.update(cx, |view, cx| {
                                        view.collapsed_refs[group] = !view.collapsed_refs[group];
                                        cx.notify();
                                    })
                                })
                                .into_any_element();
                        }
                        view.reference_groups[group][offset - 1]
                    } else {
                        index
                    };
                    let (title, subtitle, selected) = match section {
                        Section::History => {
                            let commit = &view.commits[index];
                            (
                                commit.subject.clone(),
                                format!(
                                    "{} · {}",
                                    &commit.id[..8.min(commit.id.len())],
                                    commit.date.get(5..16).unwrap_or(&commit.date)
                                ),
                                view.selected_commit
                                    .as_ref()
                                    .is_some_and(|c| c.id == commit.id),
                            )
                        }
                        Section::Branches => {
                            let reference = &view.catalog.references[index];
                            (
                                format!(
                                    "{}{}",
                                    if reference.current { "● " } else { "" },
                                    reference.name
                                ),
                                format!(
                                    "{} · {} {}",
                                    if reference.full.starts_with("refs/tags/") {
                                        tr("Tag")
                                    } else if reference.full.starts_with("refs/remotes/") {
                                        tr("Remote branch")
                                    } else {
                                        tr("Local branch")
                                    },
                                    reference.upstream,
                                    reference.tracking
                                ),
                                view.selected_reference == Some(index),
                            )
                        }
                        Section::Remotes => {
                            let remote = &view.catalog.remotes[index];
                            (
                                remote.name.clone(),
                                remote.url.clone(),
                                view.selected_reference == Some(index),
                            )
                        }
                        Section::Stashes => {
                            let stash = &view.catalog.stashes[index];
                            (
                                stash.subject.clone(),
                                format!("{} · {}", stash.name, stash.id),
                                view.selected_stash == Some(index),
                            )
                        }
                        Section::Worktrees => {
                            let tree = &view.catalog.worktrees[index];
                            (
                                tree.path.display().to_string(),
                                format!(
                                    "{} · {}{}{}",
                                    if tree.branch.is_empty() {
                                        tr("Detached HEAD")
                                    } else {
                                        tree.branch.clone()
                                    },
                                    &tree.head[..8.min(tree.head.len())],
                                    if tree.locked {
                                        format!(" · {}", tr("Locked"))
                                    } else {
                                        String::new()
                                    },
                                    if tree.prunable {
                                        format!(" · {}", tr("Prunable"))
                                    } else {
                                        String::new()
                                    }
                                ),
                                view.selected_reference == Some(index),
                            )
                        }
                        _ => unreachable!(),
                    };
                    let owner = owner.clone();
                    let menu_owner = owner.clone();
                    let target = match section {
                        Section::History => view.commits[index].id.as_str(),
                        Section::Branches => view.catalog.references[index].full.as_str(),
                        Section::Remotes => view.catalog.remotes[index].name.as_str(),
                        Section::Stashes => view.catalog.stashes[index].id.as_str(),
                        Section::Worktrees => &view.catalog.worktrees[index].path.to_string_lossy(),
                        _ => unreachable!(),
                    };
                    let source =
                        ElementId::Name(format!("git-row-{}-{target}", section as usize).into());
                    let row = view
                        .feedback_button(source.clone(), Button::new(("git-browser-row", index)))
                        .ghost()
                        .w_full()
                        .h(if section == Section::Branches {
                            px(30.)
                        } else {
                            px(52.)
                        })
                        .justify_start()
                        .selected(selected)
                        .tooltip(if section == Section::History {
                            let commit = &view.commits[index];
                            format!(
                                "{}\n{} · {}\n{}",
                                title, commit.author, commit.id, commit.date
                            )
                        } else {
                            format!("{title}\n{subtitle}")
                        })
                        .child(
                            v_flex()
                                .w_full()
                                .min_w_0()
                                .gap_1()
                                .child(div().truncate().text_sm().child(title))
                                .when(section == Section::History, |row| {
                                    row.child(
                                        h_flex()
                                            .w_full()
                                            .min_w_0()
                                            .gap_1()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .truncate()
                                                    .child(view.commits[index].author.clone()),
                                            )
                                            .child(
                                                div()
                                                    .flex_shrink_0()
                                                    .child(format!("· {subtitle}")),
                                            ),
                                    )
                                })
                                .when(
                                    !matches!(section, Section::Branches | Section::History),
                                    |row| {
                                        row.child(
                                            div()
                                                .truncate()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(subtitle),
                                        )
                                    },
                                ),
                        )
                        .on_click(move |_, _, cx| {
                            owner.update(cx, |view, cx| {
                                match section {
                                    Section::History => {
                                        let commit = view.commits[index].clone();
                                        view.selected_commit = Some(commit.clone());
                                        view.show_comparison(
                                            match view.history_path.clone() {
                                                Some(path) => {
                                                    Comparison::FileHistory(commit.id, path)
                                                }
                                                None => Comparison::Commit(commit.id),
                                            },
                                            cx,
                                        );
                                    }
                                    Section::Branches => {
                                        view.selected_reference = Some(index);
                                        let reference = view.catalog.references[index].clone();
                                        view.show_comparison(
                                            Comparison::Between("HEAD".into(), reference.full),
                                            cx,
                                        );
                                    }
                                    Section::Stashes => {
                                        view.selected_stash = Some(index);
                                        let stash = view.catalog.stashes[index].clone();
                                        view.show_comparison(Comparison::Stash(stash.id), cx);
                                    }
                                    Section::Remotes | Section::Worktrees => {
                                        view.selected_reference = Some(index);
                                    }
                                    _ => {}
                                }
                                cx.notify();
                            })
                        });
                    let root = view.active_root.clone();
                    let actions = view.item_actions(section, index);
                    div()
                        .id(("git-context-row", index))
                        .child(row)
                        .context_menu(move |menu, _, cx| {
                            menu_owner.read(cx).row_menu(
                                menu,
                                &source,
                                &actions,
                                &root,
                                &menu_owner,
                                cx,
                            )
                        })
                        .into_any_element()
                })
                .collect()
        })
        .when(section == Section::History, |list| {
            list.track_scroll(&self.history_scroll)
        })
        .min_h_0()
        .w_full();
        let mut panel = v_flex()
            .w_full()
            .min_h_0()
            .gap_1()
            .p_2()
            .child(h_flex().child(controls));
        if self.section == Section::History && self.search_open {
            panel = panel.child(
                h_flex()
                    .gap_2()
                    .child(
                        div().flex_1().min_w_0().child(
                            crate::views::form_input(&self.search, window, cx)
                                .aria_label(tr("Search commit messages")),
                        ),
                    )
                    .child(
                        super::controls::icon_action(
                            Button::new("git-search-run"),
                            gpui_kit::assets::IconName::Search,
                            "Search",
                        )
                        .disabled(self.browsing)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.history_search = view.search.read(cx).value().to_string();
                            view.reset_history();
                            view.selected_commit = None;
                            view.browse(cx);
                        })),
                    )
                    .child(
                        super::controls::icon_action(
                            Button::new("git-search-clear"),
                            gpui_kit::assets::IconName::X,
                            "Clear",
                        )
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.search
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            view.search_open = false;
                            view.history_search.clear();
                            view.reset_history();
                            view.selected_commit = None;
                            view.browse(cx);
                        })),
                    ),
            );
        }
        if let Some(path) = &self.history_path {
            if self.section == Section::History {
                panel = panel.child(
                    div()
                        .text_xs()
                        .truncate()
                        .child(path.to_string_lossy().into_owned()),
                );
            }
        }
        if count == 0 {
            panel = panel.child(
                div()
                    .h(px(20.))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if self.browsing && !self.catalog_ready {
                        tr("Loading…")
                    } else if self.section == Section::Stashes {
                        tr("No stashes")
                    } else {
                        tr("No entries")
                    }),
            );
        }
        panel = panel.flex_1().child(rows.flex_1());
        panel.into_any_element()
    }

    fn item_actions(
        &self,
        section: Section,
        index: usize,
    ) -> Vec<(IconName, String, RowAction, bool)> {
        let mut items = Vec::new();
        let mut operations = Vec::new();
        match section {
            Section::History => {
                let commit = &self.commits[index];
                let id = commit.id.clone();
                items.push((
                    IconName::Copy,
                    tr("Copy commit ID"),
                    RowAction::Copy(id.clone()),
                    false,
                ));
                items.push((
                    IconName::FileText,
                    tr("Commit details"),
                    RowAction::Compare(Comparison::Details(id.clone())),
                    false,
                ));
                items.push((
                    IconName::Tag,
                    tr("Create tag"),
                    RowAction::Form(Form::Tag),
                    false,
                ));
                operations.extend([
                    Operation::CherryPick(id.clone()),
                    Operation::Revert(id.clone()),
                    Operation::Merge(id.clone()),
                    Operation::Rebase(id.clone()),
                ]);
                for mode in ["soft", "mixed", "hard"] {
                    operations.push(Operation::Reset {
                        revision: id.clone(),
                        mode: mode.into(),
                    });
                }
                if let Some(path) = self.history_path.clone() {
                    items.push((
                        IconName::List,
                        tr("Blame"),
                        RowAction::Compare(Comparison::Blame(path.clone())),
                        false,
                    ));
                    operations.push(Operation::RestoreFile { revision: id, path });
                }
            }
            Section::Branches => {
                let reference = &self.catalog.references[index];
                let local = reference.full.starts_with("refs/heads/");
                let tag = reference.full.starts_with("refs/tags/");
                operations.extend([
                    Operation::Merge(reference.full.clone()),
                    Operation::Rebase(reference.full.clone()),
                ]);
                if !reference.current {
                    operations.insert(0, Operation::Switch(reference.full.clone()));
                }
                if local && !reference.current {
                    operations.push(Operation::DeleteBranch(reference.name.clone()));
                }
                if tag {
                    operations.push(Operation::DeleteTag(reference.name.clone()));
                    for remote in &self.catalog.remotes {
                        operations.push(Operation::PushTag {
                            remote: remote.name.clone(),
                            tag: reference.name.clone(),
                        });
                        operations.push(Operation::DeleteRemoteTag {
                            remote: remote.name.clone(),
                            tag: reference.name.clone(),
                        });
                    }
                }
                if reference.full.starts_with("refs/remotes/") {
                    if let Some(remote) = self
                        .catalog
                        .remotes
                        .iter()
                        .filter(|remote| reference.name.starts_with(&format!("{}/", remote.name)))
                        .max_by_key(|remote| remote.name.len())
                    {
                        operations.push(Operation::DeleteRemoteBranch {
                            remote: remote.name.clone(),
                            branch: reference.name[remote.name.len() + 1..].into(),
                        });
                    }
                }

                if local {
                    items.push((
                        IconName::Pencil,
                        tr("Rename branch"),
                        RowAction::Form(Form::Rename(reference.name.clone())),
                        false,
                    ));
                }
            }
            Section::Stashes => {
                let stash = &self.catalog.stashes[index];
                operations.extend([
                    Operation::StashApply(stash.id.clone()),
                    Operation::StashPop {
                        name: stash.name.clone(),
                        id: stash.id.clone(),
                    },
                    Operation::StashDrop {
                        name: stash.name.clone(),
                        id: stash.id.clone(),
                    },
                ]);
            }
            Section::Remotes => {
                let remote = &self.catalog.remotes[index];
                operations.extend([
                    Operation::Fetch(Some(remote.name.clone())),
                    Operation::Push(Some(remote.name.clone())),
                    Operation::RemoveRemote(remote.name.clone()),
                ]);
                if let Some(repository) = self
                    .repositories
                    .iter()
                    .find(|repo| Some(&repo.root) == self.active_root.as_ref())
                {
                    if let Some(branch) = repository
                        .metadata
                        .upstream
                        .strip_prefix(&format!("{}/", remote.name))
                    {
                        if let Some(reference) =
                            self.catalog.references.iter().find(|r| {
                                r.full == format!("refs/remotes/{}/{}", remote.name, branch)
                            })
                        {
                            operations.push(Operation::ForcePush {
                                remote: remote.name.clone(),
                                branch: branch.into(),
                                expected: reference.id.clone(),
                            });
                        }
                    }
                }
            }
            Section::Worktrees => {
                let tree = &self.catalog.worktrees[index];
                items.push((
                    IconName::FolderOpen,
                    tr("Open Folder"),
                    RowAction::Open(tree.path.clone()),
                    false,
                ));
                let op = Operation::RemoveWorktree(tree.path.clone());
                items.push((
                    IconName::Trash,
                    op.menu_label(),
                    RowAction::Run(op),
                    tree.locked || self.active_root.as_ref() == Some(&tree.path),
                ));
            }
            _ => {}
        }
        items.extend(operations.into_iter().map(|op| {
            (
                super::controls::operation_icon(&op),
                op.menu_label(),
                RowAction::Run(op),
                false,
            )
        }));
        items
    }
}
