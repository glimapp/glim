//! Holds weak registrations from window hosts and filters out closed windows.
use crate::views::workspace::Workspace;
use gpui_kit::{AnyWindowHandle, App, Entity, Global, WeakEntity, WindowId};

type Registration = (WeakEntity<Workspace>, AnyWindowHandle);

#[derive(Default)]
pub(crate) struct OpenWorkspaces(Vec<Registration>);

impl Global for OpenWorkspaces {}

impl OpenWorkspaces {
    pub(crate) fn register(view: &Entity<Workspace>, handle: AnyWindowHandle, cx: &mut App) {
        let registered = &mut cx.global_mut::<Self>().0;
        registered.retain(|(view, _)| view.upgrade().is_some());
        registered.push((view.downgrade(), handle));
    }

    pub(super) fn forget_window(window: WindowId, cx: &mut App) {
        cx.global_mut::<Self>()
            .0
            .retain(|(_, handle)| handle.window_id() != window);
    }

    pub(super) fn registered(cx: &App) -> &[Registration] {
        cx.try_global::<Self>().map_or(&[], |registry| &registry.0)
    }

    pub(super) fn windows(cx: &App) -> Vec<Registration> {
        let windows = cx.windows();
        Self::registered(cx)
            .iter()
            .filter(|(view, handle)| view.upgrade().is_some() && windows.contains(handle))
            .cloned()
            .collect()
    }
}
