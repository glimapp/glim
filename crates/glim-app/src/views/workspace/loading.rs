use super::Workspace;
use crate::app::reader_jobs::{self, JobClass, Priority};
use crate::views::{
    explorer::{Explorer, ExplorerEvent},
    reader::Reader,
};
pub(super) use glim_services::documents::{Content, read_content_cancellable};
use glim_services::workspace::{FolderSnapshot, open_folder_cancellable};
use gpui_kit::*;
use std::path::PathBuf;

enum Opened {
    Folder(FolderSnapshot),
    File(PathBuf, anyhow::Result<Box<Content>>),
}
impl Workspace {
    pub(super) fn pending_open_path(&self) -> Option<&PathBuf> {
        if self.loading {
            self.loading_path.as_ref()
        } else if self.incoming_file_task.is_some() {
            self.incoming_files.front()
        } else {
            None
        }
    }

    pub(super) fn cancel_pending_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading_path.is_some() && self.loading {
            self.load_task = None;
            self.loading = false;
            self.loading_path = None;
        } else if self.incoming_file_task.is_some() {
            self.incoming_file_task = None;
            self.incoming_files.pop_front();
        }
        self.load_incoming_file(window, cx);
        self.drain_remote_open_paths(window, cx);
        cx.notify();
    }

    pub(crate) fn restore_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.loading = true;
        self.loading_path = None;
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = cx.background_executor().spawn(async move {
            let paths = glim_services::preferences::workspace_folders()?;
            let recent = glim_services::preferences::recent_workspaces()?
                .into_iter()
                .find(|workspace| workspace.folders == paths);
            let expanded = glim_services::preferences::explorer_expansion(&paths);
            let folders = paths
                .into_iter()
                .map(|path| open_folder_cancellable(&path, &cancel))
                .collect::<Vec<_>>();
            Ok::<_, anyhow::Error>((folders, expanded, recent))
        });
        self.load_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                let mut errors = Vec::new();
                match result {
                    Ok((folders, expanded, recent)) => {
                        if let Some(recent) = recent {
                            let session =
                                crate::app::workspace_sessions::restore(recent.clone(), cx);
                            if !session.tabs.is_empty() {
                                view.restore_update_session(session, window, cx);
                                return;
                            }
                            view.recent_id = recent.id;
                            view.workspace_name = recent.name;
                        }
                        for folder in folders {
                            match folder {
                                Ok(folder) => view.add_folder(folder, window, cx),
                                Err(error) => errors.push(format!("{error:#}")),
                            }
                        }
                        match expanded {
                            Ok(paths) => {
                                if let Some(explorer) = &view.explorer {
                                    explorer.update(cx, |explorer, cx| {
                                        explorer.restore_expanded(paths, cx)
                                    });
                                }
                            }
                            Err(error) => errors.push(format!("{error:#}")),
                        }
                    }
                    Err(error) => errors.push(format!("{error:#}")),
                }
                view.loading = false;
                view.sync_workspace(cx);
                view.remember_session(cx);
                view.load_incoming_file(window, cx);
                if !errors.is_empty() {
                    view.error =
                        Some(format!("Cannot restore workspace: {}", errors.join("; ")).into());
                }
                cx.notify();
            });
        }));
    }
    pub(crate) fn reveal_file_path(
        &mut self,
        path: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if glim_services::remote::is_remote_id(&self.recent_id) {
            return false;
        }
        if self.roots.iter().any(|root| root == path) {
            self.select_workspace(self.recent_id.clone(), window, cx);
            self.page = super::Page::Document;
            self.explorer_visible = true;
            self.sidebar_window_width = 0.;
            cx.notify();
            window.activate_window();
            cx.activate(true);
            return true;
        }
        let index = self.tabs.iter().position(|r| r.document.path == path);
        if let Some(index) = index {
            self.select_workspace(self.recent_id.clone(), window, cx);
            self.activate_tab(index, window, cx);
            self.tabs[index].activate(cx);
            window.activate_window();
            cx.activate(true);
            true
        } else {
            false
        }
    }
    pub(crate) fn open_system_file(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.page = super::Page::Document;
        // All Finder file types share one queue. Extension-based routing here
        // would split ordinary text/images and native formats across windows.
        if self.reveal_file_path(&path, window, cx) {
            return true;
        }
        if !self.incoming_files.contains(&path) {
            self.incoming_files.push_back(path);
        }
        self.load_incoming_file(window, cx);
        window.activate_window();
        true
    }
    pub(super) fn load_incoming_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.incoming_file_task.is_some() || self.loading {
            return;
        }
        let Some(path) = self.incoming_files.front().cloned() else {
            return;
        };
        self.tab_scroll.scroll_to_item(0);
        let (cancel, cancel_on_drop) = glim_services::work::Cancellation::task();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Document,
            Priority::Interactive,
            cancel,
            move |cancel| {
                let started = std::time::Instant::now();
                glim_services::telemetry::file_event("file_open_requested", &path, []);
                if path.is_dir() && !glim_services::preview::is_iwork(&path) {
                    let result = open_folder_cancellable(&path, cancel);
                    glim_services::telemetry::result("workspace_open", None, started, &result);
                    result.map(Opened::Folder)
                } else {
                    let result = read_content_cancellable(&path, cancel).map(Box::new);
                    glim_services::telemetry::result("file_open", Some(&path), started, &result);
                    Ok(Opened::File(path, result))
                }
            },
        );
        self.incoming_file_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _cancel_on_drop = cancel_on_drop;
            let result = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                let path = view.incoming_files.pop_front();
                view.incoming_file_task = None;
                if let Some(index) = view
                    .tabs
                    .iter()
                    .position(|r| Some(&r.document.path) == path.as_ref())
                {
                    view.activate_tab(index, window, cx);
                } else {
                    match result {
                        Ok(Opened::Folder(folder)) => view.receive_folder(folder, window, cx),
                        Ok(Opened::File(_, Ok(content))) => {
                            view.install_content(*content, window, cx)
                        }
                        Ok(Opened::File(path, Err(error))) => view.install_content(
                            Content::Unavailable(path, (&error).into()),
                            window,
                            cx,
                        ),
                        Err(error) => view.error = Some(format!("{error:#}").into()),
                    }
                }
                view.load_incoming_file(window, cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.close_search(window, cx);
        if self
            .pending_search_location
            .as_ref()
            .is_some_and(|(target, _)| target != &path)
        {
            self.pending_search_location = None;
        }
        if self.pending_open_path() == Some(&path) {
            return;
        }
        if glim_services::remote::is_remote_id(&self.recent_id) {
            self.open_remote_file(path, window, cx);
            return;
        }
        glim_services::diagnostics::event(
            "INFO",
            &format!("open requested file={}", path.display()),
        );
        self.page = super::Page::Document;
        if crate::app::updates::busy(cx) {
            return;
        }
        self.results.clear();
        self.search_task = None;
        self.pending_anchor = None;
        if let Some(index) = self.tabs.iter().position(|t| t.document.path == path) {
            self.activate_tab(index, window, cx);
            self.tabs[index].activate(cx);
            return;
        }
        let offset = self
            .pending_search_location
            .as_ref()
            .map(|(_, offset)| *offset);
        if crate::app::reveal_existing_file_at(&path, Some(cx.entity_id()), offset, cx) {
            self.pending_search_location = None;
            return;
        }
        glim_services::telemetry::file_event("file_open_requested", &path, []);
        self.restoring_roots = None;
        self.loading = true;
        self.loading_path = Some(path.clone());
        self.tab_scroll.scroll_to_item(0);
        self.error = None;
        let (cancel, cancel_on_drop) = glim_services::work::Cancellation::task();
        let read = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Document,
            Priority::Interactive,
            cancel,
            move |cancel| {
                let started = std::time::Instant::now();
                if path.is_dir() && !glim_services::preview::is_iwork(&path) {
                    let result = open_folder_cancellable(&path, cancel);
                    glim_services::telemetry::result("workspace_open", None, started, &result);
                    result.map(Opened::Folder)
                } else {
                    let result = read_content_cancellable(&path, cancel).map(Box::new);
                    glim_services::telemetry::result("file_open", Some(&path), started, &result);
                    Ok(Opened::File(path, result))
                }
            },
        );
        self.load_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _cancel_on_drop = cancel_on_drop;
            let result = read.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.loading = false;
                view.load_incoming_file(window, cx);
                match result {
                    Ok(Opened::Folder(folder)) => {
                        view.receive_folder(folder, window, cx);
                    }
                    Ok(Opened::File(_, Ok(document))) => {
                        view.install_content(*document, window, cx)
                    }
                    Ok(Opened::File(path, Err(error))) => view.install_content(
                        Content::Unavailable(path, (&error).into()),
                        window,
                        cx,
                    ),
                    Err(error) => view.error = Some(format!("{error:#}").into()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn install_folder(
        &mut self,
        snapshot: FolderSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.subscriptions.clear();
        self.explorer = None;
        self.roots.clear();
        self.root = None;
        self.add_folder(snapshot, window, cx);
    }

    pub(super) fn add_folder(
        &mut self,
        snapshot: FolderSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if crate::app::updates::busy(cx) {
            return;
        }
        self.explorer_visible = true;
        self.sidebar_window_width = 0.;
        self.roots.push(snapshot.root.clone());
        glim_services::telemetry::track(
            "workspace_folder_added",
            [(
                "folder_count",
                glim_services::telemetry::number(self.roots.len() as u64),
            )],
        );
        self.root = self.roots.first().cloned();
        if !self.loading && cx.try_global::<crate::app::ReaderPreferences>().is_some() {
            self.remember_current_workspace(cx);
        }
        self.refresh_task = None;
        self.error = None;
        self.search_task = None;
        self.watch_setup = None;
        self.watch_task = None;
        self.watch = None;
        self.watch_scan_task = None;
        if let Some(explorer) = &self.explorer {
            explorer.update(cx, |v, cx| {
                v.add_folder(snapshot.root, snapshot.entries, cx)
            });
        } else {
            let remote = self.remote.clone();
            let explorer = cx.new(|cx| {
                let mut explorer = Explorer::new(snapshot.root, snapshot.entries, cx);
                explorer.set_remote(remote);
                explorer
            });
            self.subscriptions.push(cx.subscribe_in(
                &explorer,
                window,
                |view, _, event, window, cx| match event {
                    ExplorerEvent::SelectionChanged => {
                        cx.notify();
                    }
                    ExplorerEvent::OpenFile(path) => view.open_path(path.clone(), window, cx),
                    ExplorerEvent::NewFile(path) => {
                        view.choose_new_file_in(path.clone(), window, cx)
                    }
                    ExplorerEvent::TrashEntry(path) => view.confirm_trash(path.clone(), window, cx),
                    ExplorerEvent::RemoveFolder(root_id) => {
                        view.remove_folder(*root_id, window, cx)
                    }
                    ExplorerEvent::PlayFolder(path, shuffle) => {
                        view.install_content(
                            Content::Native(path.clone(), true, *shuffle),
                            window,
                            cx,
                        );
                    }
                    ExplorerEvent::ExpansionChanged(roots, expanded) => {
                        crate::app::remember_explorer(roots.clone(), expanded.clone(), cx);
                    }
                    ExplorerEvent::Error(error) => {
                        view.remote_disconnected = view
                            .remote
                            .as_ref()
                            .is_some_and(|remote| !remote.is_connected());
                        view.error = Some(error.clone().into());
                        cx.notify();
                    }
                },
            ));
            self.explorer = Some(explorer);
        }
        if !self.loading
            && let Some(explorer) = &self.explorer
        {
            let (roots, expanded) = explorer.read(cx).expansion_state();
            crate::app::remember_explorer(roots, expanded, cx);
        }
        self.scan_workspace_roots(window, cx);
        self.discover_git_repositories(cx);
    }

    pub(super) fn remove_folder(
        &mut self,
        root_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if crate::app::updates::busy(cx) {
            return;
        }
        let Some(explorer) = self.explorer.clone() else {
            return;
        };
        let Some(index) = explorer.update(cx, |explorer, cx| explorer.remove_folder(root_id, cx))
        else {
            return;
        };
        if self.workspace_name.is_none() {
            self.workspace_name = Some(self.workspace_descriptor().title());
        }
        self.roots.remove(index);
        self.remember_remote(cx);
        self.root = self.roots.first().cloned();
        self.restoring_roots = None;
        self.results.clear();
        self.search_task = None;
        self.pending_anchor = None;
        if cx.try_global::<crate::app::RecentWorkspaces>().is_some() {
            self.remember_current_workspace(cx);
        }
        let (roots, expanded) = explorer.read(cx).expansion_state();
        crate::app::remember_explorer(roots, expanded, cx);
        self.scan_workspace_roots(window, cx);
        self.discover_git_repositories(cx);
        cx.notify();
    }

    pub(super) fn discover_git_repositories(&mut self, cx: &mut Context<Self>) {
        self.git_discovery_task = None;
        if glim_services::remote::is_remote_id(&self.recent_id) || self.roots.is_empty() {
            self.git_repositories.clear();
            self.changes_expanded = false;
            return;
        }
        let roots = self.roots.clone();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = cx.background_executor().spawn(async move {
            let repositories = glim_services::workspace::discover_git_roots(&roots, &cancel);
            (roots, repositories)
        });
        self.git_discovery_task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let (roots, result) = work.await;
            let _ = view.update(cx, |view, cx| {
                if view.roots != roots {
                    return;
                }
                if let Ok(repositories) = result {
                    view.git_repositories = repositories;
                    if view.git_repositories.is_empty() {
                        view.changes_expanded = false;
                    }
                    cx.notify();
                }
            });
        }));
    }

    fn scan_workspace_roots(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if glim_services::remote::is_remote_id(&self.recent_id) {
            self.start_remote_search_index(false, window, cx);
            self.start_remote_watching(window, cx);
            return;
        }
        self.refresh_task = None;
        self.watch_scan_task = None;
        self.watch_setup = None;
        self.watch_task = None;
        self.watch = None;
        if self.roots.is_empty() {
            if !self.tabs.is_empty() {
                self.watch_file_parent(window, cx);
            }
            return;
        }
        let roots = self.roots.clone();
        let parents = self
            .tabs
            .iter()
            .filter_map(|r| r.document.path.parent().map(ToOwned::to_owned))
            .collect::<Vec<_>>();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = cx.background_executor().spawn(async move {
            if cancel.check().is_err() {
                return (roots, None);
            }
            let root = &roots[0];
            let mut watch = glim_services::watch::WorkspaceWatch::new(root).ok();
            if let Some(watch) = &mut watch {
                for path in roots.iter().skip(1).chain(parents.iter()) {
                    if cancel.check().is_err() {
                        return (roots, None);
                    }
                    let _ = watch.add_path(path);
                }
            }
            (roots, watch)
        });
        self.watch_scan_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let (roots, watch) = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                if view.roots != roots {
                    return;
                }
                view.watch_scan_task = None;
                view.watch = watch;
                view.start_watching(window, cx);
                // Reconcile changes between the initial listing and watcher registration.
                if let Some(explorer) = &view.explorer {
                    explorer.update(cx, |v, cx| v.refresh(cx));
                }
                cx.notify();
            });
        }));
    }
    pub(super) fn install_content(
        &mut self,
        content: Content,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut reader = Reader::from_content(content, window, cx);
        reader.report_opened("open");
        reader.set_editing_locked(self.editing_locked, cx);
        reader.set_word_wrap(self.word_wrap, window, cx);
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.document.path == reader.document.path)
        {
            self.tabs[i] = reader;
            self.active = Some(i);
        } else {
            self.tabs.insert(0, reader);
            self.active = Some(0);
            self.tab_scroll.scroll_to_item(0);
        }
        if let Some((path, anchor)) = self.pending_anchor.take()
            && let Some(i) = self.active
            && self.tabs[i].document.path == path
        {
            self.tabs[i].reveal_anchor(&anchor, cx);
        }
        self.reveal_search_location(window, cx);
        self.reveal_active_file(cx);
        self.observe_autosave(window, cx);
        self.record_history(cx);
        window.set_window_title("Glim");
        if self.root.is_none() {
            self.watch_file_parent(window, cx);
        }
    }
    pub(super) fn choose_add_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if glim_services::remote::is_remote_id(&self.recent_id) && self.remote.is_none() {
            return;
        }
        if let Some(connection) = self.remote_connection(cx) {
            self.show_remote(Some(connection), false, true, window, cx);
            return;
        }
        if self.picking || crate::app::updates::busy(cx) {
            return;
        }
        self.picking = true;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some(glim_services::locale::tr("Add folders to workspace").into()),
        });
        self.picker_task = Some(cx.spawn_in(window, async move |view, cx| {
            let selection = selection.await;
            let paths = match selection {
                Ok(Ok(Some(paths))) => paths,
                Ok(Ok(None)) => {
                    let _ = view.update(cx, |v, cx| {
                        v.picking = false;
                        cx.notify();
                    });
                    return;
                }
                other => {
                    let _ = view.update(cx, |v, cx| {
                        v.picking = false;
                        v.error = Some(format!("Cannot choose folders: {other:?}").into());
                        cx.notify();
                    });
                    return;
                }
            };
            let (cancel, guard) = glim_services::work::Cancellation::task();
            let _guard = guard;
            for path in paths {
                let cancel = cancel.clone();
                let folder = cx
                    .background_executor()
                    .spawn(async move { open_folder_cancellable(&path, &cancel) })
                    .await;
                let _ = view.update_in(cx, |v, window, cx| {
                    match folder {
                        Ok(folder) => v.add_folder(folder, window, cx),
                        Err(error) => v.error = Some(format!("{error:#}").into()),
                    }
                    cx.notify();
                });
            }
            let _ = view.update(cx, |v, cx| {
                v.picking = false;
                cx.notify();
            });
        }));
    }

    pub(super) fn choose_path(
        &mut self,
        folder: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.picking || crate::app::updates::busy(cx) {
            return;
        }
        self.picking = true;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: !folder,
            directories: folder,
            multiple: false,
            prompt: Some(if folder { "Open folder" } else { "Open file" }.into()),
        });
        self.picker_task = Some(cx.spawn_in(window, async move |view, cx| {
            let result = selection.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.picking = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            if (folder || glim_services::remote::is_remote_id(&view.recent_id))
                                && let Some(host) = view.workspace_window.clone()
                            {
                                window.defer(cx, move |window, cx| {
                                    let _ = host.update(cx, |host, cx| {
                                        host.append(Some(path), None, window, cx);
                                    });
                                });
                            } else {
                                view.open_path(path, window, cx);
                            }
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(e)) => view.error = Some(e.to_string().into()),
                    Err(e) => view.error = Some(e.to_string().into()),
                }
                cx.notify();
            });
        }));
    }
}

#[cfg(test)]
mod incoming_file_tests {
    use super::Workspace;
    use gpui_kit::{AppContext, TestAppContext, WindowOptions};

    #[gpui_kit::test]
    fn finder_files_leave_start_page_and_reuse_the_existing_reader(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("incoming.txt");
        std::fs::write(&path, "Finder opened this file").unwrap();
        cx.update(gpui_kit::init);
        cx.update(|cx| crate::app::remote::init(Default::default(), cx));
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Workspace::new(None, window, cx))
            })
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                assert!(matches!(view.page, crate::views::workspace::Page::Start));
                view.open_system_file(path.clone(), window, cx);
                assert!(matches!(view.page, crate::views::workspace::Page::Document));
            })
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                let editor = view.tabs[0].editor().unwrap().entity_id();
                view.page = crate::views::workspace::Page::Start;
                view.open_system_file(path, window, cx);
                assert!(matches!(view.page, crate::views::workspace::Page::Document));
                assert_eq!(view.tabs.len(), 1);
                assert_eq!(view.tabs[0].editor().unwrap().entity_id(), editor);
            })
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn incoming_files_queue_without_cancelling_or_replacing_existing_tabs(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("notes.txt");
        let second = directory.path().join("README.md");
        let image = directory.path().join("picture.gif");
        let binary = directory.path().join("unknown.custom-format");
        std::fs::write(&first, "ordinary text").unwrap();
        std::fs::write(&second, "# Markdown").unwrap();
        // The simulated GPUI window cannot host AppKit image controllers.
        // Keep the queue's media case on the GPUI GIF reader.
        std::fs::write(
            &image,
            [
                71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 0, 0, 0, 255, 255, 255, 44, 0, 0, 0,
                0, 1, 0, 1, 0, 0, 2, 2, 68, 1, 0, 59,
            ],
        )
        .unwrap();
        std::fs::write(&binary, [0, 255, 0, 255]).unwrap();
        cx.update(gpui_kit::init);
        cx.update(|cx| crate::app::remote::init(Default::default(), cx));
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Workspace::new(Some(first.clone()), window, cx))
            })
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                for path in [&first, &second, &image, &binary, &first] {
                    assert!(view.open_system_file(path.clone(), window, cx));
                }
            });
        })
        .unwrap();
        cx.run_until_parked();
        let editor = cx.update(|cx| {
            let view = view.read(cx);
            assert_eq!(view.tabs.len(), 4);
            assert_eq!(view.tabs[3].document.path, first);
            assert_eq!(view.tabs[2].document.path, second);
            assert_eq!(view.tabs[1].document.path, image);
            assert!(!view.tabs[1].is_unavailable());
            assert_eq!(view.tabs[0].document.path, binary);
            assert!(!view.tabs[0].is_unavailable());
            assert!(view.tabs[0].editor().is_none());
            assert!(view.incoming_files.is_empty());
            assert!(view.incoming_file_task.is_none());
            view.tabs[3].editor().unwrap().entity_id()
        });
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                assert!(view.open_system_file(first.clone(), window, cx));
                assert_eq!(view.active, Some(3));
                assert_eq!(view.tabs[3].editor().unwrap().entity_id(), editor);
                assert!(view.incoming_file_task.is_none());
            });
        })
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::Workspace;
    use gpui_kit::{AppContext, TestAppContext, WindowOptions, test::TestWindowExt};

    #[gpui_kit::test]
    fn opening_retains_all_tabs_and_reuses_existing_readers(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let paths = (0..20)
            .map(|index| directory.path().join(format!("note-{index}.txt")))
            .collect::<Vec<_>>();
        for path in &paths {
            std::fs::write(path, "original\n").unwrap();
        }
        cx.update(gpui_kit::init);
        cx.update(|cx| crate::app::remote::init(Default::default(), cx));
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Workspace::new(None, window, cx))
            })
            .unwrap()
        });
        for path in &paths[..2] {
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.open_path(path.clone(), window, cx);
                })
            })
            .unwrap();
            cx.run_until_parked();
        }
        cx.update(|cx| {
            let workspace = view.read(cx);
            assert_eq!(workspace.tabs.len(), 2);
            assert_eq!(workspace.tabs[0].document.path, paths[1]);
            assert_eq!(workspace.active_reader().unwrap().document.path, paths[1]);
        });
        let editor = cx.update(|cx| view.read(cx).active_reader().unwrap().editor().unwrap());
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click(("input", editor.entity_id()), cx);
            window.input("edited", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            assert!(view.read(cx).tabs[0].is_dirty());
        });
        for path in &paths[2..] {
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.open_path(path.clone(), window, cx);
                })
            })
            .unwrap();
            cx.run_until_parked();
            // Explicit opening keeps the same reader rather than opening a duplicate.
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.open_path(path.clone(), window, cx);
                })
            })
            .unwrap();
        }
        cx.update(|cx| {
            let workspace = view.read(cx);
            assert_eq!(workspace.tabs.len(), 20);
            assert_eq!(workspace.tabs[0].document.path, paths[19]);
            assert_eq!(workspace.tabs[19].document.path, paths[0]);
            assert!(workspace.tabs[18].is_dirty());
        });
        assert_eq!(std::fs::read_to_string(&paths[1]).unwrap(), "original\n");
    }
}
