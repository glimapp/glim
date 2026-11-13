use super::Workspace;
use gpui_kit::*;
use std::path::{Path, PathBuf};

impl Workspace {
    pub(super) fn choose_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(directory) = self.root.clone() {
            self.choose_new_folder_in(directory, window, cx);
        }
    }
    pub(super) fn choose_new_folder_in(
        &mut self,
        directory: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_action_dialog(
            super::file_actions::FileAction::NewFolder,
            directory,
            window,
            cx,
        );
    }
    pub(super) fn choose_new_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let directory = self
            .active_reader()
            .and_then(|reader| {
                reader
                    .document
                    .draft_directory
                    .clone()
                    .or_else(|| reader.document.path.parent().map(Path::to_path_buf))
            })
            .or_else(|| self.root.clone())
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));
        self.choose_new_file_in(directory, window, cx);
    }

    pub(super) fn choose_new_file_in(
        &mut self,
        directory: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if glim_services::remote::is_remote_id(&self.recent_id) {
            self.file_action_dialog(
                super::file_actions::FileAction::NewFile,
                directory,
                window,
                cx,
            );
            return;
        }
        if self.picking
            || self.close_request.is_some()
            || crate::app::updates::busy(cx)
            || crate::app::workspace_editors_locked(cx)
        {
            return;
        }
        glim_services::telemetry::track("draft_created", []);
        let reader = crate::views::reader::Reader::untitled(directory, window, cx);
        reader.set_word_wrap(self.word_wrap, window, cx);
        self.load_task = None;
        self.loading = false;
        self.loading_path = None;
        self.page = super::Page::Document;
        self.error = None;
        self.tabs.insert(0, reader);
        self.active = Some(0);
        self.tab_scroll.scroll_to_item(0);
        self.observe_autosave(window, cx);
        self.tabs[0].focus_source(window, cx);
        cx.notify();
    }

    pub(crate) fn has_unsaved_in(&self, path: &Path) -> bool {
        self.tabs.iter().any(|reader| {
            reader.document.restore_on_launch()
                && reader.document.remote.is_none()
                && reader.document.path.starts_with(path)
                && reader.is_dirty()
        })
    }

    pub(super) fn confirm_trash(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.confirm_task.is_some() || crate::app::workspace_editors_locked(cx) {
            return;
        }
        let title = glim_services::locale::tr("Move to Trash?");
        let detail = path.file_name().unwrap_or_default().to_string_lossy();
        let response = window.prompt(
            PromptLevel::Warning,
            &title,
            Some(&detail),
            &[
                glim_services::locale::tr("Cancel").as_str(),
                glim_services::locale::tr("Move to Trash").as_str(),
            ],
            cx,
        );
        self.confirm_task = Some(cx.spawn_in(window, async move |view, cx| {
            let accepted = response.await == Ok(1);
            let _ = view.update_in(cx, |view, window, cx| {
                view.confirm_task = None;
                if accepted {
                    view.trash(path, window, cx);
                } else {
                    glim_services::telemetry::file_operation_cancelled(
                        "trash",
                        if glim_services::remote::is_remote_id(&view.recent_id) {
                            "ssh"
                        } else {
                            "local"
                        },
                        &path,
                    );
                }
            });
        }));
    }

    fn trash(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.remote.is_some() {
            self.trash_remote(path, window, cx);
            return;
        }
        // Share the cross-window mutation lock to prevent autosave racing
        // the move. Recheck after the confirmation dialog.
        if self.is_saving() {
            self.error = Some(glim_services::locale::tr("A file is being saved. Wait for it to finish before moving this item to Trash.").into());
            cx.notify();
            return;
        }
        if let Err(error) =
            crate::app::lock_workspace_editors(cx.entity().entity_id(), &path, true, cx)
        {
            self.error = Some(error.to_string().into());
            cx.notify();
            return;
        }
        self.set_editing_locked(true, cx);
        let open = crate::app::open_document_paths(cx)
            .into_iter()
            .filter(|open| open.starts_with(&path))
            .collect::<std::collections::HashSet<_>>();
        let target = path.clone();
        let work = cx.background_executor().spawn(async move {
            let started = std::time::Instant::now();
            let mut retained = std::collections::HashMap::new();
            let result = (|| -> anyhow::Result<()> {
                for path in open {
                    if let Some(file) = glim_services::files::RetainedFile::open(&path)? {
                        retained.insert(path, file);
                    }
                }
                glim_services::workspace::trash_entry(&target)
            })();
            glim_services::telemetry::file_operation("trash", "local", &target, started, &result);
            (result, std::sync::Arc::new(retained))
        });
        self.confirm_task = Some(cx.spawn_in(window, async move |view, cx| {
            let (result, retained) = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.confirm_task = None;
                view.set_editing_locked(false, cx);
                crate::app::finish_trash_operation(
                    cx.entity().entity_id(),
                    result.as_ref().ok().map(|_| path.clone()),
                    retained.clone(),
                    cx,
                );
                match result {
                    Ok(()) => {
                        view.did_trash(&path, &retained, window, cx);
                    }
                    Err(error) => {
                        view.error = Some(
                            format!(
                                "{}: {error:#}",
                                glim_services::locale::tr("Could not move item to Trash")
                            )
                            .into(),
                        );
                    }
                }
                view.unlock_and_refresh(window, cx);
                cx.notify();
            });
        }));
    }

    pub(crate) fn did_trash_local(
        &mut self,
        path: &Path,
        retained: &std::collections::HashMap<
            PathBuf,
            std::sync::Arc<glim_services::files::RetainedFile>,
        >,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.remote.is_none() && !glim_services::remote::is_remote_id(&self.recent_id) {
            self.did_trash(path, retained, window, cx);
        }
    }
    pub(crate) fn did_trash(
        &mut self,
        path: &Path,
        retained: &std::collections::HashMap<
            PathBuf,
            std::sync::Arc<glim_services::files::RetainedFile>,
        >,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.incoming_files
            .retain(|queued| !queued.starts_with(path));
        self.results.retain(|result| !result.starts_with(path));
        self.refresh_task = None;
        if self
            .loading_path
            .as_ref()
            .is_some_and(|loading| loading.starts_with(path))
        {
            self.load_task = None;
            self.loading = false;
            self.loading_path = None;
        }
        self.incoming_file_task = None;
        self.search_task = None;
        let mut retained_tabs = 0;
        for reader in &mut self.tabs {
            if reader.document.draft_directory.is_none()
                && !reader.document.trashed
                && reader.document.path.starts_with(path)
            {
                if let Some(key) = &reader.document.recovery_key {
                    crate::app::recovery::remove(key, cx);
                }
                reader
                    .document
                    .mark_trashed(retained.get(&reader.document.path).cloned());
                reader.retain_deleted_snapshot(cx);
                retained_tabs += 1;
                if let Some(editor) = reader.editor() {
                    let id = editor.entity_id();
                    if let Some((_, pending)) = self.autosave.get_mut(&id) {
                        *pending = None;
                    }
                    self.save_queue.retain(|queued| *queued != id);
                }
            }
        }
        if retained_tabs > 0 {
            self.record_workspace_action(
                "deleted_tabs_retained",
                [
                    ("semantic", glim_services::telemetry::text("state_change")),
                    ("tab_count", glim_services::telemetry::number(retained_tabs)),
                ],
            );
        }
        if let Some(explorer) = &self.explorer {
            explorer.update(cx, |explorer, cx| explorer.did_trash(path, cx));
        }
        let roots = self
            .explorer
            .as_ref()
            .map(|e| e.read(cx).root_ids_under(path))
            .unwrap_or_default();
        for id in roots {
            self.remove_folder(id, window, cx);
        }
        self.refresh_notice = None;
        self.observe_autosave(window, cx);
        self.remember_session(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::Workspace;
    use gpui_kit::{AppContext, TestAppContext, WindowOptions};

    #[gpui_kit::test]
    fn trashed_open_edits_survive_refresh_without_autosave_or_session_restore(
        cx: &mut TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "original").unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::remote::init(Default::default(), cx);
        });
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Workspace::new(Some(path.clone()), window, cx))
            })
            .unwrap()
        });
        cx.run_until_parked();
        let editor = cx.update(|cx| view.read(cx).active_reader().unwrap().editor().unwrap());
        cx.update_window(handle, |_, window, cx| {
            editor.update(cx, |editor, cx| {
                editor.replace_all("unsaved contents", window, cx)
            });
        })
        .unwrap();
        cx.run_until_parked();
        std::fs::remove_file(&path).unwrap();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.save_queue.push_back(editor.entity_id());
                view.did_trash(&path, &Default::default(), window, cx);
                view.refresh(window, cx);
            })
        })
        .unwrap();
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(2));
        cx.run_until_parked();
        cx.update(|cx| {
            let workspace = view.read(cx);
            assert_eq!(workspace.tabs.len(), 1);
            let reader = workspace.active_reader().unwrap();
            assert!(reader.document.trashed);
            assert_eq!(reader.editor().unwrap().entity_id(), editor.entity_id());
            assert_eq!(editor.read(cx).value().as_ref(), "unsaved contents");
            assert!(reader.refresh_request(true, cx).is_none());
            assert!(workspace.save_queue.is_empty());
            assert!(workspace.update_session(cx).tabs.is_empty());
            assert!(workspace.refresh_notice.is_none());
        });
        assert!(!path.exists());
    }
}
