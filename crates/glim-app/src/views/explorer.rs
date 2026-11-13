use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
};

use glim_core::DirectoryEntry;
use glim_services::workspace::list_directory_cancellable;
use gpui_kit::{
    base::TestSupportExt,
    component::{
        ActiveTheme, Disableable, FocusableExt, Icon,
        button::{Button, ButtonCustomVariant, ButtonVariants},
    },
    prelude::FluentBuilder,
    *,
};

mod moving;
use moving::DraggedEntry;

pub enum ExplorerEvent {
    SelectionChanged,
    OpenFile(PathBuf),
    NewFile(PathBuf),
    NewFolder(PathBuf),
    RenameEntry(PathBuf),
    RemoveFolder(u64),
    TrashEntry(PathBuf),
    MoveEntry(PathBuf, PathBuf),
    ChooseMoveEntry(PathBuf),
    PlayFolder(PathBuf, bool),
    Error(String),
    ExpansionChanged(Vec<PathBuf>, glim_services::preferences::ExplorerExpansion),
}

gpui_kit::actions!(
    explorer,
    [Up, Down, Expand, Collapse, OpenSelected, MoveSelected]
);

const DIRECTORY_SCAN_CONCURRENCY: usize = 32;
const MAX_ANCESTOR_ROWS: usize = 4;

type FoldedAncestors = HashMap<(usize, PathBuf), Vec<PathBuf>>;

#[derive(Clone)]
struct Row {
    root_index: usize,
    entry: DirectoryEntry,
    depth: usize,
    parent: Option<usize>,
    subtree_end: usize,
}

impl Row {
    fn element_id(&self, part: &'static str) -> ElementId {
        let id = entry_id(&self.entry.path, part);
        if self.root_index == 0 {
            id
        } else {
            ElementId::NamedChild(std::sync::Arc::new(id), self.root_index.to_string().into())
        }
    }
}

#[derive(Default)]
struct StickyRows {
    items: Vec<StickyRow>,
    hidden: Vec<usize>,
}

struct StickyRow {
    index: Option<usize>,
    depth: usize,
    y: f32,
}

impl StickyRows {
    fn display_depth(&self, index: usize, rows: &[Row]) -> usize {
        let depth = rows[index].depth;
        if let Some(&parent) = self.hidden.last()
            && index > parent
            && index < rows[parent].subtree_end
        {
            depth.saturating_sub(self.hidden.len().saturating_sub(1))
        } else {
            depth
        }
    }
}

#[derive(Clone, Copy)]
enum FileMenuAction {
    NewFile,
    NewFolder,
    Rename,
    Play(bool),
    Open,
    Reveal,
    CopyRelative,
    CopyAbsolute,
    Trash,
    Move,
    RemoveFolder(u64),
}

/// Only expanded directories are loaded, and only visible rows are rendered.
pub struct Explorer {
    remote: Option<glim_services::remote::Session>,
    roots: Vec<PathBuf>,
    root_ids: Vec<u64>,
    next_root_id: u64,
    directories: HashMap<PathBuf, Vec<DirectoryEntry>>,
    expanded: Vec<HashSet<PathBuf>>,
    tasks: HashMap<PathBuf, Task<()>>,
    expansion_tasks: HashMap<u64, Task<()>>,
    pending_refresh: HashSet<PathBuf>,
    directory_update: Option<Task<()>>,
    expansion_dirty: bool,
    media_presence: HashMap<PathBuf, bool>,
    media_tasks: HashMap<PathBuf, Task<()>>,
    rows: Vec<Row>,
    deepest_row: Option<usize>,
    folded_ancestors: FoldedAncestors,
    clipped_labels: HashSet<(usize, PathBuf, bool)>,
    path_hint: super::path_hint::PathHint,
    selected: Option<usize>,
    clicked_path: Option<PathBuf>,
    clicked_root: Option<usize>,
    pending_reveal: Option<PathBuf>,
    pending_reveal_root: Option<u64>,
    preserve_next_reveal: Option<PathBuf>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    show_all_ancestors: bool,
    context_menu_task: Option<Task<()>>,
    drag_hover: Option<(PathBuf, Task<()>)>,
    drag_scroll: Option<Task<()>>,
}

impl EventEmitter<ExplorerEvent> for Explorer {}

impl Explorer {
    pub fn init(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("up", Up, Some("Explorer")),
            KeyBinding::new("down", Down, Some("Explorer")),
            KeyBinding::new("right", Expand, Some("Explorer")),
            KeyBinding::new("left", Collapse, Some("Explorer")),
            KeyBinding::new("enter", OpenSelected, Some("Explorer")),
            KeyBinding::new("secondary-shift-m", MoveSelected, Some("Explorer")),
        ]);
    }

    pub fn new(root: PathBuf, entries: Vec<DirectoryEntry>, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            remote: None,
            roots: vec![root.clone()],
            root_ids: vec![0],
            next_root_id: 1,
            directories: HashMap::from([(root.clone(), entries)]),
            expanded: vec![HashSet::from([root])],
            tasks: HashMap::new(),
            expansion_tasks: HashMap::new(),
            pending_refresh: HashSet::new(),
            directory_update: None,
            expansion_dirty: false,
            media_presence: HashMap::new(),
            media_tasks: HashMap::new(),
            rows: Vec::new(),
            deepest_row: None,
            folded_ancestors: HashMap::new(),
            clipped_labels: HashSet::new(),
            path_hint: Default::default(),
            selected: None,
            clicked_path: None,
            clicked_root: None,
            pending_reveal: None,
            pending_reveal_root: None,
            preserve_next_reveal: None,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            show_all_ancestors: false,
            context_menu_task: None,
            drag_hover: None,
            drag_scroll: None,
        };
        view.rebuild();
        view
    }
    pub fn set_remote(&mut self, remote: Option<glim_services::remote::Session>) {
        self.remote = remote;
    }

    pub fn add_folder(
        &mut self,
        root: PathBuf,
        entries: Vec<DirectoryEntry>,
        cx: &mut Context<Self>,
    ) {
        self.roots.push(root.clone());
        self.root_ids.push(self.next_root_id);
        self.next_root_id += 1;
        self.expanded.push(HashSet::from([root.clone()]));
        self.directories.insert(root, entries);
        self.rebuild();
        cx.notify();
    }

    /// Remove one root occurrence without touching its files or other root entries.
    pub fn remove_folder(&mut self, root_id: u64, cx: &mut Context<Self>) -> Option<usize> {
        let index = self.root_ids.iter().position(|id| *id == root_id)?;
        self.expansion_tasks.remove(&root_id);
        let selected = self
            .selected
            .and_then(|i| self.rows.get(i))
            .map(|row| (self.root_ids[row.root_index], row.entry.path.clone()));
        let clicked = self
            .clicked_root
            .and_then(|i| self.root_ids.get(i))
            .copied();
        self.roots.remove(index);
        self.root_ids.remove(index);
        self.expanded.remove(index);
        self.selected = None;
        self.clicked_root =
            clicked.and_then(|id| self.root_ids.iter().position(|root| *root == id));
        if self.clicked_root.is_none() {
            self.clicked_path = None;
        }
        self.pending_reveal = None;
        self.pending_reveal_root = None;
        self.preserve_next_reveal = None;
        self.clipped_labels.clear();
        let roots = &self.roots;
        let retained = |path: &PathBuf| roots.iter().any(|root| path.starts_with(root));
        self.directories.retain(|path, _| retained(path));
        self.tasks.retain(|path, _| retained(path));
        self.pending_refresh.retain(|path| retained(path));
        self.media_presence.retain(|path, _| retained(path));
        self.media_tasks.retain(|path, _| retained(path));
        self.rows.clear();
        self.rebuild();
        if let Some((id, path)) = selected {
            self.selected = self
                .rows
                .iter()
                .position(|row| self.root_ids[row.root_index] == id && row.entry.path == path);
        }
        cx.notify();
        Some(index)
    }

    pub fn root_ids_under(&self, path: &std::path::Path) -> Vec<u64> {
        self.roots
            .iter()
            .zip(&self.root_ids)
            .filter(|(root, _)| root.starts_with(path))
            .map(|(_, id)| *id)
            .collect()
    }

    pub fn clear_clicked_selection(&mut self, cx: &mut Context<Self>) {
        self.clicked_path = None;
        self.clicked_root = None;
        self.pending_reveal = None;
        self.pending_reveal_root = None;
        self.preserve_next_reveal = None;
        cx.notify();
    }

    pub fn expansion_state(&self) -> (Vec<PathBuf>, glim_services::preferences::ExplorerExpansion) {
        let expanded = self
            .roots
            .iter()
            .cloned()
            .zip(self.expanded.iter().map(|paths| {
                let mut paths = paths.iter().cloned().collect::<Vec<_>>();
                paths.sort();
                paths
            }))
            .collect();
        (self.roots.clone(), expanded)
    }

    fn expansion_changed(&self, cx: &mut Context<Self>) {
        let (roots, expanded) = self.expansion_state();
        cx.emit(ExplorerEvent::ExpansionChanged(roots, expanded));
    }

    pub fn restore_expanded(
        &mut self,
        paths: glim_services::preferences::ExplorerExpansion,
        cx: &mut Context<Self>,
    ) {
        self.expansion_tasks.clear();
        self.expanded = self
            .roots
            .iter()
            .map(|root| {
                paths
                    .get(root)
                    .into_iter()
                    .flatten()
                    .filter(|path| {
                        !path
                            .components()
                            .any(|part| part == std::path::Component::ParentDir)
                            && path.starts_with(root)
                    })
                    .cloned()
                    .collect()
            })
            .collect();
        self.rebuild();
        self.load_expanded(cx);
        cx.notify();
    }

    fn load_expanded(&mut self, cx: &mut Context<Self>) {
        let paths = self
            .rows
            .iter()
            .filter(|row| {
                row.entry.is_dir
                    && self.expanded[row.root_index].contains(&row.entry.path)
                    && !self
                        .expansion_tasks
                        .contains_key(&self.root_ids[row.root_index])
                    && !self.directories.contains_key(&row.entry.path)
                    && !self.tasks.contains_key(&row.entry.path)
            })
            .take(DIRECTORY_SCAN_CONCURRENCY.saturating_sub(self.tasks.len()))
            .map(|row| row.entry.path.clone())
            .collect::<Vec<_>>();
        for path in paths {
            self.load_directory(path, cx);
        }
    }

    fn schedule_directory_update(&mut self, cx: &mut Context<Self>) {
        if self.directory_update.is_some() {
            return;
        }
        // Directory completions share one rebuild instead of rebuilding per entry.
        self.directory_update = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(16))
                .await;
            let _ = view.update(cx, |view, cx| {
                view.directory_update = None;
                view.start_pending_directories(cx);
                view.rebuild();
                view.advance_reveal(cx);
                view.load_expanded(cx);
                if std::mem::take(&mut view.expansion_dirty) {
                    view.expansion_changed(cx);
                }
                cx.notify();
            });
        }));
    }

    fn set_all_folders_expanded(&mut self, root_id: u64, expand: bool, cx: &mut Context<Self>) {
        if self.remote.is_some() {
            return;
        }
        if expand && self.expansion_tasks.contains_key(&root_id) {
            return;
        }
        let Some(root_index) = self.root_ids.iter().position(|id| *id == root_id) else {
            return;
        };
        let root = self.roots[root_index].clone();
        self.expansion_tasks.remove(&root_id);
        self.pending_reveal = None;
        self.pending_reveal_root = None;
        self.preserve_next_reveal = None;
        self.show_all_ancestors = false;
        if !expand {
            self.expanded[root_index].clear();
        } else {
            self.expanded[root_index].insert(root.clone());
            let (cancel, guard) = glim_services::work::Cancellation::task();
            let task = cx.spawn(async move |view, cx| {
                let _guard = guard;
                let mut paths = VecDeque::from([root]);
                while !paths.is_empty() {
                    let reads = (0..DIRECTORY_SCAN_CONCURRENCY)
                        .filter_map(|_| paths.pop_front())
                        .map(|path| {
                            let read_cancel = cancel.clone();
                            cx.background_executor().spawn(async move {
                                let entries = list_directory_cancellable(&path, &read_cancel);
                                (path, entries)
                            })
                        })
                        .collect::<Vec<_>>();
                    let mut batch = Vec::with_capacity(reads.len());
                    for read in reads {
                        batch.push(read.await);
                    }
                    let Ok(next) = view.update(cx, |view, cx| {
                        let Some(root_index) = view.root_ids.iter().position(|id| *id == root_id)
                        else {
                            return VecDeque::new();
                        };
                        let mut next = paths;
                        let mut first_error = None;
                        for (path, entries) in batch {
                            match entries {
                                Ok(entries) => {
                                    next.extend(
                                        entries
                                            .iter()
                                            .filter(|entry| entry.is_dir && !entry.is_symlink)
                                            .map(|entry| entry.path.clone()),
                                    );
                                    view.expanded[root_index].insert(path.clone());
                                    view.directories.insert(path, entries);
                                }
                                Err(error) => {
                                    view.expanded[root_index].remove(&path);
                                    first_error.get_or_insert(error);
                                }
                            }
                        }
                        view.expansion_dirty = true;
                        view.schedule_directory_update(cx);
                        if let Some(error) = first_error {
                            let gone =
                                error.downcast_ref::<std::io::Error>().is_some_and(|error| {
                                    matches!(
                                        error.kind(),
                                        std::io::ErrorKind::NotFound
                                            | std::io::ErrorKind::NotADirectory
                                    )
                                });
                            if !gone {
                                cx.emit(ExplorerEvent::Error(format!("{error:#}")));
                            }
                        }
                        next
                    }) else {
                        return;
                    };
                    paths = next;
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(16))
                        .await;
                }
                let _ = view.update(cx, |view, cx| {
                    view.expansion_tasks.remove(&root_id);
                    cx.notify();
                });
            });
            self.expansion_tasks.insert(root_id, task);
        }
        self.rebuild();
        self.expansion_changed(cx);
        cx.notify();
    }

    fn load_directory(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.directories.contains_key(&path) || self.tasks.contains_key(&path) {
            return;
        }
        self.refresh_directory(path, cx);
    }

    fn start_pending_directories(&mut self, cx: &mut Context<Self>) {
        let paths = self
            .pending_refresh
            .iter()
            .filter(|path| !self.tasks.contains_key(*path))
            .take(DIRECTORY_SCAN_CONCURRENCY.saturating_sub(self.tasks.len()))
            .cloned()
            .collect::<Vec<_>>();
        for path in paths {
            self.pending_refresh.remove(&path);
            self.refresh_directory(path, cx);
        }
    }

    pub fn reveal_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        // Opening a row feeds back through tab activation/content installation.
        // Keep that click's scroll position; explicit tab changes still reveal.
        if self.preserve_next_reveal.take().as_ref() == Some(&path) {
            self.pending_reveal = None;
            self.pending_reveal_root = None;
            cx.notify();
            return;
        }
        // Several activation/install callbacks may reveal the same file. The
        // clicked occurrence is already correct, even when another root also
        // contains that path; do not recenter or switch root occurrences.
        if self.clicked_path.as_ref() == Some(&path)
            && self
                .selected
                .and_then(|index| self.rows.get(index))
                .is_some_and(|row| {
                    row.entry.path == path && self.clicked_root == Some(row.root_index)
                })
        {
            self.pending_reveal = None;
            self.pending_reveal_root = None;
            return;
        }
        // Capture the root identity before clearing selection. IDs survive
        // asynchronous directory loads and distinguish duplicate root entries.
        self.pending_reveal_root = self
            .clicked_root
            .filter(|index| {
                self.roots
                    .get(*index)
                    .is_some_and(|root| path.starts_with(root))
            })
            .and_then(|index| self.root_ids.get(index))
            .copied()
            .or_else(|| {
                (self.pending_reveal.as_ref() == Some(&path))
                    .then_some(self.pending_reveal_root)
                    .flatten()
            });
        self.selected = None;
        self.pending_reveal = self
            .roots
            .iter()
            .any(|root| path.starts_with(root))
            .then_some(path);
        if self.pending_reveal.is_some() {
            self.clicked_path = None;
            self.clicked_root = None;
            cx.emit(ExplorerEvent::SelectionChanged);
        }
        self.advance_reveal(cx);
        cx.notify();
    }

    fn advance_reveal(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.pending_reveal.clone() else {
            return;
        };
        let Some((root_index, root)) = self
            .roots
            .iter()
            .enumerate()
            .filter(|(_, root)| target.starts_with(root))
            .max_by_key(|(index, root)| {
                (
                    self.pending_reveal_root == self.root_ids.get(*index).copied(),
                    root.components().count(),
                )
            })
            .map(|(index, root)| (index, root.clone()))
        else {
            return;
        };
        let mut ancestors = target
            .parent()
            .into_iter()
            .flat_map(|parent| parent.ancestors())
            .take_while(|path| path.starts_with(&root))
            .map(std::path::Path::to_path_buf)
            .collect::<Vec<_>>();
        ancestors.reverse();
        let mut changed = false;
        let mut ready = true;
        for path in ancestors {
            if path != root {
                let Some(entries) = path
                    .parent()
                    .and_then(|parent| self.directories.get(parent))
                else {
                    ready = false;
                    break;
                };
                if !entries
                    .iter()
                    .any(|entry| entry.path == path && entry.is_dir)
                {
                    self.pending_reveal = None;
                    self.pending_reveal_root = None;
                    ready = false;
                    break;
                }
            }
            changed |= self.expanded[root_index].insert(path.clone());
            if !self.directories.contains_key(&path) {
                self.load_directory(path, cx);
                ready = false;
                break;
            }
        }
        self.rebuild();
        self.load_expanded(cx);
        if changed {
            self.expansion_changed(cx);
        }
        if ready {
            self.pending_reveal = None;
            self.pending_reveal_root = None;
            if let Some(index) = self
                .rows
                .iter()
                .position(|row| row.root_index == root_index && row.entry.path == target)
            {
                self.selected = Some(index);
                self.clicked_path = Some(target);
                self.clicked_root = Some(root_index);
                self.scroll
                    .scroll_to_item_strict(index, ScrollStrategy::Center);
            }
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_changed(&glim_services::watch::Changes::all(), cx);
    }

    pub(crate) fn remote_refresh_paths(&self) -> Vec<PathBuf> {
        let mut paths = self.roots.clone();
        paths.extend(
            self.rows
                .iter()
                .filter(|row| {
                    row.entry.is_dir && self.expanded[row.root_index].contains(&row.entry.path)
                })
                .map(|row| row.entry.path.clone()),
        );
        paths.extend(self.folded_ancestors.values().flatten().cloned());
        paths.sort();
        paths.dedup();
        paths
    }

    pub(crate) fn apply_remote_listing(
        &mut self,
        path: PathBuf,
        entries: Vec<DirectoryEntry>,
        cx: &mut Context<Self>,
    ) {
        if self.tasks.contains_key(&path)
            || !self.remote_refresh_paths().contains(&path)
            || self.directories.get(&path) == Some(&entries)
        {
            return;
        }
        self.directories.insert(path, entries);
        self.schedule_directory_update(cx);
    }

    pub(crate) fn did_trash(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        self.tasks.retain(|entry, _| !entry.starts_with(path));
        self.pending_refresh
            .retain(|entry| !entry.starts_with(path));
        self.expansion_tasks.clear();
        self.directories.retain(|entry, _| !entry.starts_with(path));
        for entries in self.directories.values_mut() {
            entries.retain(|entry| !entry.path.starts_with(path));
        }
        for expanded in &mut self.expanded {
            expanded.retain(|entry| !entry.starts_with(path));
        }
        self.media_tasks.retain(|entry, _| !entry.starts_with(path));
        self.media_presence
            .retain(|entry, _| !entry.starts_with(path));
        if self
            .pending_reveal
            .as_ref()
            .is_some_and(|entry| entry.starts_with(path))
        {
            self.pending_reveal = None;
            self.pending_reveal_root = None;
        }
        self.drag_hover = None;
        self.expansion_dirty = true;
        self.schedule_directory_update(cx);
    }

    pub fn refresh_changed(
        &mut self,
        changes: &glim_services::watch::Changes,
        cx: &mut Context<Self>,
    ) {
        self.media_tasks.retain(|path, _| !changes.affects(path));
        self.media_presence.retain(|path, _| !changes.affects(path));
        // Saved descendants are discovered from their loaded parents during restore.
        let mut paths = self
            .directories
            .keys()
            .chain(&self.roots)
            .filter(|path| changes.affects(path))
            .cloned()
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        for path in paths {
            self.refresh_directory(path, cx);
        }
    }

    fn refresh_directory(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        // Finish the current scan before replaying changes received during it.
        if self.tasks.contains_key(&path) || self.tasks.len() >= DIRECTORY_SCAN_CONCURRENCY {
            self.pending_refresh.insert(path);
            return;
        }
        let read_path = path.clone();
        let remote = self.remote.clone();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let read = cx.background_executor().spawn(async move {
            match remote {
                Some(remote) => remote.list_entries(&read_path, &cancel),
                None => list_directory_cancellable(&read_path, &cancel),
            }
        });
        let key = path.clone();
        let task = cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = read.await;
            let _ = view.update(cx, |view, cx| {
                match result {
                    Ok(entries) => {
                        view.directories.insert(path.clone(), entries);
                    }
                    Err(error) => {
                        view.directories.remove(&path);
                        view.pending_refresh.remove(&path);
                        for expanded in &mut view.expanded {
                            expanded.remove(&path);
                        }
                        if view
                            .pending_reveal
                            .as_ref()
                            .is_some_and(|target| target.starts_with(&path))
                        {
                            view.pending_reveal = None;
                            view.pending_reveal_root = None;
                        }
                        view.expansion_dirty = true;
                        let gone = error.downcast_ref::<std::io::Error>().is_some_and(|error| {
                            matches!(
                                error.kind(),
                                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                            )
                        });
                        if !gone {
                            cx.emit(ExplorerEvent::Error(format!("{error:#}")));
                        }
                    }
                }
                view.tasks.remove(&path);
                view.schedule_directory_update(cx);
            });
        });
        self.tasks.insert(key, task);
    }

    fn rebuild(&mut self) {
        fn append(
            path: &PathBuf,
            root_index: usize,
            depth: usize,
            parent: Option<usize>,
            this: &Explorer,
            rows: &mut Vec<Row>,
        ) {
            if let Some(children) = this.directories.get(path) {
                for entry in children {
                    let index = rows.len();
                    rows.push(Row {
                        root_index,
                        entry: entry.clone(),
                        depth,
                        parent,
                        subtree_end: index + 1,
                    });
                    if this.expanded[root_index].contains(&entry.path) {
                        append(&entry.path, root_index, depth + 1, Some(index), this, rows);
                    }
                    rows[index].subtree_end = rows.len();
                }
            }
        }
        let selected = self
            .selected
            .and_then(|i| self.rows.get(i))
            .map(|r| (r.root_index, r.entry.path.clone()));
        let mut rows = Vec::new();
        for (root_index, root) in self.roots.iter().enumerate() {
            let index = rows.len();
            rows.push(Row {
                root_index,
                entry: DirectoryEntry {
                    path: root.clone(),
                    is_dir: true,
                    is_symlink: false,
                },
                depth: 0,
                parent: None,
                subtree_end: index + 1,
            });
            if self.expanded[root_index].contains(root) {
                append(root, root_index, 1, Some(index), self, &mut rows);
            }
            rows[index].subtree_end = rows.len();
        }
        self.folded_ancestors.clear();
        if !self.show_all_ancestors {
            let (compact, folded) = compact_directory_chains(rows);
            rows = compact;
            self.folded_ancestors = folded;
        }
        self.selected = selected.and_then(|(root_index, path)| {
            rows.iter().position(|r| {
                r.root_index == root_index
                    && (r.entry.path == path
                        || self
                            .folded_ancestors
                            .get(&(root_index, r.entry.path.clone()))
                            .is_some_and(|paths| paths.contains(&path)))
            })
        });
        self.deepest_row = rows
            .iter()
            .enumerate()
            .max_by_key(|(_, row)| row.depth)
            .map(|(index, _)| index);
        self.rows = rows;
    }

    fn check_folder_media(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.remote.is_some() {
            return;
        }
        if self.media_presence.contains_key(&path) || self.media_tasks.contains_key(&path) {
            return;
        }
        let read_path = path.clone();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let read = cx.background_executor().spawn(async move {
            glim_services::preview::folder_has_media_cancellable(&read_path, &cancel)
                .unwrap_or(false)
        });
        let key = path.clone();
        let task = cx.spawn(async move |view, cx| {
            let _guard = guard;
            let present = read.await;
            let _ = view.update(cx, |v, cx| {
                v.media_tasks.remove(&path);
                v.media_presence.insert(path, present);
                cx.notify();
            });
        });
        self.media_tasks.insert(key, task);
    }

    fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(index).cloned() else {
            return;
        };
        self.pending_reveal = None;
        self.pending_reveal_root = None;
        self.preserve_next_reveal = None;
        self.selected = Some(index);
        window.focus(&self.focus, cx);
        if let Some(paths) = self
            .folded_ancestors
            .get(&(row.root_index, row.entry.path.clone()))
        {
            self.reveal_ancestors(row.root_index, paths[0].clone(), window, cx);
            return;
        }
        if !row.entry.is_dir {
            self.preserve_next_reveal = Some(row.entry.path.clone());
            cx.emit(ExplorerEvent::OpenFile(row.entry.path));
        } else {
            self.expansion_tasks.remove(&self.root_ids[row.root_index]);
            self.show_all_ancestors = false;
            self.pending_reveal = None;
            self.pending_reveal_root = None;
            if !self.expanded[row.root_index].remove(&row.entry.path) {
                self.expanded[row.root_index].insert(row.entry.path.clone());
                self.load_directory(row.entry.path, cx);
            }
            self.expansion_changed(cx);
        }
        self.rebuild();
        self.load_expanded(cx);
        cx.notify();
    }

    fn activate_path(
        &mut self,
        path: &std::path::Path,
        root_id: u64,
        sticky: bool,
        explicit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .rows
            .iter()
            .position(|row| self.root_ids[row.root_index] == root_id && row.entry.path == path)
        else {
            return;
        };
        self.clicked_path = Some(path.to_path_buf());
        self.clicked_root = Some(self.rows[index].root_index);
        cx.emit(ExplorerEvent::SelectionChanged);
        let depth = self.rows[index].depth;
        if !explicit && !self.rows[index].entry.is_dir {
            self.selected = Some(index);
            self.preserve_next_reveal = Some(path.to_path_buf());
            window.focus(&self.focus, cx);
            cx.emit(ExplorerEvent::OpenFile(path.to_path_buf()));
            cx.notify();
        } else {
            self.activate(index, window, cx);
        }
        if sticky {
            let Some(index) = self.selected else {
                return;
            };
            let capacity = sticky_capacity(
                f32::from(self.scroll.0.borrow().base_handle.bounds().size.height),
                row_height(cx),
            );
            self.scroll.scroll_to_item_strict_with_offset(
                index,
                ScrollStrategy::Top,
                depth.min(capacity),
            );
        }
    }

    fn render_row(
        &self,
        index: usize,
        sticky: bool,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row_height = row_height(cx);
        let row = &self.rows[index];
        if let Some(paths) = self
            .folded_ancestors
            .get(&(row.root_index, row.entry.path.clone()))
        {
            return self.render_ancestor_paths(row.root_index, paths, depth, sticky, cx);
        }
        let expanded = self.expanded[row.root_index].contains(&row.entry.path);
        let name = row
            .entry
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let label_name = name.clone();
        let root_index = row.root_index;
        let root_id = self.root_ids[root_index];
        let label_key = (root_index, row.entry.path.clone(), sticky);
        let label_view = cx.entity().downgrade();
        let hint_bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::default()));
        let painted_hint_bounds = hint_bounds.clone();
        let show_hint = self.clipped_labels.contains(&label_key);
        let tooltip_name = name.clone();
        let tooltip_path = row.entry.path.display().to_string();
        let row_path = row.entry.path.clone();
        let right_click_path = row_path.clone();
        let menu_path = row.entry.path.clone();
        let menu_is_dir = row.entry.is_dir;
        let menu_root_id = (row.depth == 0).then_some(self.root_ids[root_index]);
        let hover_path = menu_path.clone();

        let menu_root = self.roots[root_index].clone();
        let drag = DraggedEntry {
            path: row.entry.path.clone(),
            is_dir: row.entry.is_dir,
            storage: self.storage_key(),
        };
        let drop_path = row.entry.path.clone();
        let highlight_path = drop_path.clone();
        let accept_path = drop_path.clone();
        let accept_storage = self.storage_key();
        let highlight_storage = accept_storage.clone();
        let hover_target = drop_path.clone();
        let moving = crate::app::file_move::destination(cx) == Some(row.entry.path.as_path());
        let entry = div()
            .id(row.element_id(if sticky { "sticky-row" } else { "tree-row" }))
            .relative()
            .child(
                canvas(
                    move |bounds, _, _| {
                        painted_hint_bounds.set(bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .on_hover(cx.listener(move |view, hovered: &bool, window, cx| {
                if *hovered {
                    if show_hint {
                        view.path_hint.show(
                            &tooltip_name,
                            &tooltip_path,
                            menu_is_dir,
                            hint_bounds.get(),
                            window,
                        );
                    }
                    if menu_is_dir {
                        view.check_folder_media(hover_path.clone(), cx);
                    }
                } else {
                    view.path_hint.hide_for(&tooltip_path, hint_bounds.get());
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, _, window, cx| {
                    if menu_is_dir {
                        view.check_folder_media(right_click_path.clone(), cx);
                    }
                    view.clicked_path = Some(right_click_path.clone());
                    view.clicked_root = Some(root_index);
                    cx.emit(ExplorerEvent::SelectionChanged);
                    view.selected = view.rows.iter().position(|r| {
                        r.root_index == root_index && r.entry.path == right_click_path
                    });
                    window.focus(&view.focus, cx);
                    cx.notify();
                }),
            )
            .h(px(row_height))
            .w_full()
            .min_w(px(136. + depth as f32 * 14.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .bg(
                if self.clicked_root == Some(root_index)
                    && self.clicked_path.as_ref() == Some(&row.entry.path)
                {
                    super::selection_background(cx)
                } else {
                    super::workspace_background(cx)
                },
            )
            .when(sticky, |row| row.occlude())
            .when(
                row.depth > 0 && !crate::app::workspace_editors_locked(cx),
                |entry| {
                    entry.on_drag(drag, |drag, _, _, cx| {
                        cx.stop_propagation();
                        cx.new(|_| drag.clone())
                    })
                },
            )
            .when(row.entry.is_dir, |entry| {
                entry
                    .can_drop(move |value, _, cx| {
                        !crate::app::workspace_editors_locked(cx)
                            && value
                                .downcast_ref::<DraggedEntry>()
                                .is_some_and(|drag| drag.accepts(&accept_path, &accept_storage))
                    })
                    .drag_over::<DraggedEntry>(move |style, drag, _, cx| {
                        if !crate::app::workspace_editors_locked(cx)
                            && drag.accepts(&highlight_path, &highlight_storage)
                        {
                            style.bg(super::selection_background(cx))
                        } else {
                            style
                        }
                    })
                    .on_drag_move(cx.listener(
                        move |view, event: &DragMoveEvent<DraggedEntry>, window, cx| {
                            view.hover_move_folder(&hover_target, root_id, event, window, cx);
                        },
                    ))
                    .on_drop(cx.listener(move |view, drag: &DraggedEntry, _, cx| {
                        view.drag_hover = None;
                        view.drag_scroll = None;
                        if drag.accepts(&drop_path, &view.storage_key())
                            && !crate::app::workspace_editors_locked(cx)
                        {
                            cx.emit(ExplorerEvent::MoveEntry(
                                drag.path.clone(),
                                drop_path.clone(),
                            ));
                        }
                        cx.stop_propagation();
                    }))
            })
            .children((0..depth).map(|level| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(26. + level as f32 * 14.))
                    .w(px(1.))
                    .bg(cx.theme().border.opacity(0.5))
            }))
            .pl(px(if depth == 0 {
                0.
            } else {
                8. + depth as f32 * 14.
            }))
            .pr_2()
            .child(
                Button::new(row.element_id(if sticky { "sticky-entry" } else { "entry" }))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .hover(cx.theme().foreground.opacity(0.04))
                            .active(cx.theme().foreground.opacity(0.07)),
                    )
                    .text_size(px(12.))
                    .focus_ring(false)
                    .accessibility_label(format!(
                        "{}  {name}",
                        glim_services::locale::tr(if !row.entry.is_dir {
                            "File"
                        } else if self.tasks.contains_key(&row.entry.path) {
                            "Loading folder"
                        } else if expanded {
                            "Expanded folder"
                        } else {
                            "Collapsed folder"
                        })
                    ))
                    .child(
                        div().w_full().text_left().truncate().child(
                            gpui_kit::component::h_flex()
                                .w_full()
                                .min_w_0()
                                .gap_2()
                                .child(
                                    div()
                                        .w(px(16.))
                                        .h(px(16.))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(
                                            if row.entry.is_dir
                                                && (moving
                                                    || self.tasks.contains_key(&row.entry.path))
                                            {
                                                super::loading_icon(
                                                    row.element_id(if sticky {
                                                        "sticky-loading"
                                                    } else {
                                                        "loading"
                                                    }),
                                                    px(14.),
                                                    glim_services::locale::tr(if moving {
                                                        "Moving item"
                                                    } else {
                                                        "Loading folder"
                                                    }),
                                                    cx,
                                                )
                                            } else if row.entry.is_dir {
                                                // Keep the same icon box during loading; replacing
                                                // a font glyph with an ellipsis makes it flicker.
                                                Icon::new(if expanded {
                                                    gpui_kit::assets::IconName::ChevronDown
                                                } else {
                                                    gpui_kit::assets::IconName::ChevronRight
                                                })
                                                .size(px(14.))
                                                .flex_shrink_0()
                                                .text_color(cx.theme().muted_foreground)
                                                .into_any_element()
                                            } else {
                                                super::file_icons::file_icon(&row.entry.path, cx)
                                                    .into_any_element()
                                            },
                                        ),
                                )
                                .child(
                                    div()
                                        .id(row.element_id(if sticky {
                                            "sticky-label"
                                        } else {
                                            "label"
                                        }))
                                        .relative()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .child(name)
                                        .child(
                                            canvas(
                                                move |bounds, window, cx| {
                                                    let style = window.text_style();
                                                    let width = window
                                                        .text_system()
                                                        .layout_line(
                                                            &label_name,
                                                            style
                                                                .font_size
                                                                .to_pixels(window.rem_size()),
                                                            &[style.to_run(label_name.len())],
                                                            None,
                                                        )
                                                        .width;
                                                    let clipped =
                                                        width > bounds.size.width + px(0.5);
                                                    let key = label_key.clone();
                                                    let view = label_view.clone();
                                                    window.defer(cx, move |_, cx| {
                                                        let _ = view.update(cx, |view, cx| {
                                                            let changed = if clipped {
                                                                view.clipped_labels.insert(key)
                                                            } else {
                                                                view.clipped_labels.remove(&key)
                                                            };
                                                            if changed {
                                                                cx.notify();
                                                            }
                                                        });
                                                    });
                                                },
                                                |_, _, _, _| {},
                                            )
                                            .absolute()
                                            .size_full(),
                                        )
                                        .test_support(),
                                ),
                        ),
                    )
                    .flex_1()
                    .min_w_0()
                    .h(px(row_height))
                    .justify_start()
                    .on_click(cx.listener(move |view, event, window, cx| {
                        view.activate_path(
                            &row_path,
                            root_id,
                            sticky,
                            !matches!(event, ClickEvent::Mouse(_)) || event.click_count() > 1,
                            window,
                            cx,
                        );
                    })),
            )
            .when(row.depth == 0 && self.remote.is_none(), |entry| {
                entry.child(
                    gpui_kit::component::h_flex()
                        .gap_1()
                        .flex_shrink_0()
                        .children([true, false].into_iter().map(|expand| {
                            let label = glim_services::locale::tr(if expand {
                                "Expand all folders"
                            } else {
                                "Collapse all folders"
                            });
                            let id = match (sticky, expand) {
                                (true, true) => "sticky-expand-all-folders",
                                (true, false) => "sticky-collapse-all-folders",
                                (false, true) => "expand-all-folders",
                                (false, false) => "collapse-all-folders",
                            };
                            Button::new(row.element_id(id))
                                .custom(
                                    ButtonCustomVariant::new(cx)
                                        .hover(cx.theme().foreground.opacity(0.04))
                                        .active(cx.theme().foreground.opacity(0.07)),
                                )
                                .w(px(24.))
                                .h(px(24.))
                                .p_0()
                                .flex_shrink_0()
                                .text_color(cx.theme().muted_foreground)
                                .disabled(if expand {
                                    self.expansion_tasks.contains_key(&root_id)
                                } else {
                                    self.expanded[root_index].is_empty()
                                })
                                .icon(
                                    Icon::new(if expand {
                                        gpui_kit::assets::IconName::ChevronsDown
                                    } else {
                                        gpui_kit::assets::IconName::ChevronsUp
                                    })
                                    .size(px(16.)),
                                )
                                .tooltip(label.clone())
                                .accessibility_label(label)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    cx.stop_propagation();
                                    view.set_all_folders_expanded(root_id, expand, cx);
                                    window.focus(&view.focus, cx);
                                }))
                        })),
                )
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, _, window, cx| {
                    cx.stop_propagation();
                    view.show_file_menu(
                        menu_path.clone(),
                        menu_root.clone(),
                        menu_root_id,
                        menu_is_dir,
                        window,
                        cx,
                    );
                }),
            );

        div()
            .w_full()
            .child(entry)
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .into_any_element()
    }

    fn render_hidden_ancestors(&self, hidden: &[usize], cx: &mut Context<Self>) -> AnyElement {
        let root_index = self.rows[hidden[0]].root_index;
        let paths = hidden
            .iter()
            .flat_map(|&index| {
                let row = &self.rows[index];
                self.folded_ancestors
                    .get(&(root_index, row.entry.path.clone()))
                    .cloned()
                    .unwrap_or_else(|| vec![row.entry.path.clone()])
            })
            .collect::<Vec<_>>();
        self.render_ancestor_paths(root_index, &paths, 1, true, cx)
    }

    fn reveal_ancestors(
        &mut self,
        root_index: usize,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_all_ancestors = true;
        self.rebuild();
        if let Some(index) = self
            .rows
            .iter()
            .position(|row| row.root_index == root_index && row.entry.path == path)
        {
            self.selected = Some(index);
            self.scroll.scroll_to_item_strict_with_offset(
                index,
                ScrollStrategy::Top,
                self.rows[index].depth,
            );
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn render_ancestor_paths(
        &self,
        root_index: usize,
        paths: &[PathBuf],
        depth: usize,
        sticky: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let root_id = self.root_ids[root_index];
        let first_path = paths[0].clone();
        let id = ElementId::NamedChild(
            std::sync::Arc::new(entry_id(
                &first_path,
                if sticky {
                    "sticky-ancestors"
                } else {
                    "folded-ancestors"
                },
            )),
            root_id.to_string().into(),
        );
        let label = paths
            .iter()
            .map(|path| path.file_name().unwrap_or_default().to_string_lossy())
            .collect::<Vec<_>>()
            .join(" / ");
        div()
            .id(id.clone())
            .relative()
            .w_full()
            .min_w(px(136. + depth as f32 * 14.))
            .h(px(row_height(cx)))
            .pl(px(8. + depth as f32 * 14.))
            .pr_2()
            .rounded(px(6.))
            .bg(super::workspace_background(cx))
            .when(sticky, |row| row.occlude())
            .children((0..depth).map(|level| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(26. + level as f32 * 14.))
                    .w(px(1.))
                    .bg(cx.theme().border.opacity(0.5))
            }))
            .child(
                Button::new(ElementId::NamedChild(
                    std::sync::Arc::new(id),
                    "expand".into(),
                ))
                .custom(
                    ButtonCustomVariant::new(cx)
                        .hover(cx.theme().foreground.opacity(0.04))
                        .active(cx.theme().foreground.opacity(0.07)),
                )
                .w_full()
                .h(px(row_height(cx)))
                .text_size(px(12.))
                .justify_start()
                .text_color(cx.theme().muted_foreground)
                .accessibility_label(format!(
                    "{}: {label}",
                    glim_services::locale::tr("Show parent folders")
                ))
                .tooltip(label)
                .child(
                    div().w_full().text_left().child(
                        div()
                            .w(px(16.))
                            .h(px(16.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("…"),
                    ),
                )
                .on_click(cx.listener(move |view, _, window, cx| {
                    if let Some(root_index) = view.root_ids.iter().position(|id| *id == root_id) {
                        view.reveal_ancestors(root_index, first_path.clone(), window, cx);
                    }
                })),
            )
            .into_any_element()
    }

    fn show_file_menu(
        &mut self,
        path: PathBuf,
        root: PathBuf,
        root_id: Option<u64>,
        is_dir: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use FileMenuAction::*;
        let mut items = Vec::new();
        if is_dir {
            items.extend([
                (Some("New File"), Some(NewFile)),
                (Some("New Folder…"), Some(NewFolder)),
                (None, None),
            ]);
        }
        if is_dir && self.media_presence.get(&path).copied() == Some(true) {
            items.extend([
                (Some("Play in Order"), Some(Play(false))),
                (Some("Shuffle Play"), Some(Play(true))),
                (None, None),
            ]);
        } else if !is_dir && glim_services::preview::supports(&path) {
            items.extend([(Some("Open Preview / Play"), Some(Open)), (None, None)]);
        }
        items.extend([
            (Some("Reveal in Finder"), Some(Reveal)),
            (None, None),
            (Some("Copy Relative Path"), Some(CopyRelative)),
            (Some("Copy Absolute Path"), Some(CopyAbsolute)),
            (None, None),
        ]);
        if root_id.is_none() {
            if self.remote.is_some() {
                items.push((Some("Rename…"), Some(Rename)));
            }
            items.push((Some("Move to Folder…"), Some(Move)));
        }
        items.push((Some("Move to Trash…"), Some(Trash)));
        if let Some(id) = root_id {
            items.extend([
                (None, None),
                (Some("Remove Folder from Workspace"), Some(RemoveFolder(id))),
            ]);
        }
        if self.remote.is_some() {
            items.retain(|(_, action)| !matches!(action, Some(Play(_) | Reveal)));
            items.dedup_by(|left, right| left.0.is_none() && right.0.is_none());
            if items.first().is_some_and(|item| item.0.is_none()) {
                items.remove(0);
            }
        }
        let titles = items
            .iter()
            .map(|(label, action)| {
                (
                    label.map(glim_services::locale::tr),
                    !matches!(action, Some(Move)) || !crate::app::workspace_editors_locked(cx),
                )
            })
            .collect::<Vec<_>>();
        self.context_menu_task = None;
        let Some(menu) = super::native_menu_bridge::NativeMenu::new(&titles, window) else {
            return;
        };
        self.context_menu_task = Some(cx.spawn_in(window, async move |view, cx| {
            let selected = menu.choose(cx).await;
            let _ = view.update_in(cx, |_, _, cx| {
                if let Some(action) = selected
                    .and_then(|index| items.get(index))
                    .and_then(|(_, action)| *action)
                {
                    let action_name = match action {
                        NewFile => "new_file",
                        NewFolder => "new_folder",
                        Rename => "rename",
                        Play(true) => "play_folder_shuffle",
                        Play(false) => "play_folder",
                        Open => "open",
                        Reveal => "reveal_in_finder",
                        CopyRelative => "copy_relative_path",
                        CopyAbsolute => "copy_absolute_path",
                        Trash => "trash",
                        Move => "move",
                        RemoveFolder(_) => "remove_folder",
                    };
                    glim_services::telemetry::file_event(
                        "explorer_action",
                        &path,
                        [
                            ("action", glim_services::telemetry::text(action_name)),
                            ("is_directory", glim_services::telemetry::flag(is_dir)),
                        ],
                    );
                    match action {
                        NewFile => cx.emit(ExplorerEvent::NewFile(path.clone())),
                        NewFolder => cx.emit(ExplorerEvent::NewFolder(path.clone())),
                        Rename => cx.emit(ExplorerEvent::RenameEntry(path.clone())),
                        Play(shuffle) => cx.emit(ExplorerEvent::PlayFolder(path.clone(), shuffle)),
                        Open => cx.emit(ExplorerEvent::OpenFile(path.clone())),
                        Reveal => cx.reveal_path(&path),
                        CopyRelative => {
                            let relative = path.strip_prefix(&root).unwrap_or(&path);
                            let text = if relative.as_os_str().is_empty() {
                                ".".to_owned()
                            } else {
                                relative.to_string_lossy().into_owned()
                            };
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                        }
                        CopyAbsolute => cx.write_to_clipboard(ClipboardItem::new_string(
                            path.to_string_lossy().into_owned(),
                        )),
                        Trash => cx.emit(ExplorerEvent::TrashEntry(path.clone())),
                        Move => cx.emit(ExplorerEvent::ChooseMoveEntry(path.clone())),
                        RemoveFolder(id) => cx.emit(ExplorerEvent::RemoveFolder(id)),
                    }
                }
            });
        }));
    }

    fn show_add_folder_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.context_menu_task = None;
        let items = [(
            Some(glim_services::locale::tr("Add Folder to Workspace…")),
            true,
        )];
        let Some(menu) = super::native_menu_bridge::NativeMenu::new(&items, window) else {
            return;
        };
        self.context_menu_task = Some(cx.spawn_in(window, async move |view, cx| {
            if menu.choose(cx).await == Some(0) {
                let _ = view.update_in(cx, |_, window, cx| {
                    window.defer(cx, |window, cx| {
                        window.dispatch_action(Box::new(crate::app::actions::AddFolder), cx)
                    });
                });
            }
        }));
    }

    fn move_selection(&mut self, down: bool, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let selected = match self.selected {
            Some(i) if down => (i + 1).min(self.rows.len() - 1),
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.selected = Some(selected);
        self.scroll
            .scroll_to_item(selected, ScrollStrategy::Nearest);
        cx.notify();
    }
}

impl Render for Explorer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let row_height = row_height(cx);
        let view = cx.entity().downgrade();
        gpui_kit::component::v_flex()
            .relative()
            .overflow_hidden()
            .size_full()
            .min_h_0()
            .key_context("Explorer")
            .track_focus(&self.focus)
            .on_drag_move(
                cx.listener(|view, _: &DragMoveEvent<DraggedEntry>, window, cx| {
                    view.start_move_scroll(window, cx);
                }),
            )
            .on_action(cx.listener(|view, _: &Up, _, cx| view.move_selection(false, cx)))
            .on_action(cx.listener(|view, _: &Down, _, cx| view.move_selection(true, cx)))
            .on_action(cx.listener(|view, _: &OpenSelected, window, cx| {
                if let Some(index) = view.selected {
                    view.activate(index, window, cx);
                }
            }))
            .on_action(cx.listener(|view, _: &MoveSelected, _, cx| {
                if view.remote.is_none()
                    && !crate::app::workspace_editors_locked(cx)
                    && let Some(row) = view.selected.and_then(|index| view.rows.get(index))
                    && row.depth > 0
                {
                    cx.emit(ExplorerEvent::ChooseMoveEntry(row.entry.path.clone()));
                }
            }))
            .on_action(cx.listener(|view, _: &Expand, window, cx| {
                if let Some(index) = view.selected
                    && let Some(row) = view.rows.get(index)
                    && row.entry.is_dir
                    && (!view.expanded[row.root_index].contains(&row.entry.path)
                        || view
                            .folded_ancestors
                            .contains_key(&(row.root_index, row.entry.path.clone())))
                {
                    view.activate(index, window, cx);
                }
            }))
            .on_action(cx.listener(move |view, _: &Collapse, window, cx| {
                if let Some(index) = view.selected
                    && let Some(row) = view.rows.get(index)
                {
                    if view.expanded[row.root_index].contains(&row.entry.path) {
                        view.activate(index, window, cx);
                    } else if let Some(parent) = row.parent {
                        view.selected = Some(parent);
                        let capacity = sticky_capacity(
                            f32::from(view.scroll.0.borrow().base_handle.bounds().size.height),
                            row_height,
                        );
                        view.scroll.scroll_to_item_with_offset(
                            parent,
                            ScrollStrategy::Nearest,
                            view.rows[parent].depth.min(capacity),
                        );
                        cx.notify();
                    }
                }
            }))
            .when(self.rows.is_empty(), |view| {
                view.child(
                    div()
                        .p_4()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(glim_services::locale::tr("This folder is empty.")),
                )
            })
            .child(
                uniform_list(
                    "files",
                    self.rows.len(),
                    cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                        let scroll = &view.scroll.0.borrow().base_handle;
                        let pinned = sticky_rows_with_height(
                            &view.rows,
                            -f32::from(scroll.offset().y),
                            f32::from(scroll.bounds().size.height),
                            row_height,
                            view.show_all_ancestors,
                        );
                        range
                            .map(|index| {
                                view.render_row(
                                    index,
                                    false,
                                    pinned.display_depth(index, &view.rows),
                                    cx,
                                )
                            })
                            .collect()
                    }),
                )
                .w_full()
                .flex_1()
                .min_h_0()
                .with_width_from_item(self.deepest_row)
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .track_scroll(&self.scroll),
            )
            .child(
                canvas(
                    move |bounds, window, cx| {
                        // Layout the pinned rows after the list, using this frame's actual
                        // scroll position. They overlay the list without changing its height.
                        let (mut pinned, blank_top, horizontal_offset) = view
                            .update(cx, |view, cx| {
                                let offset =
                                    -f32::from(view.scroll.0.borrow().base_handle.offset().y);
                                let blank_top =
                                    (view.rows.len() as f32 * row_height - offset).max(0.);
                                let pinned = sticky_rows_with_height(
                                    &view.rows,
                                    offset,
                                    f32::from(bounds.size.height),
                                    row_height,
                                    view.show_all_ancestors,
                                );
                                let elements = pinned
                                    .items
                                    .iter()
                                    .map(|row| {
                                        let element = if let Some(index) = row.index {
                                            view.render_row(index, true, row.depth, cx)
                                        } else {
                                            view.render_hidden_ancestors(&pinned.hidden, cx)
                                        };
                                        (row.y, element)
                                    })
                                    .collect::<Vec<_>>();
                                let horizontal_offset =
                                    view.scroll.0.borrow().base_handle.offset().x;
                                (elements, blank_top, horizontal_offset)
                            })
                            .unwrap_or_default();
                        // A separate hit area below the final row: never an ancestor
                        // of file/folder menus, so both menus cannot open together.
                        let blank_view = view.clone();
                        let mut blank = if blank_top < f32::from(bounds.size.height) {
                            let mut element = div()
                                .id("files-blank-space")
                                .w(bounds.size.width)
                                .h(bounds.size.height - px(blank_top))
                                .on_mouse_down(MouseButton::Right, move |_, window, cx| {
                                    cx.stop_propagation();
                                    let _ = blank_view.update(cx, |view, cx| {
                                        view.show_add_folder_menu(window, cx)
                                    });
                                })
                                .into_any_element();
                            element.prepaint_as_root(
                                bounds.origin + point(px(0.), px(blank_top)),
                                size(bounds.size.width, bounds.size.height - px(blank_top)).into(),
                                window,
                                cx,
                            );
                            Some(element)
                        } else {
                            None
                        };
                        for (y, element) in &mut pinned {
                            element.prepaint_as_root(
                                bounds.origin + point(horizontal_offset, px(*y)),
                                size(bounds.size.width - horizontal_offset, px(row_height)).into(),
                                window,
                                cx,
                            );
                        }
                        (pinned, blank.take())
                    },
                    |_, (pinned, blank), window, cx| {
                        if let Some(mut blank) = blank {
                            blank.paint(window, cx);
                        }
                        // A departing child folder slides behind its pinned parent.
                        for (_, mut element) in pinned.into_iter().rev() {
                            element.paint(window, cx);
                        }
                    },
                )
                .absolute()
                .inset_0()
                .size_full(),
            )
    }
}

fn entry_id(path: &std::path::Path, part: &'static str) -> ElementId {
    ElementId::NamedChild(
        std::sync::Arc::new(ElementId::Path(path.into())),
        part.into(),
    )
}

fn row_height(_: &App) -> f32 {
    28.
}

/// Fold only fully loaded, unbranched expanded chains. Branches, files and roots
/// remain explicit; the representative row keeps the hidden paths for disclosure.
fn compact_directory_chains(rows: Vec<Row>) -> (Vec<Row>, FoldedAncestors) {
    let mut keep = vec![true; rows.len()];
    let mut folded = HashMap::new();
    let mut start = 0;
    while start < rows.len() {
        if rows[start].depth == 0 || !rows[start].entry.is_dir || rows[start].entry.is_symlink {
            start += 1;
            continue;
        }
        let mut end = start;
        while end + 1 < rows.len()
            && rows[end + 1].parent == Some(end)
            && rows[end + 1].entry.is_dir
            && !rows[end + 1].entry.is_symlink
            && rows[end + 1].subtree_end > end + 2
            && rows[end + 1].subtree_end == rows[end].subtree_end
        {
            end += 1;
        }
        // Keep the two nearest directories and save at least two rows.
        if end - start + 1 > MAX_ANCESTOR_ROWS {
            let representative = end - 2;
            keep[start..representative].fill(false);
            folded.insert(
                (
                    rows[start].root_index,
                    rows[representative].entry.path.clone(),
                ),
                rows[start..=representative]
                    .iter()
                    .map(|row| row.entry.path.clone())
                    .collect(),
            );
        }
        start = end + 1;
    }
    let mut prefix = Vec::with_capacity(rows.len() + 1);
    prefix.push(0);
    for &retained in &keep {
        prefix.push(prefix.last().unwrap() + usize::from(retained));
    }
    let mut compact: Vec<Row> = Vec::with_capacity(*prefix.last().unwrap());
    for (index, row) in rows.iter().enumerate() {
        if !keep[index] {
            continue;
        }
        let mut parent = row.parent;
        while let Some(index) = parent {
            if keep[index] {
                break;
            }
            parent = rows[index].parent;
        }
        let parent = parent.map(|index| prefix[index]);
        compact.push(Row {
            parent,
            depth: parent.map_or(0, |index| compact[index].depth + 1),
            subtree_end: prefix[row.subtree_end],
            ..row.clone()
        });
    }
    (compact, folded)
}

#[cfg(test)]
fn sticky_rows(rows: &[Row], offset: f32, height: f32) -> Vec<(usize, f32)> {
    sticky_rows_with_height(rows, offset, height, 32., false)
        .items
        .into_iter()
        .filter_map(|row| row.index.map(|index| (index, row.y)))
        .collect()
}

fn sticky_capacity(height: f32, row_height: f32) -> usize {
    // Leave at least half the viewport for the directory contents.
    ((height / row_height) as usize / 2).min(MAX_ANCESTOR_ROWS)
}

/// Compress only ancestors crossing the compact overlay, so distant descendants
/// cannot become pinned before they reach the visible list's top edge.
fn sticky_rows_with_height(
    rows: &[Row],
    offset: f32,
    height: f32,
    row_height: f32,
    show_all: bool,
) -> StickyRows {
    let offset = offset.max(0.);
    let top = (offset / row_height).floor() as usize;
    let mut pinned = StickyRows::default();
    let mut pinned_path = Vec::new();
    let visible = (height / row_height) as usize;
    for slot in 0..visible.saturating_sub(1) {
        let slot_y = slot as f32 * row_height;
        let probe = top.saturating_add(((offset % row_height + slot_y) / row_height) as usize);
        let Some(row) = rows.get(probe) else {
            break;
        };
        let mut ancestors = Vec::new();
        let mut current = if row.entry.is_dir && row.subtree_end > probe + 1 {
            Some(probe)
        } else {
            row.parent
        };
        while let Some(index) = current {
            ancestors.push(index);
            current = rows[index].parent;
        }
        ancestors.reverse();
        let capacity = if show_all {
            visible.saturating_sub(1)
        } else {
            sticky_capacity(height, row_height)
        };
        if !ancestors.starts_with(&pinned_path) {
            break;
        }
        loop {
            let mut candidate = StickyRows::default();
            let indices = if !show_all && ancestors.len() > capacity && capacity >= 3 {
                let tail = ancestors.len() - (capacity - 2);
                candidate.hidden = ancestors[1..tail].to_vec();
                std::iter::once(Some(ancestors[0]))
                    .chain(std::iter::once(None))
                    .chain(ancestors[tail..].iter().copied().map(Some))
                    .collect::<Vec<_>>()
            } else {
                ancestors[ancestors.len().saturating_sub(capacity)..]
                    .iter()
                    .copied()
                    .map(Some)
                    .collect::<Vec<_>>()
            };
            let mut truncate = None;
            for (depth, entry) in indices.into_iter().enumerate() {
                let index = entry.unwrap_or_else(|| *candidate.hidden.last().unwrap());
                let y = depth as f32 * row_height;
                let natural_y = index as f32 * row_height - offset;
                let end_y = rows[index].subtree_end as f32 * row_height - offset;
                if natural_y > y || (natural_y == y && candidate.hidden.is_empty()) || end_y <= y {
                    truncate = ancestors.iter().position(|&ancestor| ancestor == index);
                    break;
                }
                candidate.items.push(StickyRow {
                    index: entry,
                    depth,
                    y: y.min(end_y - row_height),
                });
            }
            if let Some(length) = truncate {
                ancestors.truncate(length);
            } else {
                pinned = candidate;
                pinned_path = ancestors;
                break;
            }
        }
    }
    pinned
}

#[cfg(test)]
mod tests {
    use super::{Row, compact_directory_chains, sticky_capacity, sticky_rows};
    use glim_core::DirectoryEntry;

    fn row(name: &str, parent: Option<usize>, depth: usize, end: usize, is_dir: bool) -> Row {
        Row {
            root_index: 0,
            entry: DirectoryEntry {
                path: name.into(),
                is_dir,
                is_symlink: false,
            },
            parent,
            depth,
            subtree_end: end,
        }
    }
    fn tree() -> Vec<Row> {
        vec![
            row("a", None, 0, 8, true),
            row("a/b", Some(0), 1, 6, true),
            row("a/b/c", Some(1), 2, 6, true),
            row("a/b/c/one", Some(2), 3, 4, false),
            row("a/b/c/two", Some(2), 3, 5, false),
            row("a/b/c/three", Some(2), 3, 6, false),
            row("a/z", Some(0), 1, 8, true),
            row("a/z/file", Some(6), 2, 8, false),
            row("other", None, 0, 10, true),
            row("other/file", Some(8), 1, 10, false),
        ]
    }

    fn deep_chain() -> Vec<Row> {
        let mut rows = (0usize..7)
            .map(|index| {
                row(
                    &format!(
                        "/{}",
                        (0..=index)
                            .map(|n| format!("dir{n}"))
                            .collect::<Vec<_>>()
                            .join("/")
                    ),
                    index.checked_sub(1),
                    index,
                    9,
                    true,
                )
            })
            .collect::<Vec<_>>();
        rows.push(row(
            "/dir0/dir1/dir2/dir3/dir4/dir5/dir6/one",
            Some(6),
            7,
            8,
            false,
        ));
        rows.push(row(
            "/dir0/dir1/dir2/dir3/dir4/dir5/dir6/two",
            Some(6),
            7,
            9,
            false,
        ));
        rows
    }

    #[test]
    fn seven_directory_levels_compact_before_scrolling_and_keep_both_files() {
        let source = deep_chain();
        let (rows, folded) = compact_directory_chains(source.clone());
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].entry.path, source[0].entry.path);
        assert_eq!(
            folded[&(0, source[4].entry.path.clone())],
            source[1..=4]
                .iter()
                .map(|r| r.entry.path.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(rows[1].parent, Some(0));
        assert_eq!(rows[1].subtree_end, 6);
        assert_eq!(rows[4].parent, Some(3));
        assert_eq!(rows[4].depth, 4);
        assert_eq!(rows[4].entry.path, source[7].entry.path);
        assert_eq!(rows[5].entry.path, source[8].entry.path);
    }

    #[test]
    fn compaction_preserves_branching_directories_siblings_and_symlink_boundaries() {
        let mut source = deep_chain();
        source[0].subtree_end = 10;
        source[1].subtree_end = 10;
        source.push(row("/dir0/dir1/sibling", Some(1), 2, 10, false));
        let (rows, _) = compact_directory_chains(source.clone());
        assert!(rows.iter().any(|r| r.entry.path == source[1].entry.path));
        assert_eq!(rows.last().unwrap().parent, Some(1));
        assert_eq!(rows.iter().filter(|r| !r.entry.is_dir).count(), 3);
        source[4].entry.is_symlink = true;
        let (rows, folded) = compact_directory_chains(source.clone());
        assert!(folded.is_empty());
        assert_eq!(rows.len(), source.len());
    }

    #[test]
    fn repeated_roots_compact_independently_and_rebase_parent_indices() {
        let source = deep_chain();
        let mut combined = source.clone();
        combined.extend(source.iter().cloned().map(|mut r| {
            r.root_index = 1;
            r.parent = r.parent.map(|index| index + source.len());
            r.subtree_end += source.len();
            r
        }));
        let (rows, folded) = compact_directory_chains(combined);
        assert_eq!(folded.len(), 2);
        assert_eq!(rows[6].parent, None);
        assert_eq!(rows[7].parent, Some(6));
        assert_eq!(rows[6].subtree_end, 12);
    }

    #[test]
    fn ancestor_rows_stay_bounded_in_tall_and_short_views() {
        assert_eq!(sticky_capacity(1200., 28.), 4);
        assert_eq!(sticky_capacity(112., 28.), 2);
    }
    #[test]
    fn ancestors_pin_only_after_scrolling_and_keep_tree_order() {
        let rows = tree();
        assert!(sticky_rows(&rows, 0., 320.).is_empty());
        assert_eq!(sticky_rows(&rows, 8., 320.), [(0, 0.), (1, 32.), (2, 64.)]);
        assert_eq!(sticky_rows(&rows, 96., 320.), [(0, 0.), (1, 32.), (2, 64.)]);
        assert!(sticky_rows(&rows, 96., 32.).is_empty());
    }
    #[test]
    fn sibling_branches_push_out_old_ancestors() {
        let rows = tree();
        assert_eq!(
            sticky_rows(&rows, 120., 320.),
            [(0, 0.), (1, 32.), (2, 40.)]
        );
        assert_eq!(sticky_rows(&rows, 176., 320.), [(0, 0.), (6, 32.)]);
        assert_eq!(sticky_rows(&rows, 240., 320.), [(0, -16.)]);
        assert!(sticky_rows(&rows, 256., 320.).is_empty());
        assert_eq!(sticky_rows(&rows, 264., 320.), [(8, 0.)]);
    }
    #[test]
    fn collapsed_or_empty_folders_do_not_stick() {
        let rows = vec![
            row("empty", None, 0, 1, true),
            row("next", None, 0, 2, false),
        ];
        assert!(sticky_rows(&rows, 8., 320.).is_empty());
        assert!(sticky_rows(&[], 100., 320.).is_empty());
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::{Explorer, entry_id};
    use glim_core::DirectoryEntry;
    use gpui_kit::{
        AppContext, Bounds, Point, TestAppContext, WindowBounds, WindowOptions, px, size,
        test::TestWindowExt,
    };

    // These interaction fixtures need visible children; production roots start collapsed.
    fn expanded_fixture(
        root: std::path::PathBuf,
        entries: Vec<DirectoryEntry>,
        cx: &mut gpui_kit::Context<Explorer>,
    ) -> Explorer {
        let mut view = Explorer::new(root.clone(), entries, cx);
        view.expanded[0].insert(root);
        view.rebuild();
        view
    }

    #[gpui_kit::test]
    fn reopening_cached_ancestors_restores_unloaded_expanded_descendants(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let child = root.join("child");
        let nested = child.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let file = nested.join("file.txt");
        std::fs::write(&file, "contents").unwrap();
        let entries = glim_services::workspace::list_directory_cancellable(
            &root,
            &glim_services::work::Cancellation::default(),
        )
        .unwrap();
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|cx| Explorer::new(root.clone(), entries, cx))
            })
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.restore_expanded(
                    std::collections::BTreeMap::from([(
                        root.clone(),
                        vec![child.clone(), nested.clone()],
                    )]),
                    cx,
                );
                assert!(view.tasks.is_empty());
                assert_eq!(view.rows.len(), 1);
                view.activate(0, window, cx);
                assert!(view.tasks.contains_key(&child));
            });
        })
        .unwrap();
        for _ in 0..4 {
            cx.run_until_parked();
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(20));
        }
        cx.run_until_parked();
        cx.update(|cx| {
            let view = view.read(cx);
            assert_eq!(view.rows.len(), 4);
            assert!(view.rows.iter().any(|row| row.entry.path == file));
            assert!(view.tasks.is_empty());
        });
    }

    #[gpui_kit::test]
    fn large_restore_only_reads_discovered_directories_and_bounds_scans(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let mut saved = vec![root.clone()];
        for i in 0..128 {
            let child = root.join(format!("folder-{i:04}"));
            std::fs::create_dir(&child).unwrap();
            std::fs::write(child.join("file.txt"), "contents").unwrap();
            saved.push(child);
        }
        saved.extend((0..10_000).map(|i| root.join(format!("missing/old-{i}"))));
        let entries = glim_services::workspace::list_directory_cancellable(
            &root,
            &glim_services::work::Cancellation::default(),
        )
        .unwrap();
        let view = cx.update(|cx| {
            let view = cx.new(|cx| Explorer::new(root.clone(), entries, cx));
            view.update(cx, |view, cx| {
                view.restore_expanded(
                    std::collections::BTreeMap::from([(root.clone(), saved)]),
                    cx,
                );
                view.refresh(cx);
                assert!(view.tasks.len() <= super::DIRECTORY_SCAN_CONCURRENCY);
                assert!(view.pending_refresh.len() <= 1);
            });
            view
        });
        for _ in 0..12 {
            cx.run_until_parked();
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(20));
        }
        cx.run_until_parked();
        cx.update(|cx| {
            let view = view.read(cx);
            assert_eq!(view.directories.len(), 129);
            assert_eq!(view.rows.len(), 257);
            assert_eq!(view.expanded[0].len(), 10_129);
            assert!(view.tasks.is_empty());
            assert!(view.pending_refresh.is_empty());
            assert!(view.directory_update.is_none());
        });
    }

    #[gpui_kit::test]
    fn expand_all_batches_wide_trees_and_remains_cancellable(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        for i in 0..10_000 {
            let child = root.join(format!("folder-{i:04}"));
            std::fs::create_dir(&child).unwrap();
            std::fs::write(child.join("file.txt"), "contents").unwrap();
        }
        let entries = glim_services::workspace::list_directory_cancellable(
            &root,
            &glim_services::work::Cancellation::default(),
        )
        .unwrap();
        let view = cx.update(|cx| {
            let view = cx.new(|cx| Explorer::new(root, entries, cx));
            view.update(cx, |view, cx| {
                view.set_all_folders_expanded(0, true, cx);
            });
            view
        });
        for _ in 0..800 {
            cx.run_until_parked();
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(20));
        }
        cx.run_until_parked();
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                assert_eq!(view.expanded[0].len(), 10_001);
                assert_eq!(view.rows.len(), 20_001);
                assert!(view.expansion_tasks.is_empty());
                view.set_all_folders_expanded(0, false, cx);
                view.set_all_folders_expanded(0, true, cx);
                view.set_all_folders_expanded(0, false, cx);
            })
        });
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(20));
        cx.run_until_parked();
        cx.update(|cx| {
            let view = view.read(cx);
            assert!(view.expanded[0].is_empty());
            assert_eq!(view.rows.len(), 1);
            assert!(view.expansion_tasks.is_empty());
        });
    }

    #[gpui_kit::test]
    fn a_directory_with_ten_thousand_files_renders_and_keeps_selection(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let started = std::time::Instant::now();
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|cx| {
                    Explorer::new(
                        "/fixture".into(),
                        (0..10_000)
                            .map(|i| DirectoryEntry {
                                path: format!("/fixture/file-{i:05}.txt").into(),
                                is_dir: false,
                                is_symlink: false,
                            })
                            .collect(),
                        cx,
                    )
                })
            })
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            view.update(cx, |view, cx| {
                view.reveal_path("/fixture/file-09999.txt".into(), cx)
            });
            window.render_frame(cx);
            let view = view.read(cx);
            assert_eq!(view.rows.len(), 10_001);
            assert_eq!(view.selected, Some(10_000));
            assert!(view.tasks.is_empty());
        })
        .unwrap();
        eprintln!(
            "10,000-file tree construction, two frames and selection: {:?}",
            started.elapsed()
        );
    }

    #[gpui_kit::test]
    fn new_roots_expand_one_level_and_navigation_cancels_pending_reveal(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            let view = cx.new(|cx| {
                Explorer::new(
                    "/fixture".into(),
                    vec![
                        DirectoryEntry {
                            path: "/fixture/nested".into(),
                            is_dir: true,
                            is_symlink: false,
                        },
                        DirectoryEntry {
                            path: "/fixture/file.txt".into(),
                            is_dir: false,
                            is_symlink: false,
                        },
                    ],
                    cx,
                )
            });
            view.update(cx, |view, cx| {
                assert_eq!(view.rows.len(), 3);
                assert!(view.rows.iter().all(|row| row.depth <= 1));
                assert_eq!(view.expanded[0].len(), 1);
                assert!(view.expanded[0].contains(std::path::Path::new("/fixture")));
                view.add_folder(
                    "/another".into(),
                    vec![DirectoryEntry {
                        path: "/another/nested".into(),
                        is_dir: true,
                        is_symlink: false,
                    }],
                    cx,
                );
                assert_eq!(view.rows.len(), 5);
                assert_eq!(view.expanded[1].len(), 1);
                assert!(view.expanded[1].contains(std::path::Path::new("/another")));
                assert!(view.rows.iter().all(|row| row.depth <= 1));
                view.pending_reveal = Some("/fixture/file.txt".into());
                view.preserve_next_reveal = Some("/fixture/file.txt".into());
                view.clear_clicked_selection(cx);
                assert!(view.pending_reveal.is_none());
                assert!(view.preserve_next_reveal.is_none());
            });
        });
    }

    #[gpui_kit::test]
    fn file_tree_click_keeps_scroll_but_tab_reveal_moves_it(cx: &mut TestAppContext) {
        let target = std::path::PathBuf::from("/fixture/file-35.txt");
        cx.update(gpui_kit::init);
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::default(),
                        size: size(px(400.), px(500.)),
                    })),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    cx.new(|cx| {
                        expanded_fixture(
                            "/fixture".into(),
                            (0..60)
                                .map(|i| DirectoryEntry {
                                    path: format!("/fixture/file-{i}.txt").into(),
                                    is_dir: false,
                                    is_symlink: false,
                                })
                                .collect(),
                            cx,
                        )
                    })
                },
            )
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            view.update(cx, |view, cx| {
                view.scroll
                    .0
                    .borrow()
                    .base_handle
                    .set_offset(gpui_kit::point(px(0.), px(-700.)));
                cx.notify();
            });
            window.render_frame(cx);
            let before = view.read(cx).scroll.0.borrow().base_handle.offset();
            window.click(entry_id(&target, "entry"), cx);
            view.update(cx, |view, cx| view.reveal_path(target.clone(), cx));
            window.render_frame(cx);
            assert_eq!(view.read(cx).scroll.0.borrow().base_handle.offset(), before);
            assert_eq!(view.read(cx).clicked_path.as_ref(), Some(&target));
            let other = std::path::PathBuf::from("/fixture/file-1.txt");
            view.update(cx, |view, cx| view.reveal_path(other.clone(), cx));
            window.render_frame(cx);
            assert_ne!(view.read(cx).scroll.0.borrow().base_handle.offset(), before);
            assert_eq!(view.read(cx).clicked_path.as_ref(), Some(&other));
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn repeated_and_nested_roots_keep_independent_tree_state(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            let root = std::path::PathBuf::from("/fixture");
            let child = root.join("child");
            let entries = vec![DirectoryEntry {
                path: child.clone(),
                is_dir: true,
                is_symlink: false,
            }];
            let view = cx.new(|cx| Explorer::new(root.clone(), entries.clone(), cx));
            view.update(cx, |view, cx| {
                view.expanded[0].insert(root.clone());
                view.rebuild();
                view.add_folder(child.clone(), Vec::new(), cx);
                view.add_folder(root.clone(), entries, cx);
                assert_eq!(view.roots, vec![root.clone(), child, root.clone()]);
                let duplicate = view.rows.iter().position(|r| r.root_index == 2).unwrap();
                assert_ne!(
                    view.rows[0].element_id("entry"),
                    view.rows[duplicate].element_id("entry"),
                );
                view.selected = Some(duplicate);
                view.expanded[0].remove(&root);
                view.rebuild();
                assert!(view.expanded[2].contains(&root));
                assert_eq!(view.rows[view.selected.unwrap()].root_index, 2);
                assert_eq!(view.rows.iter().filter(|r| r.depth == 0).count(), 3);
                assert!(view.rows.iter().any(|r| r.root_index == 2 && r.depth == 1));
                assert!(!view.rows.iter().any(|r| r.root_index == 0 && r.depth == 1));
            });
        });
    }

    #[gpui_kit::test]
    fn only_clipped_file_labels_enable_tooltips(cx: &mut TestAppContext) {
        let short = std::path::PathBuf::from("/fixture/short.txt");
        let long =
            std::path::PathBuf::from(format!("/fixture/{}.txt", "long-filename-".repeat(12)));
        cx.update(gpui_kit::init);
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::default(),
                        size: size(px(300.), px(300.)),
                    })),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    cx.new(|cx| {
                        expanded_fixture(
                            "/fixture".into(),
                            vec![short.clone(), long.clone()]
                                .into_iter()
                                .map(|path| DirectoryEntry {
                                    path,
                                    is_dir: false,
                                    is_symlink: false,
                                })
                                .collect(),
                            cx,
                        )
                    })
                },
            )
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            let labels = &view.read(cx).clipped_labels;
            assert!(!labels.contains(&(0, short.clone(), false)));
            assert!(labels.contains(&(0, long.clone(), false)));
        });
    }

    #[gpui_kit::test]
    fn loading_arrow_and_expanded_arrow_keep_label_in_place(cx: &mut TestAppContext) {
        let path = std::path::PathBuf::from("/fixture/folder");
        cx.update(gpui_kit::init);
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::default(),
                        size: size(px(400.), px(500.)),
                    })),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    cx.new(|cx| {
                        expanded_fixture(
                            "/fixture".into(),
                            vec![DirectoryEntry {
                                path: path.clone(),
                                is_dir: true,
                                is_symlink: false,
                            }],
                            cx,
                        )
                    })
                },
            )
            .unwrap()
        });
        let before = cx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.find(entry_id(&path, "label")).bounds()
            })
            .unwrap();
        cx.update(|cx| {
            view.update(cx, |v, cx| {
                v.tasks.insert(
                    path.clone(),
                    cx.spawn(async |_, _| std::future::pending::<()>().await),
                );
                v.expanded[0].insert(path.clone());
                cx.notify();
            })
        });
        let loading = cx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.find(entry_id(&path, "label")).bounds()
            })
            .unwrap();
        cx.update(|cx| {
            view.update(cx, |v, cx| {
                v.tasks.clear();
                cx.notify();
            })
        });
        let expanded = cx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.find(entry_id(&path, "label")).bounds()
            })
            .unwrap();
        assert_eq!(
            before, loading,
            "loading marker must not shift the filename"
        );
        assert_eq!(
            before, expanded,
            "disclosure state must not shift the filename"
        );
    }
    #[gpui_kit::test]
    fn clicking_sticky_folder_keeps_it_below_pinned_parent(cx: &mut TestAppContext) {
        let parent = std::path::PathBuf::from("/fixture/parent");
        let branch = parent.join("branch");
        let file = |path| DirectoryEntry {
            path,
            is_dir: false,
            is_symlink: false,
        };
        cx.update(gpui_kit::init);
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::default(),
                        size: size(px(400.), px(500.)),
                    })),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    cx.new(|cx| {
                        let mut view = expanded_fixture(
                            "/fixture".into(),
                            vec![DirectoryEntry {
                                path: parent.clone(),
                                is_dir: true,
                                is_symlink: false,
                            }],
                            cx,
                        );
                        let mut siblings = (0..30)
                            .map(|i| file(parent.join(format!("before-{i}"))))
                            .collect::<Vec<_>>();
                        siblings.push(DirectoryEntry {
                            path: branch.clone(),
                            is_dir: true,
                            is_symlink: false,
                        });
                        siblings.extend((0..100).map(|i| file(parent.join(format!("after-{i}")))));
                        view.directories.insert(parent.clone(), siblings);
                        view.directories.insert(
                            branch.clone(),
                            (0..10)
                                .map(|i| file(branch.join(format!("file-{i}"))))
                                .collect(),
                        );
                        view.expanded[0].extend([parent.clone(), branch.clone()]);
                        view.rebuild();
                        view
                    })
                },
            )
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.update(|cx| {
            view.update(cx, |v, cx| {
                v.scroll
                    .0
                    .borrow()
                    .base_handle
                    .set_offset(gpui_kit::point(px(0.), px(-1088.)));
                cx.notify();
            })
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let before = window.find(entry_id(&branch, "sticky-label")).bounds();
            window.click(entry_id(&branch, "sticky-entry"), cx);
            window.render_frame(cx);
            let after = window.find(entry_id(&branch, "label")).bounds();
            assert_eq!(
                before.origin, after.origin,
                "collapse must preserve the clicked folder's visual slot"
            );
        })
        .unwrap();
        assert!(!cx.update(|cx| view.read(cx).expanded[0].contains(&branch)));
    }

    #[gpui_kit::test]
    fn repeated_cached_expansion_keeps_clicked_label_fixed(cx: &mut TestAppContext) {
        let parent = std::path::PathBuf::from("/fixture/folder");
        cx.update(gpui_kit::init);
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::default(),
                        size: size(px(400.), px(500.)),
                    })),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    cx.new(|cx| {
                        let mut view = expanded_fixture(
                            "/fixture".into(),
                            vec![DirectoryEntry {
                                path: parent.clone(),
                                is_dir: true,
                                is_symlink: false,
                            }],
                            cx,
                        );
                        view.directories.insert(
                            parent.clone(),
                            (0..30)
                                .map(|i| DirectoryEntry {
                                    path: parent.join(format!("file-{i}")),
                                    is_dir: false,
                                    is_symlink: false,
                                })
                                .collect(),
                        );
                        view
                    })
                },
            )
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let original = window.find(entry_id(&parent, "label")).bounds();
            for _ in 0..20 {
                window.click(entry_id(&parent, "entry"), cx);
                window.render_frame(cx);
                assert_eq!(window.find(entry_id(&parent, "label")).bounds(), original);
            }
        })
        .unwrap();
        assert!(!cx.update(|cx| view.read(cx).expanded[0].contains(&parent)));
    }
}
