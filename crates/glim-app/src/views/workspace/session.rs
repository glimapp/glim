use super::Workspace;
use crate::app::reader_jobs::{self, JobClass, Priority};
use crate::views::reader::Reader;
use glim_core::{
    session::{SessionTab, WorkspaceSession},
    update::UpdateActivity,
};
use gpui_kit::*;

impl Workspace {
    #[cfg(test)]
    pub(crate) fn focus_active_editor(&self, window: &mut Window, cx: &mut App) {
        if let Some(reader) = self.active_reader() {
            reader.focus_source(window, cx);
        }
    }

    pub(crate) fn update_activity(&self, _cx: &App) -> UpdateActivity {
        UpdateActivity {
            dirty: self.has_unsaved(),
            saving: self.saving.is_some()
                || !self.save_queue.is_empty()
                || self.tabs.iter().any(|r| r.is_persisting()),
            loading: self.picking
                || self.loading
                || self.incoming_file_task.is_some()
                || !self.incoming_files.is_empty()
                || self.confirm_task.is_some(),
        }
    }
    fn session_snapshot(&self, cx: &App) -> WorkspaceSession {
        if let Some(pending) = &self.remote_restore {
            let mut session = pending.clone();
            session.name = self.workspace_name.clone();
            session.show_home = self.is_home();
            for reader in &self.tabs {
                let tab = reader.session_tab(cx);
                let path = |tab: &SessionTab| match tab {
                    SessionTab::File { path, .. } | SessionTab::Playlist { path, .. } => {
                        path.clone()
                    }
                };
                if !session
                    .tabs
                    .iter()
                    .any(|existing| path(existing) == path(&tab))
                {
                    session.tabs.push(tab);
                }
            }
            return session;
        }
        WorkspaceSession {
            roots: self.roots.clone(),
            tabs: self
                .tabs
                .iter()
                .filter(|r| r.document.draft_directory.is_none())
                .map(|r| r.session_tab(cx))
                .collect(),
            active: self
                .active
                .filter(|index| self.tabs[*index].document.draft_directory.is_none())
                .map(|index| {
                    self.tabs[..index]
                        .iter()
                        .filter(|r| r.document.draft_directory.is_none())
                        .count()
                }),
            workspace_id: Some(self.recent_id.clone()),
            name: self.workspace_name.clone(),
            window_group: None,
            window_active: false,
            show_home: self.is_home(),
        }
    }
    pub(crate) fn update_session(&self, cx: &App) -> WorkspaceSession {
        let mut session = self.session_snapshot(cx);
        if let Some(host) = self
            .workspace_window
            .as_ref()
            .and_then(|host| host.upgrade())
        {
            let host = host.read(cx);
            session.window_group = Some(host.group.clone());
            session.window_active = host
                .slots
                .get(host.active)
                .is_some_and(|slot| slot.workspace.id == self.recent_id);
            if session.name.is_none() {
                session.name = host
                    .slots
                    .iter()
                    .find(|slot| slot.workspace.id == self.recent_id)
                    .and_then(|slot| slot.workspace.name.clone());
            }
        }
        session
    }
    pub(crate) fn window_session(&self, cx: &App) -> Option<WorkspaceSession> {
        (!self.loading && self.restoring_roots.is_none()).then(|| self.session_snapshot(cx))
    }
    pub(crate) fn remember_session(&self, cx: &mut App) {
        if self.loading
            || self.restoring_roots.is_some()
            || self.roots.is_empty() && self.tabs.is_empty() && self.workspace_name.is_none()
        {
            return;
        }
        crate::app::workspace_sessions::remember(self.session_snapshot(cx), cx);
        if let Some(host) = self.workspace_window.clone() {
            cx.defer(move |cx| {
                let _ = host.update(cx, |host, cx| host.remember_window(cx));
            });
        }
    }
    pub(crate) fn take_reader_writes(&mut self, cx: &mut App) -> Vec<Task<()>> {
        self.tabs
            .iter_mut()
            .filter_map(|r| r.take_reader_write(cx))
            .collect()
    }
    pub(crate) fn take_preference_write(&mut self) -> Option<Task<()>> {
        self.preference_task.take()
    }

    pub(crate) fn restore_update_session(
        &mut self,
        session: WorkspaceSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(descriptor) = cx
            .try_global::<crate::app::remote::Connections>()
            .and_then(|state| {
                session
                    .workspace_id
                    .as_ref()
                    .and_then(|id| state.catalog.workspaces.get(id))
            })
            .cloned()
        {
            self.restore_remote_workspace(session, descriptor, window, cx);
            return;
        }
        if session
            .workspace_id
            .as_deref()
            .is_some_and(glim_services::remote::is_remote_id)
        {
            self.remote_restore = Some(session.clone());
            self.recent_id = session.workspace_id.unwrap_or_default();
            self.roots = session.roots;
            self.workspace_name = session.name;
            self.page = super::Page::Document;
            self.remote_disconnected = true;
            self.error = Some(
                glim_services::locale::tr(
                    "SSH connection was removed. Open a folder using an available connection.",
                )
                .into(),
            );
            return;
        }
        if let Some(id) = &session.workspace_id {
            self.recent_id = id.clone();
        }
        self.workspace_name = session.name.clone();
        self.page = if session.show_home {
            super::Page::Start
        } else {
            super::Page::Document
        };
        self.loading = true;
        self.loading_path = None;
        self.restoring_roots = Some(session.roots.clone());
        let folders = session.roots.clone();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let folder_cancel = cancel.clone();
        let work = cx.background_executor().spawn(async move {
            let expanded = glim_services::preferences::explorer_expansion(&folders);
            let folders = folders
                .into_iter()
                .map(|p| glim_services::workspace::open_folder_cancellable(&p, &folder_cancel))
                .collect::<Vec<_>>();
            (folders, expanded)
        });
        self.load_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let (folders, expanded) = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                for folder in folders {
                    match folder {
                        Ok(folder) => view.add_folder(folder, window, cx),
                        Err(error) => view.error = Some(format!("{error:#}").into()),
                    }
                }
                match expanded {
                    Ok(paths) => {
                        if let Some(explorer) = &view.explorer {
                            explorer
                                .update(cx, |explorer, cx| explorer.restore_expanded(paths, cx));
                        }
                    }
                    Err(error) => {
                        view.error = Some(format!("Cannot restore file tree: {error:#}").into())
                    }
                }
            });
            for descriptor in session.tabs {
                let requested = descriptor.clone();
                let cancel = cancel.clone();
                let loaded = reader_jobs::run(
                    cx.background_executor().clone(),
                    JobClass::Document,
                    Priority::Background,
                    cancel,
                    move |cancel| glim_services::documents::read_session_tab(&requested, cancel),
                )
                .await;
                if view
                    .update_in(cx, |view, window, cx| {
                        let mut reader = match loaded {
                            Ok(content) => Reader::from_restored_content(content, window, cx),
                            Err(error) => Reader::unavailable(
                                tab_path(&descriptor),
                                (&error).into(),
                                window,
                                cx,
                            ),
                        };
                        if let SessionTab::File { preview, .. } = descriptor {
                            reader.set_preview(preview, cx);
                        }
                        reader.set_word_wrap(view.word_wrap, window, cx);
                        reader.report_opened("restore");
                        view.tabs.push(reader);
                        view.active = Some(view.tabs.len() - 1);
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
            let _ = view.update_in(cx, |view, window, cx| {
                view.loading = false;
                view.restoring_roots = None;
                view.sync_workspace(cx);
                view.load_incoming_file(window, cx);
                view.active = session
                    .active
                    .filter(|&i| i < view.tabs.len())
                    .or_else(|| (!view.tabs.is_empty()).then_some(0));
                view.reveal_active_file(cx);
                view.observe_autosave(window, cx);
                if view.root.is_none() {
                    view.watch_file_parent(window, cx);
                }
                view.record_history(cx);
                view.remember_session(cx);
                cx.notify();
            });
        }));
    }
}
fn tab_path(tab: &SessionTab) -> std::path::PathBuf {
    match tab {
        SessionTab::File { path, .. } | SessionTab::Playlist { path, .. } => path.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::Workspace;
    use glim_core::session::{SessionTab, WorkspaceSession};
    use gpui_kit::{AppContext, TestAppContext, WindowOptions};

    #[gpui_kit::test]
    fn restores_tabs_before_new_finder_files_and_keeps_missing_files_visible(
        cx: &mut TestAppContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let text = dir.path().join("note.txt");
        let markdown = dir.path().join("note.md");
        let missing = dir.path().join("removed.txt");
        let incoming = dir.path().join("incoming.txt");
        for (path, data) in [
            (&text, "note"),
            (&markdown, "# Title"),
            (&incoming, "new file"),
        ] {
            std::fs::write(path, data).unwrap();
        }
        cx.update(gpui_kit::init);
        cx.update(|cx| crate::app::remote::init(Default::default(), cx));
        let (window, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Workspace::new(None, window, cx))
            })
            .unwrap()
        });
        cx.update_window(window, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.restore_update_session(
                    WorkspaceSession {
                        roots: Vec::new(),
                        tabs: [&text, &markdown, &missing]
                            .into_iter()
                            .map(|path| SessionTab::File {
                                path: path.clone(),
                                preview: false,
                            })
                            .collect(),
                        active: Some(1),
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                view.open_system_file(incoming.clone(), window, cx);
            })
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            let view = view.read(cx);
            assert_eq!(
                view.tabs
                    .iter()
                    .map(|r| &r.document.path)
                    .collect::<Vec<_>>(),
                vec![&incoming, &text, &markdown, &missing]
            );
            assert_eq!(view.active, Some(0));
            assert_eq!(view.tabs[2].mode(), glim_core::ReaderMode::Source);
            assert!(view.tabs[3].is_unavailable());
            assert!(!view.loading);
        });
    }
}
