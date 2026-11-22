use super::controls::Form;
use super::*;

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
    pub(super) const ALL: [Self; 6] = [
        Self::Changes,
        Self::History,
        Self::Branches,
        Self::Remotes,
        Self::Stashes,
        Self::Worktrees,
    ];
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

impl Changes {
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
        self.catalog = Arc::default();
        self.commits = Arc::default();
        self.active_root = Some(root.clone());
        self.comparison = None;
        self.selected_commit = None;
        self.selected_reference = None;
        self.selected_stash = None;
        self.history_page = 0;
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
        let page = self.history_page;
        let reflog = self.history_reflog;
        let search = self.search.read(cx).value().to_string();
        let path = self.history_path.clone();
        self.browsing = true;
        self.browser_error = None;
        let (cancel, guard) = Cancellation::task();
        self.browse_cancel = Some(guard);
        let work = cx.background_executor().spawn(async move {
            let catalog = git::catalog(&root, remote.as_ref(), &cancel)?;
            let history = if section == Section::History {
                Some(git::history(
                    &root,
                    remote.as_ref(),
                    page,
                    reflog,
                    &search,
                    path.as_deref(),
                    &cancel,
                )?)
            } else {
                None
            };
            Ok::<_, anyhow::Error>((catalog, history))
        });
        self.browse_task = Some(cx.spawn(async move |view, cx| {
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                if view.browse_generation != generation {
                    return;
                }
                view.browsing = false;
                match result {
                    Ok((catalog, history)) => {
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
                .child(
                    Button::new("git-history-reflog")
                        .small()
                        .label(tr("Reflog"))
                        .selected(self.history_reflog)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.history_reflog = !view.history_reflog;
                            view.history_path = None;
                            view.history_page = 0;
                            view.selected_commit = None;
                            view.browse(cx);
                        })),
                )
                .child(
                    Button::new("git-history-search")
                        .small()
                        .label(tr("Search"))
                        .selected(self.search_open)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.search_open = !view.search_open;
                            cx.notify();
                        })),
                )
                .child(self.form_button("git-compare", Form::Compare, cx))
                .child(
                    Button::new("git-history-all")
                        .small()
                        .label(tr("All files"))
                        .selected(self.history_path.is_none())
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.history_path = None;
                            view.history_page = 0;
                            view.selected_commit = None;
                            view.browse(cx);
                        })),
                )
                .when_some(
                    self.selection.as_ref().map(|(_, c)| c.path.clone()),
                    |bar, path| {
                        bar.child(
                            Button::new("git-file-history")
                                .small()
                                .label(tr("File history"))
                                .selected(self.history_path.as_ref() == Some(&path))
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.history_reflog = false;
                                    view.history_path = Some(path.clone());
                                    view.history_page = 0;
                                    view.selected_commit = None;
                                    view.browse(cx);
                                })),
                        )
                    },
                )
                .child(
                    Button::new("git-log-prev")
                        .small()
                        .label(tr("Previous"))
                        .disabled(self.history_page == 0 || self.browsing)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.history_page = view.history_page.saturating_sub(1);
                            view.selected_commit = None;
                            view.browse(cx);
                        })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child((self.history_page + 1).to_string()),
                )
                .child(
                    Button::new("git-log-next")
                        .small()
                        .label(tr("Next"))
                        .disabled(!self.history_more || self.browsing)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.history_page += 1;
                            view.selected_commit = None;
                            view.browse(cx);
                        })),
                )
                .into_any_element(),
            Section::Branches => h_flex()
                .gap_1()
                .flex_wrap()
                .child(self.form_button("git-new-branch", Form::Branch, cx))
                .child(self.form_button("git-new-tag", Form::Tag("HEAD".into()), cx))
                .child(self.form_button("git-compare-refs", Form::Compare, cx))
                .into_any_element(),
            Section::Remotes => h_flex()
                .gap_1()
                .child(self.form_button("git-add-remote", Form::Remote, cx))
                .child(self.operation_button("git-fetch-all", Operation::Fetch(None), false, cx))
                .into_any_element(),
            Section::Stashes => h_flex()
                .gap_1()
                .child(self.form_button("git-stash-save", Form::Stash, cx))
                .into_any_element(),
            Section::Worktrees => h_flex()
                .gap_1()
                .child(self.form_button("git-worktree-add", Form::Worktree, cx))
                .into_any_element(),
            Section::Changes => div().into_any_element(),
        };
        let count = match self.section {
            Section::History => self.commits.len(),
            Section::Branches => self.catalog.references.len(),
            Section::Remotes => self.catalog.remotes.len(),
            Section::Stashes => self.catalog.stashes.len(),
            Section::Worktrees => self.catalog.worktrees.len(),
            Section::Changes => 0,
        };
        let owner = cx.entity();
        let section = self.section;
        let rows = uniform_list("git-browser-list", count, move |range, _, cx| {
            let view = owner.read(cx);
            range
                .map(|index| {
                    let (title, subtitle, selected) = match section {
                        Section::History => {
                            let commit = &view.commits[index];
                            (
                                format!(
                                    "{}  {}",
                                    &commit.id[..8.min(commit.id.len())],
                                    commit.subject
                                ),
                                format!("{} · {}  {}", commit.author, commit.date, commit.refs),
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
                    Button::new(("git-browser-row", index))
                        .ghost()
                        .w_full()
                        .h(px(52.))
                        .justify_start()
                        .selected(selected)
                        .tooltip(format!("{title}\n{subtitle}"))
                        .child(
                            v_flex()
                                .w_full()
                                .min_w_0()
                                .gap_1()
                                .child(div().truncate().text_sm().child(title))
                                .child(
                                    div()
                                        .truncate()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(subtitle),
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
                        })
                        .into_any_element()
                })
                .collect()
        })
        .min_h_0()
        .w_full();
        let mut panel = v_flex().w_full().min_h_0().gap_2().p_2().child(controls);
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
                        Button::new("git-search-run")
                            .small()
                            .label(tr("Search"))
                            .disabled(self.browsing)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.history_page = 0;
                                view.selected_commit = None;
                                view.browse(cx);
                            })),
                    )
                    .child(
                        Button::new("git-search-clear")
                            .small()
                            .label(tr("Clear"))
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.search
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                                view.search_open = false;
                                view.history_page = 0;
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
        panel = panel.child(
            div()
                .h(px(20.))
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(if self.browsing {
                    tr("Loading…")
                } else if count == 0 {
                    tr("No entries")
                } else {
                    format!("{count} {}", tr("entries"))
                }),
        );
        panel = panel.flex_1().child(rows.flex_1());
        panel = panel.child(self.browser_actions(cx));
        panel.into_any_element()
    }

    fn browser_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut bar = h_flex().gap_1().flex_wrap();
        match self.section {
            Section::History => {
                if let Some(commit) = &self.selected_commit {
                    let id = commit.id.clone();
                    let copy = id.clone();
                    let details_id = id.clone();
                    bar = bar
                        .child(
                            Button::new("git-copy-commit")
                                .small()
                                .label(tr("Copy commit ID"))
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                                }),
                        )
                        .child(
                            Button::new("git-commit-details")
                                .small()
                                .label(tr("Commit details"))
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.show_comparison(
                                        Comparison::Details(details_id.clone()),
                                        cx,
                                    )
                                })),
                        )
                        .child(self.operation_menu(
                            "git-commit-actions",
                            "Commit actions",
                            vec![
                                Operation::CherryPick(id.clone()),
                                Operation::Revert(id.clone()),
                                Operation::Merge(id.clone()),
                                Operation::Rebase(id.clone()),
                            ],
                            cx,
                        ))
                        .child(
                            self.operation_menu(
                                "git-reset",
                                "Reset to commit",
                                ["soft", "mixed", "hard"]
                                    .map(|mode| Operation::Reset {
                                        revision: id.clone(),
                                        mode: mode.into(),
                                    })
                                    .into(),
                                cx,
                            ),
                        )
                        .child(self.form_button("git-tag-commit", Form::Tag(id.clone()), cx));
                    if let Some(path) = self.history_path.clone() {
                        let blame_path = path.clone();
                        bar = bar.child(
                            Button::new("git-file-blame")
                                .small()
                                .label(tr("Blame"))
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.show_comparison(Comparison::Blame(blame_path.clone()), cx)
                                })),
                        );
                        bar = bar.child(self.operation_button(
                            "git-restore-file",
                            Operation::RestoreFile { revision: id, path },
                            false,
                            cx,
                        ));
                    }
                    bar = bar.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(format!("{}: {}", tr("Parents"), commit.parents)),
                    );
                }
            }
            Section::Branches => {
                if let Some(reference) = self
                    .selected_reference
                    .and_then(|i| self.catalog.references.get(i))
                {
                    let local = reference.full.starts_with("refs/heads/");
                    let tag = reference.full.starts_with("refs/tags/");
                    let mut operations = vec![
                        Operation::Merge(reference.full.clone()),
                        Operation::Rebase(reference.full.clone()),
                    ];
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
                            .filter(|remote| {
                                reference.name.starts_with(&format!("{}/", remote.name))
                            })
                            .max_by_key(|remote| remote.name.len())
                        {
                            operations.push(Operation::DeleteRemoteBranch {
                                remote: remote.name.clone(),
                                branch: reference.name[remote.name.len() + 1..].into(),
                            });
                        }
                    }
                    bar = bar.child(self.operation_menu(
                        "git-reference-actions",
                        "Branch actions",
                        operations,
                        cx,
                    ));
                    if local {
                        bar = bar.child(self.form_button(
                            "git-rename-branch",
                            Form::Rename(reference.name.clone()),
                            cx,
                        ));
                    }
                }
            }
            Section::Stashes => {
                if let Some(stash) = self
                    .selected_stash
                    .and_then(|i| self.catalog.stashes.get(i))
                {
                    bar = bar.child(self.operation_menu(
                        "git-stash-actions",
                        "Stash actions",
                        vec![
                            Operation::StashApply(stash.id.clone()),
                            Operation::StashPop {
                                name: stash.name.clone(),
                                id: stash.id.clone(),
                            },
                            Operation::StashDrop {
                                name: stash.name.clone(),
                                id: stash.id.clone(),
                            },
                        ],
                        cx,
                    ));
                }
            }
            Section::Remotes => {
                if let Some(remote) = self
                    .selected_reference
                    .and_then(|i| self.catalog.remotes.get(i))
                {
                    let mut operations = vec![
                        Operation::Fetch(Some(remote.name.clone())),
                        Operation::Push(Some(remote.name.clone())),
                        Operation::RemoveRemote(remote.name.clone()),
                    ];
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
                            if let Some(reference) = self.catalog.references.iter().find(|r| {
                                r.full == format!("refs/remotes/{}/{}", remote.name, branch)
                            }) {
                                operations.push(Operation::ForcePush {
                                    remote: remote.name.clone(),
                                    branch: branch.into(),
                                    expected: reference.id.clone(),
                                });
                            }
                        }
                    }
                    bar = bar.child(self.operation_menu(
                        "git-remote-actions",
                        "Remote actions",
                        operations,
                        cx,
                    ));
                }
            }
            Section::Worktrees => {
                if let Some(tree) = self
                    .selected_reference
                    .and_then(|i| self.catalog.worktrees.get(i))
                {
                    let path = tree.path.clone();
                    bar = bar
                        .child(
                            Button::new("git-open-worktree")
                                .small()
                                .label(tr("Open Folder"))
                                .on_click(
                                    cx.listener(move |_, _, _, cx| cx.emit(OpenFile(path.clone()))),
                                ),
                        )
                        .child(self.operation_button(
                            "git-remove-worktree",
                            Operation::RemoveWorktree(tree.path.clone()),
                            self.active_root.as_ref() == Some(&tree.path) || tree.locked,
                            cx,
                        ));
                }
            }
            _ => {}
        }
        bar.into_any_element()
    }
}
