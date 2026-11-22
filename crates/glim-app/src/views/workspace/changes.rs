mod browser;
mod controls;
mod panel;
use browser::Section;
use glim_services::{
    git::{self, Area, Catalog, Change, Commit, Comparison, Operation, Repository},
    locale::tr,
    work::{CancelOnDrop, Cancellation},
};
use gpui_kit::{
    component::{
        ActiveTheme, Disableable, Selectable, Sizable,
        button::{Button, ButtonVariants},
        diff::{Diff, DiffFile, DiffMode, DiffState},
        h_flex,
        input::{InputState, TextareaState},
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};
use std::{path::PathBuf, sync::Arc};

type NavigationRow = (PathBuf, Option<Change>);

pub(super) struct OpenFile(pub PathBuf);
impl EventEmitter<OpenFile> for Changes {}
pub(super) struct ShowDiff;
impl EventEmitter<ShowDiff> for Changes {}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum DiffTab {
    File(PathBuf, Change),
    Comparison(PathBuf, Comparison),
}

impl DiffTab {
    pub(super) fn label(&self) -> String {
        match self {
            Self::File(_, change) => format!(
                "{} · {}",
                change
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
                tr(Changes::area(change.area))
            ),
            Self::Comparison(_, comparison) => comparison.title(),
        }
    }

    pub(super) fn description(&self) -> String {
        match self {
            Self::File(root, change) => root.join(&change.path).display().to_string(),
            Self::Comparison(root, comparison) => {
                format!("{} · {}", root.display(), comparison.title())
            }
        }
    }
}

enum Prepared {
    Diff(
        Vec<DiffFile>,
        glim_services::resources::WorkingReservation,
        Arc<String>,
    ),
    Raw(glim_services::paged::TextPage),
}

pub(crate) struct Changes {
    pub(super) tabs: Vec<DiffTab>,
    section: Section,
    settings_open: bool,
    identity: [Entity<InputState>; 2],
    identity_root: Option<PathBuf>,
    identity_task: Option<Task<()>>,
    identity_ready: bool,
    monitoring: bool,
    refresh_timer: Option<Task<()>>,
    available: bool,
    active_root: Option<PathBuf>,
    catalog: Arc<Catalog>,
    commits: Arc<Vec<Commit>>,
    history_page: usize,
    history_reflog: bool,
    history_more: bool,
    history_path: Option<PathBuf>,
    search_open: bool,
    search: Entity<InputState>,
    message: Entity<TextareaState>,
    amend: bool,
    message_root: Option<PathBuf>,
    drafts: std::collections::HashMap<PathBuf, (String, bool)>,
    form_values: std::collections::HashMap<String, Vec<String>>,
    comparison: Option<(PathBuf, Comparison)>,
    selected_commit: Option<Commit>,
    collapsed_refs: [bool; 2],
    reference_groups: [Vec<usize>; 2],
    selected_reference: Option<usize>,
    selected_stash: Option<usize>,
    patch_source: Option<Arc<String>>,
    browsing: bool,
    browse_generation: u64,
    browse_task: Option<Task<()>>,
    browse_cancel: Option<CancelOnDrop>,
    operation_busy: bool,
    operation_error: Option<SharedString>,
    browser_error: Option<SharedString>,
    operation_label: Option<SharedString>,
    operation_task: Option<Task<()>>,
    clear_message: Option<String>,
    notice_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
    roots: Vec<PathBuf>,
    remote: Option<glim_services::remote::Session>,
    repositories: Vec<Repository>,
    rows: Arc<Vec<NavigationRow>>,
    scroll: UniformListScrollHandle,
    errors: Vec<SharedString>,
    loading: bool,
    selection: Option<(PathBuf, Change)>,
    diff: Entity<DiffState>,
    memory: Option<glim_services::resources::WorkingReservation>,
    raw: Option<Entity<crate::views::reader::paged::PagedReader>>,
    patch_loading: bool,
    patch_error: Option<SharedString>,
    soft_wrap: bool,
    generation: u64,
    status_task: Option<Task<()>>,
    status_cancel: Option<CancelOnDrop>,
    patch_task: Option<Task<()>>,
    patch_cancel: Option<CancelOnDrop>,
}

impl Changes {
    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        self.settings_open = false;
        self.identity_root = None;
        self.identity_task = None;
        self.monitoring = false;
        self.refresh_timer = None;
        self.generation += 1;
        self.browse_generation += 1;
        self.browse_cancel = None;
        self.browse_task = None;
        self.browsing = false;
        self.status_cancel = None;
        self.patch_cancel = None;
        self.status_task = None;
        self.patch_task = None;
        self.loading = false;
        self.patch_loading = false;
        self.clear_patch(cx);
    }

    pub(super) fn disable(&mut self, cx: &mut Context<Self>) {
        self.cancel(cx);
        self.tabs.clear();
        self.repositories.clear();
        self.catalog = Arc::default();
        self.reference_groups = Default::default();
        self.commits = Arc::default();
        self.rows = Arc::default();
        self.selection = None;
        self.comparison = None;
        self.errors.clear();
        cx.notify();
    }

    pub(super) fn unavailable(&mut self, cx: &mut Context<Self>) {
        self.cancel(cx);
        self.available = false;
        self.repositories.clear();
        self.rows = Arc::default();
        self.selection = None;
        self.clear_patch(cx);
        self.errors = vec![tr("Connect to the remote workspace to view changes").into()];
        cx.notify();
    }
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let message = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(3)
                .placeholder(tr("Commit message"))
        });
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr("Search commit messages")));
        let diff = cx.new(|cx| DiffState::new([], cx));
        let subscriptions = vec![
            cx.observe(&message, |_, _, cx| cx.notify()),
            cx.observe_global::<crate::app::GitSettings>(|view, cx| {
                view.update_refresh_timer(cx);
                if view.section == Section::Worktrees && !Self::preferences(cx).worktrees {
                    view.switch_section(Section::Changes, cx);
                }
                cx.notify();
            }),
            cx.observe(&diff, |_, _, cx| cx.notify()),
        ];
        Self {
            tabs: Vec::new(),
            section: Section::Changes,
            settings_open: false,
            identity: std::array::from_fn(|_| cx.new(|cx| InputState::new(window, cx))),
            identity_root: None,
            identity_task: None,
            identity_ready: false,
            monitoring: false,
            refresh_timer: None,
            available: true,
            active_root: None,
            catalog: Arc::default(),
            commits: Arc::default(),
            history_page: 0,
            history_reflog: false,
            history_more: false,
            history_path: None,
            search_open: false,
            search,
            message,
            amend: false,
            message_root: None,
            drafts: Default::default(),
            form_values: Default::default(),
            comparison: None,
            selected_commit: None,
            collapsed_refs: [false; 2],
            reference_groups: Default::default(),
            selected_reference: None,
            selected_stash: None,
            patch_source: None,
            browsing: false,
            browse_generation: 0,
            browse_task: None,
            browse_cancel: None,
            operation_busy: false,
            operation_error: None,
            browser_error: None,
            operation_label: None,
            operation_task: None,
            clear_message: None,
            notice_task: None,
            _subscriptions: subscriptions,
            roots: Vec::new(),
            remote: None,
            repositories: Vec::new(),
            rows: Arc::default(),
            scroll: UniformListScrollHandle::new(),
            errors: Vec::new(),
            loading: false,
            selection: None,
            diff,
            memory: None,
            raw: None,
            patch_loading: false,
            patch_error: None,
            soft_wrap: false,
            generation: 0,
            status_task: None,
            status_cancel: None,
            patch_task: None,
            patch_cancel: None,
        }
    }

    pub(super) fn load(
        &mut self,
        roots: Vec<PathBuf>,
        remote: Option<glim_services::remote::Session>,
        cx: &mut Context<Self>,
    ) {
        if !git::enabled() {
            self.disable(cx);
            return;
        }
        self.monitoring = !roots.is_empty();
        self.update_refresh_timer(cx);
        self.available = true;
        self.status_cancel = None;
        self.status_task = None;
        self.patch_cancel = None;
        self.patch_task = None;
        self.patch_loading = false;
        self.generation += 1;
        let generation = self.generation;
        if self.roots != roots
            || self.remote.as_ref().map(|r| &r.connection.id)
                != remote.as_ref().map(|r| &r.connection.id)
        {
            self.tabs.clear();
            self.selection = None;
            self.active_root = None;
            self.comparison = None;
            self.catalog = Arc::default();
            self.reference_groups = Default::default();
            self.commits = Arc::default();
            self.selected_commit = None;
            self.selected_reference = None;
            self.selected_stash = None;
            self.history_page = 0;
            self.history_path = None;
            self.cancel_browse();
            self.repositories.clear();
            self.rows = Arc::default();
            self.clear_patch(cx);
        }
        self.roots = roots.clone();
        self.remote = remote.clone();
        self.loading = true;
        self.errors.clear();
        let (cancel, guard) = Cancellation::task();
        self.status_cancel = Some(guard);
        let work = cx.background_executor().spawn(async move {
            roots
                .into_iter()
                .map(|root| {
                    git::changes(&root, remote.as_ref(), &cancel)
                        .map_err(|error| format!("{}: {error:#}", root.display()))
                })
                .collect::<Vec<_>>()
        });
        self.status_task = Some(cx.spawn(async move |view, cx| {
            let results = work.await;
            let _ = view.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                view.loading = false;
                view.repositories.clear();
                for result in results {
                    match result {
                        Ok(repository) => view.repositories.push(repository),
                        Err(error) => view.errors.push(error.into()),
                    }
                }
                let mut rows = Vec::new();
                for repository in &view.repositories {
                    if view.repositories.len() > 1 {
                        rows.push((repository.root.clone(), None));
                    }
                    for area in [Area::Staged, Area::Working, Area::Untracked] {
                        rows.extend(
                            repository
                                .changes
                                .iter()
                                .filter(|change| change.area == area)
                                .map(|change| (repository.root.clone(), Some(change.clone()))),
                        );
                    }
                }
                view.rows = Arc::new(rows);
                if view
                    .active_root
                    .as_ref()
                    .is_none_or(|root| !view.roots.contains(root))
                {
                    view.active_root = view.roots.first().cloned();
                }
                let selection =
                    view.selection
                        .as_ref()
                        .filter(|(root, selected)| {
                            Some(root) == view.active_root.as_ref()
                                && view.repositories.iter().any(|repo| {
                                    &repo.root == root && repo.changes.contains(selected)
                                })
                        })
                        .cloned()
                        .or_else(|| {
                            view.repositories
                                .iter()
                                .filter(|repo| Some(&repo.root) == view.active_root.as_ref())
                                .find_map(|repo| {
                                    repo.changes
                                        .first()
                                        .map(|change| (repo.root.clone(), change.clone()))
                                })
                        });
                if view.section != Section::Changes {
                    view.browse(cx);
                    if let Some((root, comparison)) = view.comparison.clone() {
                        view.load_patch(root, None, Some(comparison), cx);
                    }
                } else if let Some((root, Comparison::AllChanges)) = view.comparison.clone() {
                    view.load_patch(root, None, Some(Comparison::AllChanges), cx);
                } else if let Some((root, change)) = selection {
                    view.select(root, change, cx);
                } else {
                    view.selection = None;
                    view.clear_patch(cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn clear_patch(&mut self, cx: &mut Context<Self>) {
        self.diff.update(cx, |state, cx| state.set_files([], cx));
        self.raw = None;
        self.memory = None;
        self.patch_error = None;
        self.patch_source = None;
    }

    fn current_tab(&self) -> Option<DiffTab> {
        self.comparison
            .clone()
            .map(|(root, query)| DiffTab::Comparison(root, query))
            .or_else(|| {
                self.selection
                    .clone()
                    .filter(|_| self.section == Section::Changes)
                    .map(|(root, change)| DiffTab::File(root, change))
            })
    }

    pub(super) fn active_tab(&self) -> Option<usize> {
        let current = self.current_tab()?;
        self.tabs.iter().position(|tab| tab == &current)
    }

    pub(super) fn activate_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        match self.tabs.get(index).cloned() {
            Some(DiffTab::File(root, change)) => self.select(root, change, cx),
            Some(DiffTab::Comparison(root, query)) => {
                self.cancel_browse();
                self.active_root = Some(root);
                self.section = match &query {
                    Comparison::AllChanges => Section::Changes,
                    Comparison::Stash(_) => Section::Stashes,
                    Comparison::Between(_, _) => Section::Branches,
                    _ => Section::History,
                };
                self.selected_commit = match &query {
                    Comparison::Commit(id)
                    | Comparison::FileHistory(id, _)
                    | Comparison::Details(id) => {
                        self.commits.iter().find(|commit| &commit.id == id).cloned()
                    }
                    _ => None,
                };
                self.selected_reference = None;
                self.selected_stash = None;
                self.selection = None;
                self.show_comparison(query, cx);
            }
            None => return,
        }
        cx.notify();
    }

    pub(super) fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        let active = self.active_tab() == Some(index);
        self.tabs.remove(index);
        if active {
            if self.tabs.is_empty() {
                self.patch_cancel = None;
                self.patch_task = None;
                self.patch_loading = false;
                self.selection = None;
                self.comparison = None;
                self.clear_patch(cx);
            } else {
                self.activate_tab(index.min(self.tabs.len() - 1), cx);
            }
        }
        cx.notify();
    }

    fn select(&mut self, root: PathBuf, change: Change, cx: &mut Context<Self>) {
        if self.active_root.as_ref() != Some(&root) && self.busy(cx) {
            return;
        }
        self.cancel_browse();
        self.section = Section::Changes;
        self.active_root = Some(root.clone());
        self.comparison = None;
        self.selection = Some((root.clone(), change.clone()));
        self.load_patch(root, Some(change), None, cx);
    }

    fn load_patch(
        &mut self,
        root: PathBuf,
        change: Option<Change>,
        comparison: Option<Comparison>,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.current_tab()
            && !self.tabs.contains(&tab)
        {
            self.tabs.push(tab);
        }
        self.patch_cancel = None;
        self.patch_task = None;
        self.clear_patch(cx);
        self.patch_loading = true;
        let generation = self.generation;
        let selection = self.selection.clone();
        let comparison_guard = self.comparison.clone();
        let remote = self.remote.clone();
        let (cancel, guard) = Cancellation::task();
        self.patch_cancel = Some(guard);
        let work = cx.background_executor().spawn(async move {
            let conflict = change.as_ref().is_some_and(|c| c.status == 'U');
            let patch = match (&change, &comparison) {
                (Some(change), _) if conflict => {
                    git::conflict_source(&root, remote.as_ref(), &change.path, &cancel)?
                }
                (Some(change), _) => git::patch(&root, change, remote.as_ref(), &cancel)?,
                (_, Some(comparison)) => {
                    git::comparison_patch(&root, remote.as_ref(), comparison, &cancel)?
                }
                _ => anyhow::bail!("Missing Git diff selection"),
            };
            if comparison.as_ref().is_some_and(Comparison::is_raw) {
                return Ok(Prepared::Raw(glim_services::paged::generated(
                    &PathBuf::from("git-details.txt"),
                    patch,
                )?));
            }
            let bytes = patch.as_file().metadata()?.len();
            let mut input = patch.reopen()?;
            let mut buffer = vec![0; glim_services::limits::IO_BUFFER_BYTES];
            let mut lines = 0u64;
            loop {
                use std::io::Read as _;
                cancel.check()?;
                let read = input.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                lines = lines.saturating_add(
                    buffer[..read].iter().filter(|byte| **byte == b'\n').count() as u64,
                );
            }
            // Reserve a conservative parser envelope before the library allocates
            // per-line strings, source ranges and presentation rows.
            let envelope = bytes
                .saturating_mul(std::mem::size_of::<usize>() as u64)
                .saturating_add(lines.saturating_mul(std::mem::size_of::<DiffFile>() as u64));
            let memory = glim_services::resources::pool().reserve_working(envelope, envelope);
            let mut binary = false;
            if let Ok(memory) = memory {
                let text = glim_services::resources::read_to_end(patch.reopen()?, &cancel);
                match text {
                    Ok(text) => match String::from_utf8(text) {
                        Ok(text) => {
                            cancel.check()?;
                            let files = if conflict {
                                vec![DiffFile::parse_conflicts(
                                    change.as_ref().unwrap().path.to_string_lossy().into_owned(),
                                    &text,
                                )?]
                            } else {
                                DiffFile::parse(&text)?
                            };
                            cancel.check()?;
                            return Ok::<_, anyhow::Error>(Prepared::Diff(
                                files,
                                memory,
                                Arc::new(text),
                            ));
                        }
                        Err(_) => binary = true,
                    },
                    Err(error) if !glim_services::resources::is_memory_pressure(&error) => {
                        return Err(error);
                    }
                    Err(_) => (),
                }
            }
            cancel.check()?;
            let path = change
                .as_ref()
                .map(|c| c.path.with_extension("diff"))
                .unwrap_or_else(|| PathBuf::from("comparison.diff"));
            Ok(Prepared::Raw(if binary {
                glim_services::paged::generated_binary(&path, patch)?
            } else {
                glim_services::paged::generated(&path, patch)?
            }))
        });
        self.patch_task = Some(cx.spawn(async move |view, cx| {
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                if view.generation != generation
                    || view.selection != selection
                    || view.comparison != comparison_guard
                {
                    return;
                }
                view.patch_loading = false;
                match result {
                    Ok(Prepared::Diff(files, memory, source)) => {
                        view.patch_source = Some(source);
                        view.diff.update(cx, |state, cx| state.set_files(files, cx));
                        view.memory = Some(memory);
                    }
                    Ok(Prepared::Raw(page)) => {
                        view.raw = Some(
                            cx.new(|_| crate::views::reader::paged::PagedReader::text_only(page)),
                        )
                    }
                    Err(error) => view.patch_error = Some(format!("{error:#}").into()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn preferences(cx: &App) -> glim_services::preferences::GitPreferences {
        cx.try_global::<crate::app::GitSettings>()
            .map(|settings| settings.preferences)
            .unwrap_or_default()
    }

    fn update_refresh_timer(&mut self, cx: &mut Context<Self>) {
        if !self.monitoring || !Self::preferences(cx).auto_refresh {
            self.refresh_timer = None;
        } else if self.refresh_timer.is_none() {
            self.refresh_timer = Some(cx.spawn(async move |view, cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(60))
                        .await;
                    if view
                        .update(cx, |view, cx| {
                            if !view.loading
                                && !view.patch_loading
                                && !view.browsing
                                && !view.busy(cx)
                            {
                                view.refresh(cx);
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.load(self.roots.clone(), self.remote.clone(), cx);
    }

    fn area(area: Area) -> &'static str {
        match area {
            Area::Working => "Unstaged",
            Area::Staged => "Staged",
            Area::Untracked => "Untracked",
        }
    }

    fn navigation(&self, cx: &Context<Self>) -> AnyElement {
        let view = self;
        let rows = view.rows.clone();
        let scroll = view.scroll.clone();
        let owner = cx.entity();
        v_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .when(view.loading, |v| {
                v.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr("Loading changes…")),
                )
            })
            .children(view.errors.iter().enumerate().map(|(index, error)| {
                let error = error.clone();
                Button::new(("git-status-error", index))
                    .ghost()
                    .small()
                    .w_full()
                    .label(tr("Git query failed"))
                    .tooltip(error.clone())
                    .accessibility_label(tr("Copy error details"))
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(error.to_string()))
                    })
            }))
            .when(
                !view.loading
                    && view.repositories.iter().all(|r| r.changes.is_empty())
                    && view.errors.is_empty(),
                |v| {
                    v.child(
                        div()
                            .px_3()
                            .py_2()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(if view.repositories.is_empty() {
                                "No Git repository"
                            } else {
                                "No changes"
                            })),
                    )
                },
            )
            .child(
                uniform_list("git-changes-list", rows.len(), move |range, _, cx| {
                    let view = owner.read(cx);
                    range
                        .map(|index| {
                            let (root, change) = &rows[index];
                            match change {
                                None => Button::new(("git-repository", index))
                                    .ghost()
                                    .w_full()
                                    .justify_start()
                                    .selected(view.active_root.as_ref() == Some(root))
                                    .on_click({
                                        let root = root.clone();
                                        let owner = owner.clone();
                                        move |_, _, cx| {
                                            owner.update(cx, |view, cx| {
                                                view.activate_root(root.clone(), cx);
                                                cx.emit(ShowDiff);
                                            });
                                        }
                                    })
                                    .h(px(32.))
                                    .px_3()
                                    .flex()
                                    .items_center()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        root.file_name()
                                            .unwrap_or(root.as_os_str())
                                            .to_string_lossy()
                                            .into_owned(),
                                    )
                                    .into_any_element(),
                                Some(change) => {
                                    let selected = view.selection.as_ref().is_some_and(
                                        |(selected_root, selected)| {
                                            selected_root == root && selected == change
                                        },
                                    );
                                    let (root, change) = (root.clone(), change.clone());
                                    let click_owner = owner.clone();
                                    let target_root = Some(root.clone());
                                    let menu_owner = owner.clone();
                                    let mut ops = vec![if change.area == Area::Staged {
                                        Operation::Unstage(Some(change.clone()))
                                    } else {
                                        Operation::Stage(Some(change.clone()))
                                    }];
                                    if change.area == Area::Untracked {
                                        ops.push(Operation::Ignore(change.path.clone()));
                                    }
                                    if change.area == Area::Working && change.status != 'U' {
                                        ops.push(Operation::Discard(change.clone()));
                                    }
                                    let actions: Vec<_> = ops
                                        .into_iter()
                                        .map(|op| {
                                            (
                                                controls::operation_icon(&op),
                                                op.menu_label(),
                                                browser::RowAction::Run(op),
                                                false,
                                            )
                                        })
                                        .collect();
                                    let row = Button::new(("git-change", index))
                                        .ghost()
                                        .w_full()
                                        .h(px(32.))
                                        .px_3()
                                        .rounded(px(4.))
                                        .justify_start()
                                        .selected(selected)
                                        .tooltip(format!(
                                            "{} · {}",
                                            tr(Self::area(change.area)),
                                            root.join(&change.path).display()
                                        ))
                                        .accessibility_label(format!(
                                            "{} {} {}",
                                            change.status,
                                            change.path.display(),
                                            tr(Self::area(change.area))
                                        ))
                                        .child(
                                            h_flex()
                                                .w_full()
                                                .min_w_0()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .w(px(14.))
                                                        .flex_shrink_0()
                                                        .text_color(if change.status == 'D' {
                                                            cx.theme().danger
                                                        } else {
                                                            cx.theme().foreground
                                                        })
                                                        .child(change.status.to_string()),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .truncate()
                                                        .text_xs()
                                                        .child(
                                                            change
                                                                .path
                                                                .to_string_lossy()
                                                                .into_owned(),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .flex_shrink_0()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(tr(Self::area(change.area))),
                                                ),
                                        )
                                        .on_click(move |_, _, cx| {
                                            click_owner.update(cx, |view, cx| {
                                                cx.emit(ShowDiff);
                                                view.select(root.clone(), change.clone(), cx)
                                            })
                                        });
                                    use gpui_kit::component::menu::ContextMenuExt;
                                    div()
                                        .id(("git-change-context", index))
                                        .child(row)
                                        .context_menu(move |menu, _, cx| {
                                            menu_owner.update(cx, |view, cx| {
                                                if let Some(root) = &target_root {
                                                    view.activate_root(root.clone(), cx);
                                                }
                                                view.row_menu(
                                                    menu,
                                                    &actions,
                                                    &target_root,
                                                    &menu_owner,
                                                    cx,
                                                )
                                            })
                                        })
                                        .into_any_element()
                                }
                            }
                        })
                        .collect()
                })
                .track_scroll(&scroll)
                .flex_1()
                .min_h_0(),
            )
            .into_any_element()
    }
}

impl Changes {
    pub(super) fn diff_view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.diff.read(cx).mode();
        let ready = !self.patch_loading && !self.diff.read(cx).files().is_empty();
        let toolbar = h_flex()
            .gap_1()
            .child(
                Button::new("diff-open-file")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::FileCode)
                    .tooltip(tr("Open File"))
                    .accessibility_label(tr("Open File"))
                    .disabled(
                        self.comparison.is_some()
                            || self
                                .selection
                                .as_ref()
                                .is_none_or(|(_, change)| change.status == 'D'),
                    )
                    .on_click(cx.listener(|view, _, _, cx| {
                        if let Some((root, change)) = &view.selection {
                            cx.emit(OpenFile(root.join(&change.path)));
                        }
                    })),
            )
            .child(
                Button::new("diff-unified")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::Rows2)
                    .selected(mode == DiffMode::Unified)
                    .tooltip(tr("Unified diff"))
                    .accessibility_label(tr("Unified diff"))
                    .disabled(!ready)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.diff
                            .update(cx, |state, cx| state.set_mode(DiffMode::Unified, cx))
                    })),
            )
            .child(
                Button::new("diff-split")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::Columns2)
                    .selected(mode == DiffMode::Split)
                    .tooltip(tr("Split diff"))
                    .accessibility_label(tr("Split diff"))
                    .disabled(!ready)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.diff
                            .update(cx, |state, cx| state.set_mode(DiffMode::Split, cx))
                    })),
            )
            .child(
                Button::new("diff-wrap")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::CornerDownLeft)
                    .tooltip(tr("Wrap"))
                    .accessibility_label(tr("Wrap"))
                    .selected(self.soft_wrap)
                    .disabled(!ready)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.soft_wrap = !view.soft_wrap;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("diff-previous")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::ArrowUp)
                    .tooltip(tr("Previous change"))
                    .accessibility_label(tr("Previous change"))
                    .disabled(!ready)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.diff.update(cx, |state, cx| state.previous_change(cx))
                    })),
            )
            .child(
                Button::new("diff-next")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::ArrowDown)
                    .tooltip(tr("Next change"))
                    .accessibility_label(tr("Next change"))
                    .disabled(!ready)
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.diff.update(cx, |state, cx| state.next_change(cx))
                    })),
            )
            .child(
                Button::new("diff-refresh")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::RefreshCw)
                    .tooltip(tr("Refresh"))
                    .accessibility_label(tr("Refresh"))
                    .disabled(self.loading)
                    .on_click(cx.listener(|view, _, _, cx| view.refresh(cx))),
            );
        let content = if self.patch_loading || (self.loading && self.selection.is_none()) {
            message(tr("Loading changes…"), cx)
        } else if let Some(error) = self.patch_error.as_ref().or_else(|| {
            self.selection
                .is_none()
                .then(|| self.errors.first())
                .flatten()
        }) {
            let error = error.clone();
            let copied_error = error.clone();
            v_flex()
                .size_full()
                .min_h_0()
                .p_4()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .flex_1()
                        .min_h_0()
                        .id("diff-error-details")
                        .overflow_y_scroll()
                        .child(error),
                )
                .child(
                    Button::new("copy-diff-error")
                        .ghost()
                        .small()
                        .label(tr("Copy error details"))
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                copied_error.to_string(),
                            ))
                        }),
                )
                .child(
                    Button::new("retry-diff")
                        .small()
                        .label(tr("Retry"))
                        .on_click(cx.listener(|view, _, _, cx| view.refresh(cx))),
                )
                .into_any_element()
        } else if let Some(raw) = &self.raw {
            v_flex()
                .size_full()
                .min_h_0()
                .child(
                    div()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(
                            if self.comparison.as_ref().is_some_and(|(_, c)| c.is_raw()) {
                                "Git details"
                            } else {
                                "Showing patch source"
                            },
                        )),
                )
                .child(raw.clone())
                .into_any_element()
        } else if ready {
            Diff::new(&self.diff)
                .soft_wrap(self.soft_wrap)
                .size_full()
                .into_any_element()
        } else if self.selection.is_some() {
            message(tr("No textual changes"), cx)
        } else {
            div().into_any_element()
        };
        let show_toolbar = self.selection.is_some() || self.comparison.is_some();
        v_flex()
            .relative()
            .size_full()
            .min_h_0()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .when(show_toolbar, |view| view.pb(px(48.)))
                    .child(content),
            )
            .when(show_toolbar, |view| {
                view.child(crate::views::reader::controls::capsule(
                    toolbar.into_any_element(),
                    false,
                    false,
                    px(0.),
                    cx,
                ))
            })
            .into_any_element()
    }
}

fn message(text: String, cx: &App) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_owned())
        .into_any_element()
}

#[cfg(all(test, feature = "git"))]
mod tests {
    use super::browser;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, ElementId, Point, TestAppContext, WindowBounds, WindowOptions, px, size,
    };
    use std::process::Command;

    #[gpui_kit::test]
    fn review_commit_and_sync_in_workspace(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("field-notes");
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let git = |args: &[&str]| {
            let output = Command::new("/usr/bin/git")
                .args(args)
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.name", "Glim QA"]);
        git(&["config", "user.email", "qa@example.invalid"]);
        git(&["config", "commit.gpgSign", "false"]);
        std::fs::write(root.join("schedule.md"), "Monday\n").unwrap();
        git(&["add", "schedule.md"]);
        git(&["commit", "-qm", "Initial schedule"]);
        let remote = directory.path().join("upstream.git");
        let remote_path = remote.to_str().unwrap();
        git(&["init", "--bare", "-b", "main", remote_path]);
        git(&["remote", "add", "origin", remote_path]);
        git(&["push", "-u", "origin", "main"]);
        std::fs::write(root.join("schedule.md"), "Tuesday\n").unwrap();
        std::fs::write(root.join("test-note.md"), "Untracked marker\n").unwrap();
        let (handle, workspace) = cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::remote::init(Default::default(), cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::default(),
                        size: size(px(1100.), px(740.)),
                    })),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    cx.new(|cx| super::super::Workspace::new(Some(root.clone()), window, cx))
                },
            )
            .unwrap()
        });
        let click = |cx: &mut TestAppContext, id: ElementId| {
            cx.run_until_parked();
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click(id, cx);
            })
            .unwrap();
            cx.run_until_parked();
        };
        let files_heading = |cx: &mut TestAppContext| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let bounds = window.find("file-panel-files").bounds();
                (bounds.top(), bounds.size.height)
            })
            .unwrap()
        };
        for _ in 0..3 {
            click(cx, "file-panel-files".into());
            assert!(cx.update(|cx| workspace.read(cx).changes_expanded));
            let expanded = files_heading(cx);
            click(cx, "file-panel-files".into());
            assert!(!cx.update(|cx| workspace.read(cx).changes_expanded));
            assert_eq!(expanded, files_heading(cx));
        }
        click(cx, "file-panel-changes".into());
        click(cx, "expanded-changes-heading".into());
        assert!(!cx.update(|cx| workspace.read(cx).changes_expanded));
        click(cx, "file-panel-changes".into());
        click(cx, "diff-split".into());
        click(cx, ("git-change", 1usize).into());
        click(cx, "all-changes-diff".into());
        let patch = cx.update(|cx| {
            workspace
                .read(cx)
                .changes
                .read(cx)
                .patch_source
                .clone()
                .unwrap()
        });
        assert!(patch.contains("+Tuesday") && patch.contains("Untracked marker"));
        click(cx, ("diff-tab", 1usize).into());
        let patch = cx.update(|cx| {
            workspace
                .read(cx)
                .changes
                .read(cx)
                .patch_source
                .clone()
                .unwrap()
        });
        assert!(patch.contains("Untracked marker") && !patch.contains("+Tuesday"));
        click(cx, ("close-diff-tab", 2usize).into());
        click(cx, ("diff-tab", 0usize).into());
        let patch = cx.update(|cx| {
            workspace
                .read(cx)
                .changes
                .read(cx)
                .patch_source
                .clone()
                .unwrap()
        });
        assert!(patch.contains("+Tuesday") && !patch.contains("Untracked marker"));
        std::fs::remove_file(root.join("test-note.md")).unwrap();
        click(cx, "git-stage-file".into());
        assert_eq!(git(&["diff", "--cached", "--name-only"]), "schedule.md\n");
        click(cx, "git-unstage-file".into());
        assert!(git(&["diff", "--cached", "--name-only"]).is_empty());
        click(cx, "git-stage-file".into());
        let input = cx.update(|cx| workspace.read(cx).changes.read(cx).message.entity_id());
        click(cx, ("input", input).into());
        cx.update_window(handle, |_, window, cx| {
            window.input("Move opening to Tuesday", cx)
        })
        .unwrap();
        click(cx, "git-commit".into());
        assert_eq!(
            git(&["log", "-1", "--format=%s"]),
            "Move opening to Tuesday\n"
        );
        assert!(git(&["status", "--porcelain"]).is_empty());
        click(
            cx,
            ("git-section", browser::Section::Branches as usize).into(),
        );
        git(&["branch", "ui-switch-test"]);
        git(&["tag", "0.2.3"]);
        git(&["tag", "0.2.26"]);
        click(
            cx,
            ("git-section", browser::Section::Changes as usize).into(),
        );
        click(cx, "git-commit-refresh".into());
        for index in [1usize, 0] {
            click(cx, "git-branch-picker".into());
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                w.within("popup-menu").click(index, cx);
            })
            .unwrap();
            cx.run_until_parked();
            assert_eq!(
                git(&["branch", "--show-current"]).trim(),
                if index == 1 { "ui-switch-test" } else { "main" }
            );
        }
        click(cx, "git-settings".into());
        assert!(cx.update(|cx| workspace.read(cx).changes.read(cx).identity_ready));
        assert_eq!(
            cx.update(|cx| workspace.read(cx).changes.read(cx).identity[0]
                .read(cx)
                .value()
                .to_string()),
            "Glim QA"
        );
        click(
            cx,
            ("git-section", browser::Section::Branches as usize).into(),
        );
        cx.update(|cx| {
            let catalog = &workspace.read(cx).changes.read(cx).catalog;
            assert_eq!(catalog.default_branch.as_deref(), Some("refs/heads/main"));
            let tags: Vec<_> = catalog
                .references
                .iter()
                .filter(|r| r.full.starts_with("refs/tags/"))
                .map(|r| r.name.as_str())
                .collect();
            assert_eq!(tags, ["0.2.26", "0.2.3"]);
        });
        for group in [0usize, 1] {
            click(cx, ("git-reference-group", group).into());
            assert!(cx.update(|cx| workspace.read(cx).changes.read(cx).collapsed_refs[group]));
            click(cx, ("git-reference-group", group).into());
        }
        for (form, field) in [
            ("git-new-tag", 1usize),
            ("git-new-branch", 1),
            ("git-compare-refs", 0),
            ("git-compare-refs", 1),
        ] {
            click(cx, form.into());
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                assert!(
                    w.find(("revision-picker", field))
                        .label()
                        .unwrap()
                        .ends_with(" · main")
                );
            })
            .unwrap();
            click(cx, ("revision-picker", field).into());
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                w.within("popup-menu").click(1usize, cx);
            })
            .unwrap();
            click(cx, "git-form-cancel".into());
        }
        for (section, id) in [
            (browser::Section::Remotes, "git-subsection"),
            (browser::Section::Worktrees, "git-section"),
        ] {
            click(cx, (id, section as usize).into());
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                w.right_click(("git-browser-row", 0usize), cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                w.within("popup-menu").find(0usize);
                w.press("escape", cx);
            })
            .unwrap();
        }
        click(
            cx,
            ("git-section", browser::Section::Changes as usize).into(),
        );
        for (section, id) in [
            (browser::Section::Stashes, "git-subsection"),
            (browser::Section::History, "git-section"),
        ] {
            click(cx, (id, section as usize).into());
            assert!(cx.update(|cx| workspace.read(cx).changes.read(cx).section == section));
        }
        assert_eq!(
            cx.update(|cx| workspace.read(cx).changes.read(cx).commits.len()),
            2
        );
        click(cx, "git-compare".into());
        for field in [0usize, 1] {
            cx.update_window(handle, |_, w, cx| {
                w.render_frame(cx);
                assert!(
                    w.find(("revision-picker", field))
                        .label()
                        .unwrap()
                        .ends_with(" · main")
                );
            })
            .unwrap();
            click(cx, ("revision-picker", field).into());
            cx.update_window(handle, |_, win, cx| {
                win.render_frame(cx);
                win.within("popup-menu").click(1usize, cx);
            })
            .unwrap();
        }
        click(cx, "git-form-cancel".into());
        click(cx, ("git-browser-row", 1usize).into());
        assert_eq!(
            cx.update(|cx| workspace
                .read(cx)
                .changes
                .read(cx)
                .selected_commit
                .as_ref()
                .unwrap()
                .subject
                .clone()),
            "Initial schedule"
        );
        click(
            cx,
            ("git-section", browser::Section::Changes as usize).into(),
        );
        click(cx, "git-push".into());
        assert_eq!(
            git(&["rev-parse", "HEAD"]),
            git(&["--git-dir", remote_path, "rev-parse", "main"])
        );
        let peer = directory.path().join("peer");
        let peer_path = peer.to_str().unwrap();
        git(&["clone", "-q", remote_path, peer_path]);
        std::fs::write(peer.join("receipt.md"), "Accepted").unwrap();
        git(&["-C", peer_path, "add", "receipt.md"]);
        git(&[
            "-C",
            peer_path,
            "-c",
            "user.name=Peer",
            "-c",
            "user.email=peer@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "-qm",
            "Accept schedule",
        ]);
        git(&["-C", peer_path, "push", "origin", "main"]);
        for action in ["git-fetch", "git-pull"] {
            click(cx, action.into());
            assert_eq!(
                git(&["rev-parse", "origin/main"]),
                git(&["-C", peer_path, "rev-parse", "HEAD"])
            );
        }
        assert_eq!(
            std::fs::read_to_string(root.join("receipt.md")).unwrap(),
            "Accepted"
        );
        click(cx, "file-panel-files".into());
    }
}
