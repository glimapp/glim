use super::workspace::Workspace;
use glim_core::update::WorkspaceSession;
use glim_services::preferences::RecentWorkspace;
use gpui_kit::*;

pub(crate) struct WorkspaceSlot {
    pub view: Entity<Workspace>,
    pub workspace: RecentWorkspace,
}

/// Owns independent reader/editor lifetimes for every workspace in one window.
pub(crate) struct WorkspaceWindow {
    pub slots: Vec<WorkspaceSlot>,
    pub active: usize,
    pub group: String,
    closing: bool,
}

impl WorkspaceWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            slots: Vec::new(),
            active: 0,
            closing: false,
            group: format!(
                "window-{}-{:?}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                cx.entity_id()
            ),
        }
    }

    pub fn append(
        &mut self,
        path: Option<std::path::PathBuf>,
        session: Option<WorkspaceSession>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Workspace> {
        if let Some(group) = session.as_ref().and_then(|s| s.window_group.clone()) {
            self.group = group;
        }
        if let Some(slot) = self.slots.get_mut(self.active) {
            slot.workspace = slot.view.read(cx).workspace_descriptor();
            if slot.workspace.folders.is_empty()
                && slot.workspace.name.is_none()
                && slot.view.read(cx).telemetry_tab_count() > 0
            {
                slot.workspace.name = Some(glim_services::locale::tr("Workspace"));
            }
            slot.view
                .update(cx, |view, cx| view.suspend_workspace(window, cx));
        }
        let view = cx.new(|cx| Workspace::new(None, window, cx));
        let host = cx.entity().downgrade();
        view.update(cx, |view, cx| {
            view.workspace_window = Some(host.clone());
            if let Some(session) = session {
                view.restore_update_session(session, window, cx);
            } else if let Some(path) = path {
                view.open_workspace_path(path, window, cx);
            }
        });
        let workspace = view.read(cx).workspace_descriptor();
        self.slots.push(WorkspaceSlot {
            view: view.clone(),
            workspace,
        });
        self.active = self.slots.len() - 1;
        crate::app::register_workspace(&view, window.window_handle(), cx);
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            if crate::app::updates::intercept_last_window_close(cx) {
                return false;
            }
            let _ = weak.update(cx, |host, cx| host.request_close(window, cx));
            false
        });
        cx.notify();
        view
    }

    pub fn select(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.slots.iter().position(|slot| slot.workspace.id == id) else {
            return;
        };
        if self.active != index {
            self.slots[self.active]
                .view
                .update(cx, |view, cx| view.suspend_workspace(window, cx));
            self.active = index;
        }
        self.slots[index]
            .view
            .update(cx, |view, cx| view.resume_workspace(window, cx));
        cx.notify();
    }

    pub fn open(
        &mut self,
        workspace: RecentWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self
            .slots
            .iter()
            .find(|slot| {
                slot.workspace.id == workspace.id || slot.workspace.folders == workspace.folders
            })
            .map(|slot| slot.workspace.id.clone())
        {
            self.select(&id, window, cx);
            return;
        }
        self.append(
            None,
            Some(WorkspaceSession {
                roots: workspace.folders,
                workspace_id: Some(workspace.id),
                name: workspace.name,
                ..Default::default()
            }),
            window,
            cx,
        );
    }

    pub fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::app::updates::intercept_last_window_close(cx) {
            return;
        }
        if !self.closing {
            for slot in &self.slots {
                slot.view.update(cx, |view, _| view.reset_quit_approval());
            }
            self.closing = true;
        }
        let next = self
            .slots
            .iter()
            .find(|slot| !slot.view.read(cx).quit_is_approved())
            .map(|slot| (slot.workspace.id.clone(), slot.view.clone()));
        if let Some((id, view)) = next {
            self.select(&id, window, cx);
            view.update(cx, |view, cx| {
                view.confirm_workspace_window_close(window, cx)
            });
        } else {
            for slot in &self.slots {
                slot.view.update(cx, |view, cx| view.commit_quit(cx));
            }
            window.remove_window();
        }
    }

    pub fn cancel_close(&mut self, cx: &mut Context<Self>) {
        self.closing = false;
        cx.notify();
    }
}

impl Render for WorkspaceWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .children(self.slots.get(self.active).map(|slot| slot.view.clone()))
    }
}
