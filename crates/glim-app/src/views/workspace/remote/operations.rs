use super::*;
use gpui_kit::component::{WindowExt, dialog::DialogButtonProps};
use std::path::Path;

#[derive(Clone, Copy)]
pub(crate) enum FileAction {
    NewFile,
    NewFolder,
    Rename,
    Move,
}
impl FileAction {
    fn telemetry_action(self) -> &'static str {
        match self {
            Self::NewFile => "new_file",
            Self::NewFolder => "new_folder",
            Self::Rename => "rename",
            Self::Move => "move",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::NewFile => "New File",
            Self::NewFolder => "New Folder…",
            Self::Rename => "Rename…",
            Self::Move => "Move to Folder…",
        }
    }
}

impl Workspace {
    pub(crate) fn matches_remote(&self, remote: &Session) -> bool {
        self.remote.as_ref().is_some_and(|session| {
            session.connection.id == remote.connection.id
                && session.connection.address() == remote.connection.address()
        })
    }

    pub(in crate::views::workspace) fn remote_file_dialog(
        &mut self,
        action: FileAction,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.remote.is_none()
            || self.loading
            || self.remote_restore.is_some()
            || self.remote_disconnected
            || self.picking
            || crate::app::workspace_editors_locked(cx)
        {
            return;
        }
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            let initial = match action {
                FileAction::Rename => path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                FileAction::Move => path
                    .parent()
                    .unwrap_or(Path::new("/"))
                    .display()
                    .to_string(),
                FileAction::NewFile => "untitled.txt".into(),
                FileAction::NewFolder => tr("Untitled Folder"),
            };
            input.set_value(initial, window, cx);
            input
        });
        let validation = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
        let owner = cx.entity().downgrade();
        let cancel_owner = owner.clone();
        let close_owner = owner.clone();
        self.picking = true;
        self.record_workspace_action(
            "remote_file_dialog",
            [(
                "file_action",
                glim_services::telemetry::text(action.telemetry_action()),
            )],
        );
        window.open_dialog(cx, move |dialog, _, cx| {
            let validation = validation.clone();
            let submit = owner.clone();
            let cancel = cancel_owner.clone();
            let close = close_owner.clone();
            let input = input.clone();
            let path = path.clone();
            let cancelled_path = path.clone();
            let closed_path = path.clone();
            let message = validation.borrow().clone();
            dialog
                .title(tr(action.label()))
                .width(px(420.))
                .child(Input::new(&input).aria_label(tr(match action {
                    FileAction::Move => "Destination folder",
                    _ => "Name",
                })))
                .when_some(message, |dialog, message| {
                    dialog.child(div().text_sm().text_color(cx.theme().danger).child(message))
                })
                .footer(
                    h_flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("remote-operation-cancel")
                                .label(tr("Cancel"))
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("remote-operation-confirm")
                                .primary()
                                .label(tr(match action {
                                    FileAction::Move => "Move Here",
                                    _ => "Save",
                                }))
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(
                                        Box::new(gpui_kit::component::dialog::Confirm {
                                            secondary: false,
                                        }),
                                        cx,
                                    )
                                }),
                        ),
                )
                .button_props(DialogButtonProps::default().cancel_text(tr("Cancel")))
                .on_ok(move |_, window, cx| {
                    let value = input.read(cx).value().to_string();
                    submit
                        .update(cx, |view, cx| {
                            let value = value.trim();
                            let destination = match action {
                                FileAction::Move => {
                                    let directory = PathBuf::from(value);
                                    if !directory.is_absolute() {
                                        *validation.borrow_mut() =
                                            Some(tr("Enter an absolute remote folder path"));
                                        window.refresh();
                                        return false;
                                    }
                                    directory.join(path.file_name().unwrap_or_default())
                                }
                                _ => {
                                    if value.is_empty()
                                        || value.contains('/')
                                        || value.contains('\0')
                                        || value == "."
                                        || value == ".."
                                    {
                                        *validation.borrow_mut() =
                                            Some(tr("Enter a filename without slashes"));
                                        window.refresh();
                                        return false;
                                    }
                                    match action {
                                        FileAction::Rename => {
                                            path.parent().unwrap_or(Path::new("/")).join(value)
                                        }
                                        _ => path.join(value),
                                    }
                                }
                            };
                            view.picking = false;
                            view.run_remote_file_operation(
                                action,
                                path.clone(),
                                destination,
                                window,
                                cx,
                            );
                            true
                        })
                        .unwrap_or(true)
                })
                .on_cancel(move |_, _, cx| {
                    let _ = cancel.update(cx, |view, cx| {
                        if view.picking {
                            glim_services::telemetry::file_operation_cancelled(
                                action.telemetry_action(),
                                "ssh",
                                &cancelled_path,
                            );
                        }
                        view.picking = false;
                        cx.notify();
                    });
                    true
                })
                .on_close(move |_, _, cx| {
                    let _ = close.update(cx, |view, cx| {
                        if view.picking {
                            glim_services::telemetry::file_operation_cancelled(
                                action.telemetry_action(),
                                "ssh",
                                &closed_path,
                            );
                        }
                        view.picking = false;
                        cx.notify();
                    });
                })
        });
    }

    pub(in crate::views::workspace) fn run_remote_file_operation(
        &mut self,
        action: FileAction,
        source: PathBuf,
        destination: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(remote) = self.remote.clone() else {
            return;
        };
        if self.confirm_task.is_some()
            || self.remote_disconnected
            || self.loading
            || self.remote_restore.is_some()
        {
            return;
        }
        let (cancel, guard) = Cancellation::task();
        self.confirm_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let worker = remote.clone();
            let source_copy = source.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let started = std::time::Instant::now();
                    let result = match action {
                        FileAction::NewFile => worker.create_entry(&destination, false, &cancel),
                        FileAction::NewFolder => worker.create_entry(&destination, true, &cancel),
                        FileAction::Rename | FileAction::Move => {
                            worker.move_entry(&source_copy, &destination, &cancel)
                        }
                    };
                    glim_services::telemetry::file_operation(
                        action.telemetry_action(),
                        "ssh",
                        if matches!(action, FileAction::NewFile | FileAction::NewFolder) {
                            &destination
                        } else {
                            &source_copy
                        },
                        started,
                        &result,
                    );
                    result
                })
                .await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.confirm_task = None;
                match result {
                    Ok(destination) => {
                        let moved = matches!(action, FileAction::Rename | FileAction::Move)
                            .then_some(source);
                        view.did_remote_file_operation(
                            moved.as_deref(),
                            Some(&destination),
                            false,
                            window,
                            cx,
                        );
                        crate::app::notify_remote_file_operation(
                            cx.entity_id(),
                            remote,
                            moved,
                            Some(destination),
                            false,
                            cx,
                        );
                    }
                    Err(error) => {
                        view.error = Some(format!("{error:#}").into());
                        cx.notify();
                    }
                }
            });
        }));
    }

    pub(in crate::views::workspace) fn trash_remote(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(remote) = self.remote.clone() else {
            return;
        };
        if self.confirm_task.is_some()
            || self.remote_disconnected
            || self.loading
            || self.remote_restore.is_some()
        {
            return;
        }
        let open = self
            .tabs
            .iter()
            .filter(|reader| !reader.document.trashed && reader.document.path.starts_with(&path))
            .filter_map(|reader| {
                remote
                    .cache_path(&reader.document.path)
                    .ok()
                    .map(|cached| (reader.document.path.clone(), cached))
            })
            .collect::<Vec<_>>();
        let (cancel, guard) = Cancellation::task();
        self.confirm_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let worker = remote.clone();
            let source = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let started = std::time::Instant::now();
                    let result = (|| {
                        let mut retained = std::collections::HashMap::new();
                        for (path, cached) in open {
                            if let Some(file) = glim_services::files::RetainedFile::open(&cached)? {
                                retained.insert(path, file);
                            }
                        }
                        worker.trash_entry(&source, &cancel)?;
                        Ok::<_, anyhow::Error>(retained)
                    })();
                    glim_services::telemetry::file_operation(
                        "trash", "ssh", &source, started, &result,
                    );
                    result
                })
                .await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.confirm_task = None;
                match result {
                    Ok(retained) => {
                        view.did_trash(&path, &retained, window, cx);
                        view.did_remote_file_operation(Some(&path), None, true, window, cx);
                        crate::app::notify_remote_file_operation(
                            cx.entity_id(),
                            remote,
                            Some(path),
                            None,
                            true,
                            cx,
                        );
                    }
                    Err(error) => {
                        view.error = Some(format!("{error:#}").into());
                        cx.notify();
                    }
                }
            });
        }));
    }

    pub(crate) fn did_remote_file_operation(
        &mut self,
        source: Option<&Path>,
        destination: Option<&Path>,
        trashed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(source) = source {
            if trashed {
                let retained = std::collections::HashMap::new();
                self.did_trash(source, &retained, window, cx);
            } else if let Some(destination) = destination {
                let relocate = |path: &PathBuf| {
                    path.strip_prefix(source)
                        .map(|suffix| destination.join(suffix))
                        .unwrap_or_else(|_| path.clone())
                };
                for reader in &mut self.tabs {
                    if !reader.document.trashed && reader.document.path.starts_with(source) {
                        reader.document.path = relocate(&reader.document.path);
                        reader.document.remote_version = None;
                    }
                }
                self.roots = self.roots.iter().map(relocate).collect();
                self.root = self.roots.first().cloned();
                self.incoming_files = self.incoming_files.iter().map(relocate).collect();
                for target in &mut self.history {
                    target.path = relocate(&target.path);
                }
                if let Some(explorer) = &self.explorer {
                    explorer.update(cx, |explorer, cx| {
                        explorer.did_move_entry(source, destination, cx)
                    });
                }
                self.remember_remote(cx);
            }
        }
        self.search_task = None;
        self.results.clear();
        self.content_results.clear();
        self.remember_session(cx);
        self.refresh(window, cx);
        if source.is_none()
            && let (Some(destination), Some(explorer)) = (destination, &self.explorer)
        {
            explorer.update(cx, |explorer, cx| {
                explorer.reveal_path(destination.to_path_buf(), cx)
            });
        }
        cx.notify();
    }
}
