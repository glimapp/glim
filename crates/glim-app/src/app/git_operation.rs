//! Retain serialized Git writes and protect operations on working files.
use super::*;
use crate::views::workspace::changes::Changes;
use glim_services::{git::Operation, remote::Session, work::Cancellation};
use std::path::PathBuf;

struct Running {
    root: PathBuf,
    connection: Option<String>,
    operation: Operation,
    _task: Task<()>,
}

#[derive(Default)]
struct GitOperations(Vec<Running>);
impl Global for GitOperations {}

pub(crate) fn blocked(
    root: &std::path::Path,
    remote: Option<&Session>,
    operation: &Operation,
    cx: &App,
) -> bool {
    if operation.requires_editor_lock() && workspace_editors_locked(cx) {
        return true;
    }
    cx.try_global::<GitOperations>().is_some_and(|running| {
        running.0.iter().any(|other| {
            other.root == root
                && other.connection.as_deref() == remote.map(|s| s.connection.id.as_str())
                && !operation.concurrent_with(&other.operation)
        })
    })
}

pub(crate) fn start(
    root: PathBuf,
    remote: Option<Session>,
    operation: Operation,
    source: Option<ElementId>,
    owner: WeakEntity<Changes>,
    cx: &mut App,
) -> anyhow::Result<()> {
    glim_services::git::ensure_enabled()?;
    let id = owner.entity_id();
    anyhow::ensure!(
        !blocked(&root, remote.as_ref(), &operation, cx),
        glim_services::locale::tr("Another Git operation is in progress")
    );
    let editors_locked = operation.requires_editor_lock();
    if editors_locked {
        for (view, _) in OpenWorkspaces::windows(cx) {
            if let Some(view) = view.upgrade() {
                anyhow::ensure!(
                    !view.read(cx).is_saving() && !view.read(cx).has_unsaved_in(&root),
                    glim_services::locale::tr(
                        "Save open files and wait for pending saves before running this Git action"
                    )
                );
            }
        }
        lock_workspace_editors(id, &root, false, cx)?;
    }
    let work_operation = operation.clone();
    let work_root = root.clone();
    let connection = remote.as_ref().map(|s| s.connection.id.clone());
    let completed_connection = connection.clone();
    let completed_operation = operation.clone();
    let completed_root = root.clone();
    let background_fetch = source.is_none();
    let work = cx.background_executor().spawn(async move {
        if background_fetch {
            return glim_services::git::fetch_remote_updates(
                &work_root,
                remote.as_ref(),
                &Cancellation::default(),
            );
        }
        glim_services::git::execute(
            &work_root,
            remote.as_ref(),
            &work_operation,
            &Cancellation::default(),
        )
        .map(|_| false)
    });
    let task = cx.spawn(async move |cx| {
        let result = work.await;
        let _ = cx.update(|cx| {
            if editors_locked {
                finish_trash_operation(id, None, Default::default(), cx);
            }
            cx.global_mut::<GitOperations>().0.retain(|job| {
                !(job.root == completed_root
                    && job.connection == completed_connection
                    && job.operation == completed_operation)
            });
            let _ = owner.update(cx, |view, cx| {
                if let Some(source) = source {
                    view.operation_finished(
                        completed_root,
                        completed_connection,
                        source,
                        &completed_operation,
                        result.map(|_| ()),
                        cx,
                    );
                } else {
                    match result {
                        Ok(true) => {
                            view.remote_updates_fetched(completed_root, completed_connection, cx)
                        }
                        Ok(false) => {}
                        Err(error) => log::debug!("Background Git fetch failed: {error:#}"),
                    }
                }
            });
            if !background_fetch {
                cx.refresh_windows();
            }
        });
    });
    cx.default_global::<GitOperations>().0.push(Running {
        root,
        connection,
        operation,
        _task: task,
    });
    if !background_fetch {
        cx.refresh_windows();
    }
    Ok(())
}
