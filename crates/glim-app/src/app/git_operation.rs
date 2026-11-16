//! Retain serialized Git writes and protect operations on working files.
use super::*;
use crate::views::workspace::changes::Changes;
use glim_services::{git::Operation, remote::Session, work::Cancellation};
use std::path::PathBuf;

#[derive(Default)]
struct GitOperation(Option<Task<()>>);
impl Global for GitOperation {}

pub(crate) fn busy(cx: &App) -> bool {
    cx.try_global::<GitOperation>()
        .is_some_and(|operation| operation.0.is_some())
}

pub(crate) fn start(
    root: PathBuf,
    remote: Option<Session>,
    operation: Operation,
    owner: WeakEntity<Changes>,
    cx: &mut App,
) -> anyhow::Result<()> {
    glim_services::git::ensure_enabled()?;
    let id = owner.entity_id();
    anyhow::ensure!(
        !busy(cx),
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
    let work = cx.background_executor().spawn(async move {
        glim_services::git::execute(
            &root,
            remote.as_ref(),
            &work_operation,
            &Cancellation::default(),
        )
    });
    let task = cx.spawn(async move |cx| {
        let result = work.await;
        let _ = cx.update(|cx| {
            if editors_locked {
                finish_trash_operation(id, None, Default::default(), cx);
            }
            let _ = owner.update(cx, |view, cx| {
                view.operation_finished(&operation, result, cx)
            });
            cx.global_mut::<GitOperation>().0 = None;
            cx.refresh_windows();
        });
    });
    cx.set_global(GitOperation(Some(task)));
    cx.refresh_windows();
    Ok(())
}
