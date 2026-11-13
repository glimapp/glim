//! Retain filesystem moves independently of the initiating workspace's lifetime.
use super::{OpenWorkspaces, Workspace};
use glim_core::session::SessionTab;
use glim_services::{
    documents::{Content, read_content_cancellable},
    work::Cancellation,
};
use gpui_kit::*;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct FileMove {
    destination: Option<PathBuf>,
    task: Option<Task<()>>,
}
impl Global for FileMove {}

pub(crate) fn destination(cx: &App) -> Option<&Path> {
    cx.try_global::<FileMove>()?.destination.as_deref()
}

pub(crate) fn relocated_path(path: &Path, source: &Path, destination: &Path) -> PathBuf {
    match path.strip_prefix(source) {
        Ok(suffix) if suffix.as_os_str().is_empty() => destination.to_path_buf(),
        Ok(suffix) => destination.join(suffix),
        Err(_) => path.to_path_buf(),
    }
}

pub(crate) fn start(
    source: PathBuf,
    directory: PathBuf,
    requester: WeakEntity<Workspace>,
    cx: &mut App,
) {
    start_operation(source, directory, None, requester, cx)
}

pub(crate) fn rename(
    source: PathBuf,
    name: std::ffi::OsString,
    requester: WeakEntity<Workspace>,
    cx: &mut App,
) {
    let Some(parent) = source.parent().map(Path::to_path_buf) else {
        return;
    };
    start_operation(source, parent, Some(name), requester, cx)
}

fn start_operation(
    source: PathBuf,
    directory: PathBuf,
    name: Option<std::ffi::OsString>,
    requester: WeakEntity<Workspace>,
    cx: &mut App,
) {
    let Some(requester) = requester.upgrade() else {
        return;
    };
    let windows = OpenWorkspaces::windows(cx);
    let validation = (|| {
        for (view, _) in &windows {
            if let Some(view) = view.upgrade() {
                view.read(cx).check_file_move(&source, cx)?;
            }
        }
        super::lock_workspace_editors(requester.entity_id(), &source, false, cx)
    })();
    if let Err(error) = validation {
        requester.update(cx, |view, cx| {
            view.report_recovery_error(error.to_string(), cx)
        });
        return;
    }
    requester.update(cx, |view, cx| view.set_editing_locked(true, cx));
    let tabs = windows
        .iter()
        .map(|(view, handle)| {
            let tabs = view
                .upgrade()
                .map(|view| view.read(cx).tabs_under(&source, cx))
                .unwrap_or_default();
            (view.clone(), *handle, tabs)
        })
        .collect::<Vec<_>>();
    let target = directory.clone();
    let origin = source.clone();
    let error_label = if name.is_some() {
        "Could not rename item"
    } else {
        "Could not move item"
    };
    let work = cx.background_executor().spawn(async move {
        let started = std::time::Instant::now();
        let (action, result) = if let Some(name) = name {
            (
                "rename",
                glim_services::workspace::rename_entry(&origin, &name),
            )
        } else {
            (
                "move",
                glim_services::workspace::move_entry(&origin, &target),
            )
        };
        glim_services::telemetry::file_operation(action, "local", &origin, started, &result);
        result
    });
    let task = cx.spawn(async move |cx| {
        let result = work.await;
        if let Ok(destination) = &result {
            for (view, handle, tabs) in tabs {
                for tab in tabs {
                    let (old_path, content) = match tab {
                        SessionTab::File { path, .. } => {
                            let new_path = relocated_path(&path, &source, destination);
                            let read_path = new_path.clone();
                            let content = super::reader_jobs::run(
                                cx.background_executor().clone(),
                                super::reader_jobs::JobClass::Document,
                                super::reader_jobs::Priority::Interactive,
                                Cancellation::default(),
                                move |cancel| read_content_cancellable(&read_path, cancel),
                            )
                            .await
                            .unwrap_or_else(|error| {
                                Content::Unavailable(new_path, (&error).into())
                            });
                            (path, content)
                        }
                        SessionTab::Playlist { path, shuffle } => {
                            let new_path = relocated_path(&path, &source, destination);
                            (path, Content::Native(new_path, true, shuffle))
                        }
                    };
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = view.update(cx, |view, cx| {
                            view.replace_moved_tab(&old_path, content, window, cx)
                        });
                    });
                }
            }
        }
        let _ = cx.update(|cx| {
            cx.set_global(super::WorkspaceEditorLock(false));
            cx.global_mut::<FileMove>().destination = None;
            cx.global_mut::<FileMove>().task = None;
            for (view, handle) in OpenWorkspaces::windows(cx) {
                let moved = result.as_ref().ok().cloned();
                let source = source.clone();
                let error = result
                    .as_ref()
                    .err()
                    .map(|error| format!("{}: {error:#}", glim_services::locale::tr(error_label)));
                let requester = requester.downgrade();
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = view.update(cx, |view, cx| {
                            if let Some(destination) = moved {
                                view.did_move_entry(&source, &destination, window, cx);
                            }
                            if requester.entity_id() == cx.entity_id()
                                && let Some(error) = error
                            {
                                view.report_recovery_error(error, cx);
                            }
                            view.unlock_and_refresh(window, cx);
                        });
                    });
                });
            }
            cx.refresh_windows();
        });
    });
    cx.set_global(FileMove {
        destination: Some(directory),
        task: Some(task),
    });
    cx.refresh_windows();
}
