use super::{Workspace, loading::Content};
use glim_services::watch::{Changes, WorkspaceWatch};
use gpui_kit::*;
use std::time::Duration;

impl Workspace {
    pub(super) fn start_watching(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.watch_task = Some(cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                if view
                    .update_in(cx, |view, window, cx| {
                        if let Some(watch) = &view.watch {
                            let changes = watch.take_changes();
                            if !changes.is_empty() {
                                if let Some(explorer) = &view.explorer {
                                    explorer.update(cx, |v, cx| v.refresh(cx));
                                }
                                view.pending_refresh.merge(changes);
                            }
                        }
                        view.start_pending_refresh(window, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
    pub(super) fn watch_file_parent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let parents = self
            .tabs
            .iter()
            .filter(|r| r.document.draft_directory.is_none())
            .filter_map(|r| r.document.path.parent().map(ToOwned::to_owned))
            .collect::<std::collections::BTreeSet<_>>();
        let work = cx.background_executor().spawn(async move {
            let mut parents = parents.into_iter();
            let parent = parents
                .next()
                .ok_or_else(|| anyhow::anyhow!("No open files"))?;
            let mut watch = WorkspaceWatch::new(&parent)?;
            for parent in parents {
                watch.add_path(&parent)?;
            }
            Ok::<_, anyhow::Error>(watch)
        });
        self.watch_setup = Some(cx.spawn_in(window, async move |view, cx| {
            let result = work.await;
            let _ = view.update_in(cx, |view, window, cx| {
                if let Ok(watch) = result {
                    view.watch = Some(watch);
                    view.start_watching(window, cx);
                }
            });
        }));
    }
    pub(super) fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_files(window, cx);
    }

    fn refresh_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(explorer) = &self.explorer {
            explorer.update(cx, |v, cx| v.refresh(cx));
        }
        self.refresh_readers(true, Changes::all(), window, cx);
    }

    fn refresh_readers(
        &mut self,
        force: bool,
        changes: Changes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_refresh.merge(changes);
        self.force_refresh |= force;
        self.start_pending_refresh(window, cx);
    }

    fn start_pending_refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_refresh.is_empty()
            || self.loading
            || self.refresh_task.is_some()
            || self.watch_scan_task.is_some()
            || self.is_saving()
            || self.tabs.iter().any(|reader| reader.refresh_busy(cx))
        {
            return;
        }
        let changes = std::mem::take(&mut self.pending_refresh);
        let force = std::mem::take(&mut self.force_refresh);
        for reader in &mut self.tabs {
            reader.refresh_dependencies(cx);
        }
        let mut targets = self
            .tabs
            .iter()
            .filter_map(|reader| {
                Some((
                    reader.refresh_version(cx),
                    reader.refresh_request(force, cx)?,
                ))
            })
            .collect::<Vec<_>>();
        if let Some(active) = self
            .active
            .and_then(|i| self.tabs.get(i))
            .map(|r| r.refresh_version(cx))
        {
            targets.sort_by_key(|(version, _)| *version != active);
        }
        let changes = std::sync::Arc::new(changes);
        let (cancel, guard) = glim_services::work::Cancellation::task();
        self.refresh_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            for (version, request) in targets {
                let cancel = cancel.clone();
                let work_changes = changes.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { request.read(&work_changes, &cancel) })
                    .await;
                if view
                    .update_in(cx, |view, window, cx| {
                        let Some(index) = view
                            .tabs
                            .iter()
                            .position(|r| r.refresh_version(cx) == version)
                        else {
                            view.pending_refresh.merge((*changes).clone());
                            return;
                        };
                        if view.tabs[index].refresh_busy(cx) {
                            view.pending_refresh.merge((*changes).clone());
                            return;
                        }
                        if view.tabs[index].is_dirty() || view.is_saving() {
                            return;
                        }
                        let content = match result {
                            Ok(Some(content)) => content,
                            Ok(None) => return,
                            Err(error) => {
                                // Keep the last readable view when a replacement cannot be prepared.
                                if !error.is::<glim_services::work::Cancelled>() {
                                    view.error = Some(
                                        format!(
                                            "Cannot refresh {}: {error:#}",
                                            view.tabs[index].document.path.display()
                                        )
                                        .into(),
                                    );
                                    cx.notify();
                                }
                                return;
                            }
                        };
                        if matches!(content, Content::Sqlite(_))
                            && view.tabs[index].refresh_sqlite(cx)
                        {
                            return;
                        }
                        if content.matches_snapshot(&view.tabs[index].document.snapshot)
                            && !view.tabs[index].is_unavailable()
                        {
                            return;
                        }
                        let mut reader =
                            crate::views::reader::Reader::from_content(content, window, cx);
                        reader.inherit_view(&view.tabs[index], cx);
                        reader.set_word_wrap(view.word_wrap, window, cx);
                        reader.set_editing_locked(view.editing_locked, cx);
                        view.tabs[index] = reader;
                        view.observe_autosave(window, cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
            let _ = view.update(cx, |view, _| {
                view.refresh_task = None;
            });
        }));
    }
}
