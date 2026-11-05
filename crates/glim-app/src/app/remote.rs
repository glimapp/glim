use glim_services::remote::{Catalog, Connection, Session};
use gpui_kit::*;

#[derive(Default)]
pub(crate) struct Connections {
    pub catalog: Catalog,
    pub write: Option<Task<()>>,
    pub sessions: std::collections::BTreeMap<String, Session>,
    connection_states: std::collections::BTreeMap<String, bool>,
    workspace_states: std::collections::BTreeMap<String, bool>,
    status_task: Option<Task<()>>,
}
impl Global for Connections {}

pub(crate) fn init(catalog: Catalog, cx: &mut App) {
    cx.set_global(Connections {
        catalog,
        ..Default::default()
    });
}

pub(crate) fn monitor_status(cx: &mut App) {
    let task = cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
            cx.update(|cx| {
                let catalog = &cx.global::<Connections>().catalog;
                let mut connections = catalog
                    .connections
                    .iter()
                    .map(|connection| {
                        (
                            connection.id.clone(),
                            cached_session(connection, cx).is_some(),
                        )
                    })
                    .collect::<std::collections::BTreeMap<_, _>>();
                let mut workspaces = std::collections::BTreeMap::new();
                for (view, _) in super::OpenWorkspaces::windows(cx) {
                    let Some(view) = view.upgrade() else { continue };
                    let view = view.read(cx);
                    let workspace = view.workspace_descriptor();
                    if !glim_services::remote::is_remote_id(&workspace.id) {
                        continue;
                    }
                    workspaces.insert(workspace.id, view.remote_workspace_is_connected());
                    for connection in &catalog.connections {
                        if view.remote_connection_is_live(connection) {
                            connections.insert(connection.id.clone(), true);
                        }
                    }
                }
                let state = cx.global_mut::<Connections>();
                if state.connection_states != connections || state.workspace_states != workspaces {
                    state.connection_states = connections;
                    state.workspace_states = workspaces;
                    cx.refresh_windows();
                }
            });
        }
    });
    cx.global_mut::<Connections>().status_task = Some(task);
}

pub(crate) fn workspace_is_connected(id: &str, cx: &App) -> bool {
    let state = cx.global::<Connections>();
    state.workspace_states.get(id).copied().unwrap_or_else(|| {
        state
            .catalog
            .workspaces
            .get(id)
            .is_some_and(|workspace| connection_is_connected(&workspace.connection, cx))
    })
}
pub(crate) fn connection_in_use(id: &str, cx: &App) -> bool {
    let catalog = &cx.global::<Connections>().catalog;
    super::OpenWorkspaces::windows(cx).iter().any(|(view, _)| {
        view.upgrade().is_some_and(|view| {
            let workspace_id = view.read(cx).workspace_descriptor().id;
            catalog
                .workspaces
                .get(&workspace_id)
                .is_some_and(|workspace| workspace.connection == id)
        })
    })
}

pub(crate) fn observe_connection_status<T: 'static>(cx: &mut Context<T>) -> Vec<Subscription> {
    super::OpenWorkspaces::windows(cx)
        .into_iter()
        .filter_map(|(view, _)| view.upgrade())
        .map(|view| cx.observe(&view, |_, _, cx| cx.notify()))
        .collect()
}

pub(crate) fn connection_is_connected(id: &str, cx: &App) -> bool {
    let state = cx.global::<Connections>();
    let Some(connection) = state.catalog.connections.iter().find(|item| item.id == id) else {
        return false;
    };
    if cached_session(connection, cx).is_some() {
        return true;
    }
    state.connection_states.get(id).copied().unwrap_or(false)
}

pub(crate) fn session_matches(session: &Session, connection: &Connection) -> bool {
    let current = &session.connection;
    session.is_connected()
        && current.host == connection.host
        && current.user == connection.user
        && current.port == connection.port
        && current.identity == connection.identity
}

pub(crate) fn cached_session(connection: &Connection, cx: &App) -> Option<Session> {
    cx.global::<Connections>()
        .sessions
        .get(&connection.id)
        .filter(|session| session_matches(session, connection))
        .cloned()
        .map(|mut session| {
            session.connection = connection.clone();
            session
        })
}

pub(crate) fn persist(cx: &mut App) {
    let state = cx.global_mut::<Connections>();
    let catalog = state.catalog.clone();
    let previous = state.write.take();
    let task = cx.spawn(async move |cx| {
        if let Some(previous) = previous {
            previous.await;
        }
        if let Err(error) = cx
            .background_executor()
            .spawn(async move { glim_services::remote::save_catalog(&catalog) })
            .await
        {
            log::error!("Cannot save SSH connections: {error:#}");
            cx.update(|cx| {
                for window in cx.windows() {
                    let message = glim_services::locale::tr("Cannot save SSH connections");
                    let _ = window.update(cx, |_, window, cx| {
                        use gpui_kit::component::WindowExt;
                        window.push_notification(message, cx);
                    });
                }
            });
        }
    });
    cx.global_mut::<Connections>().write = Some(task);
    cx.refresh_windows();
}

/// OpenSSH launches this executable as its askpass helper, before GPUI startup.
pub(crate) fn askpass() -> anyhow::Result<()> {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    glim_services::locale::initialize();
    let question = std::env::args().nth(1).unwrap_or_default();
    let confirmation = std::env::var("SSH_ASKPASS_PROMPT").as_deref() == Ok("confirm")
        || question.contains("yes/no");
    let script = if confirmation {
        "on run argv\nset result to display dialog (item 1 of argv) with title \"Glim · SSH\" buttons {item 2 of argv, item 3 of argv} default button 2 cancel button 1\nreturn \"yes\"\nend run"
    } else {
        "on run argv\nset result to display dialog (item 1 of argv) with title \"Glim · SSH\" default answer \"\" with hidden answer buttons {item 2 of argv, item 3 of argv} default button 2 cancel button 1\nreturn text returned of result\nend run"
    };
    let mut child = Command::new("/usr/bin/osascript")
        .args([
            "-",
            &question,
            &glim_services::locale::tr("Cancel"),
            &glim_services::locale::tr("Connect"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped AppleScript input")
        .write_all(script.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        std::process::exit(1);
    }
    std::io::stdout().write_all(&output.stdout)?;
    Ok(())
}
