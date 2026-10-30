use glim_core::update::WorkspaceSession;
use glim_services::preferences::RecentWorkspace;
use gpui_kit::*;
use std::collections::BTreeMap;

pub(super) struct SavedWorkspaces {
    pub sessions: BTreeMap<String, WorkspaceSession>,
    pub write: Option<Task<()>>,
}
impl Global for SavedWorkspaces {}

pub(crate) fn remember(mut session: WorkspaceSession, cx: &mut App) {
    let Some(id) = session.workspace_id.clone() else {
        return;
    };
    if !cx.has_global::<SavedWorkspaces>() {
        return;
    }
    session.window_group = None;
    let saved = cx.global_mut::<SavedWorkspaces>();
    if saved.sessions.get(&id) == Some(&session) {
        return;
    }
    saved.sessions.insert(id, session);
    let sessions = saved.sessions.clone();
    let previous = saved.write.take();
    let task = cx.spawn(async move |cx| {
        if let Some(previous) = previous {
            previous.await;
        }
        if let Err(error) = cx
            .background_executor()
            .spawn(async move { glim_services::preferences::save_workspace_sessions(&sessions) })
            .await
        {
            log::error!("Cannot remember workspace files: {error:#}");
        }
    });
    cx.global_mut::<SavedWorkspaces>().write = Some(task);
}

pub(crate) fn restore(workspace: RecentWorkspace, cx: &App) -> WorkspaceSession {
    let mut session = cx
        .try_global::<SavedWorkspaces>()
        .and_then(|saved| saved.sessions.get(&workspace.id))
        .cloned()
        .unwrap_or_default();
    session.roots = workspace.folders;
    session.workspace_id = Some(workspace.id);
    session.name = workspace.name;
    session.window_group = None;
    session
}
