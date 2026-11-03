use glim_core::session::WorkspaceSession;
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
    session.window_active = false;
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
    session.window_active = false;
    session.show_home = false;
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
    restore_groups(windows, cx)
}

pub(crate) fn restore_update_windows(sessions: Vec<WorkspaceSession>, cx: &mut App) {
    let mut windows = BTreeMap::<String, Vec<WorkspaceSession>>::new();
    for (index, session) in sessions.into_iter().enumerate() {
        let group = session
            .window_group
            .clone()
            .unwrap_or_else(|| format!("restored-{index}"));
        windows.entry(group).or_default().push(session);
    }
    restore_groups(windows, cx);
}

fn restore_groups(windows: BTreeMap<String, Vec<WorkspaceSession>>, cx: &mut App) -> bool {
    let restored = !windows.is_empty();
    let was_restoring = cx.global::<SavedWorkspaces>().restoring;
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
            sessions.push(WorkspaceSession {
                show_home: true,
                ..Default::default()
            });
        }
        let active = sessions
            .iter()
            .position(|session| session.window_active)
            .unwrap_or(sessions.len() - 1);
        cx.global_mut::<SavedWorkspaces>()
            .windows
            .insert(group.clone(), sessions.clone());
        for mut session in sessions {
            session.window_group = Some(group.clone());
            super::open_restored_workspace_with_session(session, cx);
        }
        for (view, handle) in super::OpenWorkspaces::windows(cx) {
            let host = view
                .upgrade()
                .and_then(|view| view.read(cx).workspace_window.as_ref()?.upgrade());
            if let Some(host) = host
                && host.read(cx).group == group
            {
                let _ = handle.update(cx, |_, window, cx| {
                    host.update(cx, |host, cx| host.restore_selection(active, window, cx));
                });
                break;
            }
        }
    }
    cx.global_mut::<SavedWorkspaces>().restoring = was_restoring;
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

#[cfg(test)]
mod tests {
    use super::{SavedWorkspaces, restore_windows};
    use glim_core::session::WorkspaceSession;
    use glim_services::preferences::RecentWorkspace;
    use gpui_kit::TestAppContext;
    use std::collections::BTreeMap;

    #[gpui_kit::test]
    fn startup_restores_selected_workspace_and_home_without_reordering_recents(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let temp = tempfile::tempdir().unwrap();
        let roots = [temp.path().join("dev"), temp.path().join("other")];
        for root in &roots {
            std::fs::create_dir(root).unwrap();
        }
        let sessions = roots
            .iter()
            .enumerate()
            .map(|(index, root)| WorkspaceSession {
                roots: vec![root.clone()],
                workspace_id: Some(if index == 0 { "dev" } else { "other" }.into()),
                window_active: index == 0,
                show_home: index == 1,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        cx.update(|cx| {
            cx.set_global(super::super::OpenWorkspaces::default());
            cx.set_global(super::super::RecentWorkspaces(vec![
                RecentWorkspace {
                    id: "other".into(),
                    folders: vec![roots[1].clone()],
                    name: None,
                },
                RecentWorkspace {
                    id: "dev".into(),
                    folders: vec![roots[0].clone()],
                    name: None,
                },
            ]));
            cx.set_global(SavedWorkspaces {
                sessions: BTreeMap::new(),
                write: None,
                windows: BTreeMap::from([("test-window".into(), sessions)]),
                window_write: None,
                restoring: true,
            });
            assert!(restore_windows(cx));
            // Keep this fixture from persisting application state as background loads finish.
            cx.remove_global::<SavedWorkspaces>();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let (view, handle) = super::super::OpenWorkspaces::windows(cx)
                .into_iter()
                .next()
                .unwrap();
            let host = view
                .upgrade()
                .unwrap()
                .read(cx)
                .workspace_window
                .as_ref()
                .unwrap()
                .upgrade()
                .unwrap();
            let window = host.read(cx);
            assert_eq!(window.active, 0);
            assert_eq!(
                window
                    .slots
                    .iter()
                    .map(|s| s.workspace.id.as_str())
                    .collect::<Vec<_>>(),
                ["dev", "other"]
            );
            assert_eq!(
                window.slots[0].view.read(cx).workspace_descriptor().folders,
                vec![roots[0].canonicalize().unwrap()]
            );
            let active = window.slots[0].view.read(cx).update_session(cx);
            assert!(active.window_active);
            assert!(!active.show_home);
            assert_eq!(
                cx.global::<super::super::RecentWorkspaces>()
                    .0
                    .iter()
                    .map(|r| r.id.as_str())
                    .collect::<Vec<_>>(),
                ["other", "dev"]
            );
            handle
                .update(cx, |_, window, cx| {
                    host.update(cx, |host, cx| host.restore_selection(1, window, cx));
                })
                .unwrap();
            let home = host.read(cx).slots[1].view.read(cx).update_session(cx);
            assert!(home.window_active);
            assert!(home.show_home);
        });
    }
}
