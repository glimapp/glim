use super::controls::Form;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    Changes,
    History,
    Branches,
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
            other => other,
        }
    }
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Changes => "Changes",
            Self::History => "History",
            Self::Branches => "Branches & tags",
            Self::Stashes => "Stashes",
            Self::Worktrees => "Worktrees",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Commit,
    Reference,
    Remote,
    Stash,
    Worktree,
}

#[derive(Clone)]
pub(super) enum RowAction {
    Run(Operation),
    OpenWorktree(PathBuf),
    Compare(Comparison),
    FileHistory(PathBuf),
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
                    .icon(if matches!(action, RowAction::Form(Form::Tag)) {
                        super::controls::create_tag_icon()
                    } else {
                        icon.clone().into()
                    })
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
                                RowAction::FileHistory(path) => {
                                    view.switch_section(Section::History, cx);
                                    view.history_path = Some(path.clone());
                                    view.reset_history();
                                    view.selected_commit = None;
                                    view.browse(cx);
                                    cx.emit(ShowDiff);
                                }
                                RowAction::OpenWorktree(path) => {
                                    cx.emit(OpenWorktree(path.clone()))
                                }
                                RowAction::Copy(id) => {
                                    cx.write_to_clipboard(ClipboardItem::new_string(id.clone()))
                                }
                                RowAction::Compare(comparison) => {
                                    view.open_comparison(comparison.clone(), cx);
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
        self.selected_remote = None;
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
        if section == Section::Worktrees || self.section == Section::Worktrees {
            self.selection = None;
        }
        self.section = section;
        self.catalog_ready = false;
        if section == Section::History {
            self.reset_history();
            self.history_path = None;
        }
        self.comparison = None;
        self.selected_commit = None;
        self.selected_reference = None;
        self.selected_remote = None;
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

    pub(super) fn open_comparison(&mut self, comparison: Comparison, cx: &mut Context<Self>) {
        self.show_comparison(comparison, cx);
        cx.emit(ShowDiff);
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
            let mut catalog = git::catalog(&root, remote.as_ref(), &cancel)?;
            if section == Section::Worktrees {
                for tree in &mut catalog.worktrees {
                    cancel.check()?;
                    if tree.prunable {
                        tree.status_error = Some(tr("Prunable"));
                        continue;
                    }
                    match git::changed_files(&tree.path, remote.as_ref(), &cancel) {
                        Ok(changes) => {
                            tree.changed_paths = changes
                                .iter()
                                .map(|change| &change.path)
                                .collect::<std::collections::HashSet<_>>()
                                .len();
                            tree.changes = changes;
                        }
                        Err(error) => tree.status_error = Some(format!("{error:#}")),
                    }
                }
            }
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
                        view.selected_remote = view.selected_remote.take().filter(|name| {
                            catalog.remotes.iter().any(|remote| &remote.name == name)
                        });
                        view.catalog = Arc::new(catalog);
                        view.selected_reference =
                            reference_key.as_ref().and_then(|key| match view.section {
                                Section::Branches => {
                                    view.catalog.references.iter().position(|r| &r.full == key)
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
                .child(self.form_button("git-compare", Form::Compare, cx))
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
                .child(self.form_button("git-add-remote", Form::Remote, cx))
                .child(self.form_button("git-compare-refs", Form::Compare, cx))
                .child(
                    self.operation_button(
                        "git-fetch-all",
                        Operation::Fetch(None),
                        self.catalog.remotes.is_empty(),
                        cx,
                    )
                    .tooltip(tr("Fetch updates from all remotes"))
                    .accessibility_label(tr("Fetch updates from all remotes")),
                )
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
        let mut visible_refs = [
            self.reference_groups[0].len(),
            self.reference_groups[1].len(),
            self.catalog.remotes.len(),
        ];
        for (count, collapsed) in visible_refs.iter_mut().zip(self.collapsed_refs) {
            if collapsed {
                *count = 0;
            }
        }
        let mut ends = Vec::new();
        if self.section == Section::Worktrees {
            let mut end = 0;
            for tree in &self.catalog.worktrees {
                end += 1;
                if self.expanded_worktrees.contains(&tree.path) {
                    end += tree.changes.len();
                }
                ends.push(end);
            }
        }
        let count = match self.section {
            Section::History => self.commits.len(),
            Section::Branches => visible_refs.len() + visible_refs.iter().sum::<usize>(),
            Section::Stashes => self.catalog.stashes.len(),
            Section::Worktrees => ends.last().copied().unwrap_or(0),
            Section::Changes => 0,
        };
        let owner = cx.entity();
        let section = self.section;
        let render_row = move |index: usize, owner: &Entity<Self>, cx: &App| {
            let view = owner.read(cx);
            let (kind, index) = match section {
                Section::History => (EntryKind::Commit, index),
                Section::Stashes => (EntryKind::Stash, index),
                Section::Worktrees => {
                    let tree_index = ends.partition_point(|&end| end <= index);
                    let offset = index - tree_index.checked_sub(1).map_or(0, |prev| ends[prev]);
                    if offset > 0 {
                        let tree = &view.catalog.worktrees[tree_index];
                        let change = tree.changes[offset - 1].clone();
                        let root = tree.path.clone();
                        let owner = owner.clone();
                        let selected = view
                            .selection
                            .as_ref()
                            .is_some_and(|(r, c)| r == &root && c == &change);
                        let path = change.path.to_string_lossy().into_owned();
                        let detail = format!(
                            "{} · {}",
                            root.join(&change.path).display(),
                            tr(Self::area(change.area))
                        );
                        return Button::new(("git-worktree-file", index))
                            .ghost()
                            .w_full()
                            .h(px(52.))
                            .pl_4()
                            .justify_start()
                            .selected(selected)
                            .accessibility_label(detail.clone())
                            .tooltip(detail)
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .child(div().truncate().child(path))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!(
                                                "{} · {}",
                                                change.status,
                                                tr(Self::area(change.area))
                                            )),
                                    ),
                            )
                            .on_click(move |_, _, cx| {
                                owner.update(cx, |view, cx| {
                                    view.preview_worktree_file(root.clone(), change.clone(), cx);
                                    cx.emit(ShowDiff);
                                })
                            })
                            .into_any_element();
                    }
                    (EntryKind::Worktree, tree_index)
                }
                Section::Changes => unreachable!(),
                Section::Branches => {
                    let mut group = 0;
                    let mut offset = index;
                    while offset > visible_refs[group] {
                        offset -= visible_refs[group] + 1;
                        group += 1;
                    }
                    if offset == 0 {
                        let owner = owner.clone();
                        let (icon, label) = [
                            (IconName::GitBranch, "Git branches"),
                            (IconName::Tag, "Git tags"),
                            (IconName::Server, "Remotes"),
                        ][group];
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
                    if group == 2 {
                        (EntryKind::Remote, offset - 1)
                    } else {
                        (
                            EntryKind::Reference,
                            view.reference_groups[group][offset - 1],
                        )
                    }
                }
            };
            let tag = (kind == EntryKind::Reference)
                .then(|| view.catalog.references[index].tag.as_deref())
                .flatten();
            let (title, subtitle, selected) = match kind {
                EntryKind::Commit => {
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
                EntryKind::Reference => {
                    let reference = &view.catalog.references[index];
                    (
                        format!(
                            "{}{}",
                            if reference.current { "● " } else { "" },
                            reference.name
                        ),
                        if let Some(tag) = tag {
                            tag.date.get(5..16).unwrap_or(&tag.date).to_owned()
                        } else {
                            format!(
                                "{} · {} {}",
                                tr(if reference.full.starts_with("refs/remotes/") {
                                    "Remote branch"
                                } else {
                                    "Local branch"
                                }),
                                reference.upstream,
                                reference.tracking
                            )
                        },
                        view.selected_remote.is_none() && view.selected_reference == Some(index),
                    )
                }
                EntryKind::Remote => {
                    let remote = &view.catalog.remotes[index];
                    (
                        remote.name.clone(),
                        remote.url.clone(),
                        view.selected_remote.as_ref() == Some(&remote.name),
                    )
                }
                EntryKind::Stash => {
                    let stash = &view.catalog.stashes[index];
                    (
                        stash.subject.clone(),
                        format!("{} · {}", stash.name, stash.id),
                        view.selected_stash == Some(index),
                    )
                }
                EntryKind::Worktree => {
                    let tree = &view.catalog.worktrees[index];
                    (
                        if tree.branch.is_empty() {
                            format!(
                                "{} {}",
                                tr("Detached HEAD"),
                                &tree.head[..8.min(tree.head.len())]
                            )
                        } else {
                            tree.branch.clone()
                        },
                        tree.path.display().to_string(),
                        view.expanded_worktrees.contains(&tree.path),
                    )
                }
            };
            let owner = owner.clone();
            let menu_owner = owner.clone();
            let target = match kind {
                EntryKind::Commit => view.commits[index].id.as_str(),
                EntryKind::Reference => view.catalog.references[index].full.as_str(),
                EntryKind::Remote => view.catalog.remotes[index].name.as_str(),
                EntryKind::Stash => view.catalog.stashes[index].id.as_str(),
                EntryKind::Worktree => &view.catalog.worktrees[index].path.to_string_lossy(),
            };
            let source = ElementId::Name(format!("git-row-{}-{target}", kind as usize).into());
            let author = if kind == EntryKind::Commit {
                Some(view.commits[index].author.as_str())
            } else {
                tag.map(|tag| tag.author.as_str())
            };
            let count = (kind == EntryKind::Worktree).then(|| {
                let tree = &view.catalog.worktrees[index];
                if tree.status_error.is_some() {
                    tr("Status unavailable")
                } else if !view.catalog_ready {
                    "…".to_owned()
                } else {
                    tree.changed_paths.to_string()
                }
            });
            let description = if let Some(tag) = tag {
                [title.as_str(), &tag.subject, &tag.author, &tag.date]
                    .into_iter()
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else if kind == EntryKind::Commit {
                let commit = &view.commits[index];
                format!(
                    "{}\n{} · {}\n{}",
                    commit.message.trim_end(),
                    commit.author,
                    commit.id,
                    commit.date
                )
            } else {
                format!("{title}\n{subtitle}")
            };
            let description = if kind == EntryKind::Worktree {
                let tree = &view.catalog.worktrees[index];
                let details = [
                    tree.locked.then(|| tr("Locked")),
                    tree.prunable.then(|| tr("Prunable")),
                    tree.status_error.clone(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
                std::iter::once(description)
                    .chain(count.clone())
                    .chain(details)
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                description
            };
            let row_id = if kind == EntryKind::Remote {
                "git-remote-row"
            } else {
                "git-browser-row"
            };
            let row = view
                .feedback_button(source.clone(), Button::new((row_id, index)))
                .ghost()
                .w_full()
                .h(if kind == EntryKind::Reference && tag.is_none() {
                    px(30.)
                } else {
                    px(52.)
                })
                .justify_start()
                .selected(selected)
                .when(kind == EntryKind::Worktree, |row| {
                    row.icon(if selected {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                })
                .accessibility_label(description.clone())
                .tooltip(description)
                .child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .gap_1()
                        .child(
                            h_flex()
                                .min_w_0()
                                .gap_1()
                                .child(div().truncate().text_sm().child(
                                    if let Some(tag) = tag.filter(|t| !t.subject.is_empty()) {
                                        format!("{title}  {}", tag.subject)
                                    } else {
                                        title
                                    },
                                ))
                                .when_some(count, |line, count| {
                                    line.child(
                                        div()
                                            .flex_shrink_0()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(count),
                                    )
                                }),
                        )
                        .when_some(author, |row, author| {
                            row.child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap_1()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(div().min_w_0().truncate().child(author.to_owned()))
                                    .when(!subtitle.is_empty(), |line| {
                                        line.child(div().flex_shrink_0().child(
                                            if author.is_empty() {
                                                subtitle.clone()
                                            } else {
                                                format!("· {subtitle}")
                                            },
                                        ))
                                    }),
                            )
                        })
                        .when(
                            !matches!(kind, EntryKind::Reference | EntryKind::Commit),
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
                .on_click(move |event, _, cx| {
                    owner.update(cx, |view, cx| {
                        match kind {
                            EntryKind::Commit => {
                                let commit = view.commits[index].clone();
                                view.selected_commit = Some(commit.clone());
                                view.open_comparison(
                                    match view.history_path.clone() {
                                        Some(path) => Comparison::FileHistory(commit.id, path),
                                        None => Comparison::Commit(commit.id),
                                    },
                                    cx,
                                );
                            }
                            EntryKind::Reference => {
                                view.selected_remote = None;
                                view.selected_reference = Some(index);
                                let reference = view.catalog.references[index].clone();
                                view.open_comparison(Comparison::for_reference(&reference), cx);
                            }
                            EntryKind::Stash => {
                                view.selected_stash = Some(index);
                                let stash = view.catalog.stashes[index].clone();
                                view.open_comparison(Comparison::Stash(stash.id), cx);
                            }
                            EntryKind::Remote => {
                                view.selected_remote =
                                    Some(view.catalog.remotes[index].name.clone());
                            }
                            EntryKind::Worktree => {
                                let path = view.catalog.worktrees[index].path.clone();
                                if event.click_count() == 2 {
                                    if !view.catalog.worktrees[index].prunable {
                                        cx.emit(OpenWorktree(path));
                                    }
                                } else if !view.expanded_worktrees.remove(&path) {
                                    view.expanded_worktrees.insert(path);
                                }
                            }
                        }
                        cx.notify();
                    })
                });
            let root = view.active_root.clone();
            let actions = view.item_actions(kind, index, cx);
            div()
                .id((
                    if kind == EntryKind::Remote {
                        "git-remote-context"
                    } else {
                        "git-context-row"
                    },
                    index,
                ))
                .child(row)
                .context_menu(move |menu, _, cx| {
                    menu_owner
                        .read(cx)
                        .row_menu(menu, &source, &actions, &root, &menu_owner, cx)
                })
                .into_any_element()
        };
        let rows = if section == Section::Branches {
            // Group headers and branch names are compact. Tag and remote details
            // need taller rows, so measure visible items individually.
            let mut start = 1;
            for (previous, &visible) in self.visible_group_sizes.iter_mut().zip(&visible_refs) {
                if *previous != visible {
                    let common = (*previous).min(visible);
                    self.reference_list
                        .splice(start + common..start + *previous, visible - common);
                    *previous = visible;
                }
                start += visible + 1;
            }
            list(self.reference_list.clone(), move |index, _, cx| {
                render_row(index, &owner, cx)
            })
            .w_full()
            .flex_1()
            .min_h_0()
            .into_any_element()
        } else {
            uniform_list("git-browser-list", count, move |range, _, cx| {
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
                range.map(|index| render_row(index, &owner, cx)).collect()
            })
            .when(section == Section::History, |list| {
                list.track_scroll(&self.history_scroll)
            })
            .w_full()
            .flex_1()
            .min_h_0()
            .into_any_element()
        };
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
        panel = panel.flex_1().child(rows);
        panel.into_any_element()
    }

    fn create_worktree_action(
        &self,
        checkout: git::WorktreeCheckout,
        name: &str,
    ) -> (IconName, String, RowAction, bool) {
        let path = self
            .active_root
            .as_ref()
            .map(|root| super::controls::suggested_worktree_path(root, name))
            .unwrap_or_default();
        (
            IconName::FolderPlus,
            tr("Create worktree"),
            RowAction::Form(Form::CheckoutWorktree(checkout, path)),
            self.active_root.is_none(),
        )
    }

    fn item_actions(
        &self,
        kind: EntryKind,
        index: usize,
        cx: &App,
    ) -> Vec<(IconName, String, RowAction, bool)> {
        let mut items = Vec::new();
        let mut operations = Vec::new();
        match kind {
            EntryKind::Commit => {
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
                if Self::preferences(cx).worktrees {
                    items.push(self.create_worktree_action(
                        git::WorktreeCheckout::Detached(id.clone()),
                        &id[..8.min(id.len())],
                    ));
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
            EntryKind::Reference => {
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

                if Self::preferences(cx).worktrees {
                    if local {
                        let branch = reference.full.trim_start_matches("refs/heads/");
                        if let Some(tree) = self
                            .catalog
                            .worktrees
                            .iter()
                            .find(|tree| tree.branch == branch)
                        {
                            items.insert(
                                0,
                                (
                                    IconName::FolderOpen,
                                    tr("Open worktree"),
                                    RowAction::OpenWorktree(tree.path.clone()),
                                    tree.prunable,
                                ),
                            );
                        } else {
                            items.insert(
                                0,
                                self.create_worktree_action(
                                    git::WorktreeCheckout::Branch(branch.to_owned()),
                                    branch,
                                ),
                            );
                        }
                    } else if tag {
                        items.push(self.create_worktree_action(
                            git::WorktreeCheckout::Detached(reference.full.clone()),
                            &reference.name,
                        ));
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
            EntryKind::Stash => {
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
            EntryKind::Remote => {
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
            EntryKind::Worktree => {
                let tree = &self.catalog.worktrees[index];
                items.push((
                    IconName::FolderOpen,
                    tr("Open worktree"),
                    RowAction::OpenWorktree(tree.path.clone()),
                    tree.prunable,
                ));
                let op = Operation::RemoveWorktree(tree.path.clone());
                items.push((
                    IconName::Trash,
                    op.menu_label(),
                    RowAction::Run(op),
                    tree.locked || self.active_root.as_ref() == Some(&tree.path),
                ));
            }
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
