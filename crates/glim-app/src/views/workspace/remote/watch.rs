use super::*;

impl Workspace {
    pub(in crate::views::workspace) fn start_remote_watching(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (cancel, guard) = Cancellation::task();
        self.watch_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let mut directory_cursor = 0usize;
            let mut file_cursor = 0usize;
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(
                        glim_services::limits::REMOTE_REFRESH_INTERVAL_MS,
                    ))
                    .await;
                let request = view.update_in(cx, |view, window, cx| {
                    let session = view.remote.clone()?;
                    if view.loading
                        || view.remote_restore.is_some()
                        || view.is_saving()
                        || !view.is_visible_workspace(cx)
                        || !window.is_window_active()
                        || !session.is_idle()
                    {
                        return None;
                    }
                    let paths = view
                        .explorer
                        .as_ref()
                        .map(|explorer| explorer.read(cx).remote_refresh_paths())
                        .unwrap_or_default();
                    let directories = rotating_batch(
                        &paths,
                        &mut directory_cursor,
                        glim_services::limits::REMOTE_REFRESH_BATCH,
                    );
                    let paths = view
                        .tabs
                        .iter()
                        .filter(|reader| {
                            reader.document.remote.is_some() && !reader.refresh_busy(cx)
                        })
                        .map(|reader| reader.document.path.clone())
                        .collect::<Vec<_>>();
                    let mut selected = rotating_batch(
                        &paths,
                        &mut file_cursor,
                        glim_services::limits::REMOTE_REFRESH_BATCH,
                    );
                    if let Some(reader) = view.active.and_then(|index| view.tabs.get(index)) {
                        selected.retain(|path| *path != reader.document.path);
                        selected.insert(0, reader.document.path.clone());
                    }
                    let files = selected
                        .into_iter()
                        .filter_map(|path| {
                            let reader = view
                                .tabs
                                .iter()
                                .find(|reader| reader.document.path == path)?;
                            Some((
                                path,
                                reader.refresh_version(cx),
                                reader.document.remote_version.clone(),
                            ))
                        })
                        .collect::<Vec<_>>();
                    Some((session, directories, files))
                });
                let Ok(request) = request else { break };
                let Some((session, directories, files)) = request else {
                    continue;
                };
                for (path, version, previous) in files {
                    let worker = session.clone();
                    let requested = path.clone();
                    let token = cancel.clone();
                    let work = cx.background_executor().spawn(async move {
                        if !worker.is_idle() {
                            return Ok(None);
                        }
                        if let Some(previous) = previous
                            && !worker.has_changed(&previous, &token)?
                        {
                            return Ok(None);
                        }
                        worker.read_content(&requested, &token).map(Some)
                    });
                    let result = work.await;
                    if view
                        .update_in(cx, |view, window, cx| {
                            let Some(index) = view.tabs.iter().position(|reader| {
                                reader.document.path == path
                                    && reader.refresh_version(cx) == version
                            }) else {
                                return;
                            };
                            if view.loading || view.tabs[index].refresh_busy(cx) {
                                return;
                            }
                            match result {
                                Ok(Some(content)) => {
                                    let on_failure = view.refresh_failure_handler(version, cx);
                                    if view.tabs[index].refresh_in_place(&content, on_failure, cx) {
                                        view.tabs[index].document.remote_version =
                                            session.loaded_version(&path);
                                        cx.notify();
                                        return;
                                    }
                                    let mut reader = crate::views::reader::Reader::from_content(
                                        content, window, cx,
                                    );
                                    reader.inherit_view(&view.tabs[index], cx);
                                    reader.set_remote(session.clone(), path, cx);
                                    reader.set_word_wrap(view.word_wrap, window, cx);
                                    if view.active != Some(index) {
                                        reader.deactivate(cx);
                                    }
                                    view.tabs[index] = reader;
                                    cx.notify();
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    log::debug!(
                                        "Remote refresh kept {}: {error:#}",
                                        path.display()
                                    );
                                    view.remote_disconnected = !session.is_connected();
                                }
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                    if !session.is_connected() {
                        break;
                    }
                }
                for path in directories {
                    let worker = session.clone();
                    let requested = path.clone();
                    let token = cancel.clone();
                    let work = cx.background_executor().spawn(async move {
                        if !worker.is_idle() {
                            return Ok(None);
                        }
                        worker.list_entries(&requested, &token).map(Some)
                    });
                    let result = work.await;
                    if view
                        .update(cx, |view, cx| {
                            if let Ok(Some(entries)) = result
                                && let Some(explorer) = &view.explorer
                            {
                                explorer.update(cx, |explorer, cx| {
                                    explorer.apply_remote_listing(path, entries, cx)
                                });
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                    if !session.is_connected() {
                        break;
                    }
                }
            }
        }));
    }
}

fn rotating_batch(paths: &[PathBuf], cursor: &mut usize, count: usize) -> Vec<PathBuf> {
    if paths.is_empty() {
        return Vec::new();
    }
    let batch = (0..count.min(paths.len()))
        .map(|offset| paths[(*cursor + offset) % paths.len()].clone())
        .collect::<Vec<_>>();
    *cursor = (*cursor + batch.len()) % paths.len();
    batch
}
