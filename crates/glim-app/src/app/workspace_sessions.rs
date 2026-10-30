use glim_core::update::WorkspaceSession;
use glim_services::preferences::RecentWorkspace;
use gpui_kit::*;
use std::collections::BTreeMap;

pub(crate) struct SavedWorkspaces {
    pub sessions: BTreeMap<String, WorkspaceSession>,
    pub write: Option<Task<()>>,
    pub windows: BTreeMap<String, Vec<WorkspaceSession>>,
    pub window_write: Option<Task<()>>,
    pub restoring: bool,
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

pub(crate) fn remember_window(group: String, sessions: Vec<WorkspaceSession>, cx: &mut App) {
    if !cx.has_global::<SavedWorkspaces>() {
        return;
    }
    let saved = cx.global_mut::<SavedWorkspaces>();
    if saved.restoring || saved.windows.get(&group) == Some(&sessions) {
        return;
    }
    saved.windows.insert(group, sessions);
    let windows = saved.windows.clone();
    let previous = saved.window_write.take();
    let task = cx.spawn(async move |cx| {
        if let Some(previous) = previous {
            previous.await;
        }
        if let Err(error) = cx
            .background_executor()
            .spawn(async move { glim_services::preferences::save_window_sessions(&windows) })
            .await
        {
            log::error!("Cannot remember workspace windows: {error:#}");
        }
    });
    cx.global_mut::<SavedWorkspaces>().window_write = Some(task);
}

pub(crate) fn restore_windows(cx: &mut App) -> bool {
    let Some(saved) = cx.try_global::<SavedWorkspaces>() else {
        return false;
    };
    let windows = saved.windows.clone();
    let restored = !windows.is_empty();
    cx.global_mut::<SavedWorkspaces>().restoring = true;
    for (group, mut sessions) in windows {
        let exists = super::OpenWorkspaces::windows(cx)
            .into_iter()
            .any(|(view, _)| {
                view.upgrade()
                    .and_then(|view| {
                        view.read(cx)
                            .workspace_window
                            .as_ref()
                            .and_then(|host| host.upgrade())
                    })
                    .is_some_and(|host| host.read(cx).group == group)
            });
        if exists {
            continue;
        }
        if sessions.is_empty() {
            sessions.push(WorkspaceSession::default());
        }
        for mut session in sessions {
            session.window_group = Some(group.clone());
            super::open_home_workspace_with_session(session, cx);
        }
    }
    cx.global_mut::<SavedWorkspaces>().restoring = false;
    restored
}

pub(super) fn checkpoint_windows(cx: &mut App) {
    let mut groups = std::collections::HashSet::new();
    for (view, _) in super::OpenWorkspaces::windows(cx) {
        let host = view.upgrade().and_then(|view| {
            view.read(cx)
                .workspace_window
                .as_ref()
                .and_then(|host| host.upgrade())
        });
        if let Some(host) = host
            && groups.insert(host.entity_id())
        {
            host.update(cx, |host, cx| host.remember_window(cx));
        }
    }
}
