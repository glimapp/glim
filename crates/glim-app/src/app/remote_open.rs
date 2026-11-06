//! Serialized external SSH handoffs into the existing workspace lifecycle.
use super::remote::{Connections, persist};
use glim_services::{
    integrations::codex::RemoteOpen, preferences::RecentWorkspace, remote::RemoteWorkspace,
};
use gpui_kit::*;

#[derive(Default)]
struct Requests {
    open: Option<Task<()>>,
    upgrade: Option<Task<()>>,
}
impl Global for Requests {}

pub fn init(cx: &mut App) {
    cx.set_global(Requests::default());
    let work = cx
        .background_executor()
        .spawn(async { glim_services::integrations::upgrade_codex_handlers() });
    cx.global_mut::<Requests>().upgrade = Some(cx.spawn(async move |_| {
        if let Err(error) = work.await {
            log::warn!("Cannot upgrade Codex file handlers: {error:#}");
        }
    }));
}

pub fn receive(url: &url::Url, cx: &mut App) -> bool {
    if url.scheme() != "glim" || url.host_str() != Some("open-remote") {
        return false;
    }
    let url = url.clone();
    let previous = cx.global_mut::<Requests>().open.take();
    let task = cx.spawn(async move |cx| {
        if let Some(previous) = previous {
            previous.await;
        }
        let request = cx
            .background_executor()
            .spawn(async move { RemoteOpen::from_url(&url) })
            .await;
        cx.update(|cx| match request {
            Ok(request) => open(request, cx),
            Err(error) => {
                log::error!("Cannot open SSH request: {error:#}");
                for window in cx.windows() {
                    let _ = window.update(cx, |_, window, cx| {
                        use gpui_kit::component::WindowExt;
                        window.push_notification(error.to_string(), cx);
                    });
                }
            }
        });
    });
    cx.global_mut::<Requests>().open = Some(task);
    true
}

fn open(mut request: RemoteOpen, cx: &mut App) {
    let catalog = &mut cx.global_mut::<Connections>().catalog;
    if let Some(existing) = catalog.connections.iter().find(|connection| {
        connection.host == request.connection.host
            && connection.user == request.connection.user
            && connection.port == request.connection.port
            && connection.identity == request.connection.identity
    }) {
        request.connection = existing.clone();
    } else {
        catalog.connections.push(request.connection.clone());
    }
    let roots = vec![request.root];
    let id = glim_services::remote::workspace_id(&request.connection.id, &roots);
    catalog.workspaces.insert(
        id.clone(),
        RemoteWorkspace {
            connection: request.connection.id,
            roots: roots.clone(),
        },
    );
    persist(cx);
    let recent = cx
        .global::<super::RecentWorkspaces>()
        .0
        .iter()
        .find(|workspace| workspace.id == id)
        .cloned()
        .unwrap_or_else(|| RecentWorkspace {
            id: id.clone(),
            folders: roots,
            name: None,
        });
    let mut windows = super::OpenWorkspaces::windows(cx);
    let active = cx.active_window();
    windows.sort_by_key(|(view, handle)| {
        (
            view.upgrade()
                .is_some_and(|view| view.read(cx).workspace_descriptor().id == id),
            Some(*handle) == active,
        )
    });
    let destination = windows.into_iter().rev().find_map(|(view, handle)| {
        let view = view.upgrade()?;
        let host = view.read(cx).workspace_window.as_ref()?.upgrade()?;
        Some((host, handle))
    });
    if let Some((host, handle)) = destination {
        let _ = handle.update(cx, |_, window, cx| {
            host.update(cx, |host, cx| host.open(recent, window, cx));
            let view = host.read(cx).slots[host.read(cx).active].view.clone();
            view.update(cx, |view, cx| {
                view.open_external_remote(request.file, window, cx)
            });
            window.activate_window();
        });
        cx.activate(true);
    } else {
        super::open_recent_workspace(recent, cx);
        for (view, handle) in super::OpenWorkspaces::windows(cx) {
            let Some(view) = view.upgrade() else { continue };
            if view.read(cx).workspace_descriptor().id == id {
                let _ = handle.update(cx, |_, window, cx| {
                    view.update(cx, |view, cx| {
                        view.open_external_remote(request.file, window, cx)
                    });
                });
                break;
            }
        }
    }
}
