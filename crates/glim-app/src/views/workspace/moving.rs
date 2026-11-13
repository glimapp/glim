use super::Workspace;
use glim_core::session::SessionTab;
use glim_services::documents::Content;
use gpui_kit::*;
use std::path::{Path, PathBuf};

impl Workspace {
    pub(super) fn choose_move_entry(
        &mut self,
        source: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if glim_services::remote::is_remote_id(&self.recent_id) {
            self.remote_file_dialog(
                super::remote::operations::FileAction::Move,
                source,
                window,
                cx,
            );
            return;
        }
        if self.picking || crate::app::workspace_editors_locked(cx) {
            return;
        }
        self.picking = true;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(glim_services::locale::tr("Move Here").into()),
        });
        self.picker_task = Some(cx.spawn_in(window, async move |view, cx| {
            let selection = selection.await;
            let _ = view.update(cx, |view, cx| {
                view.picking = false;
                match selection {
                    Ok(Ok(Some(paths))) => {
                        if let Some(directory) = paths.into_iter().next() {
                            let requester = cx.entity().downgrade();
                            cx.defer(move |cx| {
                                crate::app::file_move::start(source, directory, requester, cx)
                            });
                        }
                    }
                    Ok(Ok(None)) => {}
                    error => {
                        view.error = Some(
                            format!(
                                "{}: {error:?}",
                                glim_services::locale::tr("Could not move item")
                            )
                            .into(),
                        )
                    }
                }
                cx.notify();
            });
        }));
    }
    pub(crate) fn check_file_move(&self, source: &Path, cx: &App) -> anyhow::Result<()> {
        if glim_services::remote::is_remote_id(&self.recent_id) {
            return Ok(());
        }
        anyhow::ensure!(
            !self.update_activity(cx).loading
                && !self.is_saving()
                && self
                    .tabs
                    .iter()
                    .filter(|reader| reader.document.path.starts_with(source))
                    .all(|reader| reader.can_move_to_window(cx)),
            glim_services::locale::tr(
                "A file operation is in progress. Wait for it to finish before moving this item."
            )
        );
        anyhow::ensure!(
            !self.has_unsaved_in(source),
            glim_services::locale::tr(
                "An open file in this item has unsaved changes. Save it before moving this item."
            )
        );
        Ok(())
    }

    pub(crate) fn tabs_under(&self, source: &Path, cx: &App) -> Vec<SessionTab> {
        if glim_services::remote::is_remote_id(&self.recent_id) {
            return Vec::new();
        }
        self.tabs
            .iter()
            .filter(|reader| {
                reader.document.restore_on_launch() && reader.document.path.starts_with(source)
            })
            .map(|reader| reader.session_tab(cx))
            .collect()
    }

    pub(crate) fn replace_moved_tab(
        &mut self,
        old_path: &Path,
        content: Content,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.tabs.iter().position(|reader| {
            reader.document.path == old_path && reader.document.restore_on_launch()
        }) else {
            return;
        };
        let mut reader = super::Reader::from_restored_content(content, window, cx);
        reader.inherit_view(&self.tabs[index], cx);
        reader.set_word_wrap(self.word_wrap, window, cx);
        reader.set_editing_locked(self.editing_locked, cx);
        self.tabs[index].deactivate(cx);
        self.tabs[index] = reader;
        if self.active == Some(index) && self.is_visible_workspace(cx) {
            self.tabs[index].activate(cx);
        }
        self.observe_autosave(window, cx);
        cx.notify();
    }

    pub(crate) fn did_move_entry(
        &mut self,
        source: &Path,
        destination: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if glim_services::remote::is_remote_id(&self.recent_id) {
            return;
        }
        let relocate =
            |path: &PathBuf| crate::app::file_move::relocated_path(path, source, destination);
        let roots = self.roots.iter().map(relocate).collect::<Vec<_>>();
        let roots_changed = roots != self.roots;
        self.roots = roots;
        self.root = self.roots.first().cloned();
        self.incoming_files = self.incoming_files.iter().map(relocate).collect();
        for reader in &mut self.tabs {
            if let Some(directory) = &mut reader.document.draft_directory {
                *directory = relocate(directory);
            }
        }
        for target in &mut self.history {
            target.path = relocate(&target.path);
        }
        if let Some((path, _)) = &mut self.pending_anchor {
            *path = relocate(path);
        }
        if let Some((path, _)) = &mut self.pending_search_location {
            *path = relocate(path);
        }
        self.results
            .iter_mut()
            .for_each(|path| *path = relocate(path));
        if self
            .roots
            .iter()
            .any(|root| source.starts_with(root) || destination.starts_with(root))
            || roots_changed
        {
            self.search_task = None;
            self.content_results.clear();
            self.search_loading = false;
        }
        if let Some(explorer) = &self.explorer {
            explorer.update(cx, |explorer, cx| {
                explorer.did_move_entry(source, destination, cx)
            });
        }
        if roots_changed
            || self.tabs.iter().any(|reader| {
                reader.document.path.starts_with(destination) && reader.document.restore_on_launch()
            })
        {
            self.scan_workspace_roots(window, cx);
        }
        if roots_changed {
            self.remember_current_workspace(cx);
        } else {
            self.sync_workspace(cx);
            self.remember_session(cx);
        }
        self.load_incoming_file(window, cx);
        cx.notify();
    }
}
