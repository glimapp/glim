use super::{Page, Workspace};
use glim_services::preferences::RecentWorkspace;
use gpui_kit::{
    component::{
        ActiveTheme, Icon,
        button::{Button, ButtonCustomVariant, ButtonVariants},
        h_flex,
        input::{Input, InputEvent, InputState},
        menu::{ContextMenuExt, PopupMenuItem},
        v_flex,
    },
    *,
};

impl Workspace {
    pub(crate) fn workspace_descriptor(&self) -> RecentWorkspace {
        RecentWorkspace {
            id: self.recent_id.clone(),
            folders: self.restoring_roots.as_ref().unwrap_or(&self.roots).clone(),
            name: self.workspace_name.clone(),
        }
    }

    pub(crate) fn open_workspace_path(
        &mut self,
        path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.page = Page::Document;
        self.open_path(path, window, cx);
    }

    pub(crate) fn suspend_workspace(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.set_file_fullscreen(false, window);
        self.flush_usage();
        for reader in &self.tabs {
            reader.hide_native(cx);
        }
    }

    pub(crate) fn resume_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = Page::Document;
        window.focus(&self.focus, cx);
        self.reveal_active_file(cx);
        cx.notify();
    }

    pub(crate) fn is_visible_workspace(&self, cx: &App) -> bool {
        self.workspace_window
            .as_ref()
            .and_then(|host| host.upgrade())
            .is_none_or(|host| {
                host.read(cx)
                    .slots
                    .get(host.read(cx).active)
                    .is_some_and(|slot| slot.workspace.id == self.recent_id)
            })
    }

    pub(super) fn sync_workspace(&self, cx: &mut Context<Self>) {
        let workspace = self.workspace_descriptor();
        let entity_id = cx.entity_id();
        if let Some(host) = self.workspace_window.clone() {
            cx.defer(move |cx| {
                let _ = host.update(cx, |host, cx| {
                    if let Some(slot) = host
                        .slots
                        .iter_mut()
                        .find(|slot| slot.view.entity_id() == entity_id)
                    {
                        slot.workspace = workspace.clone();
                    }
                    cx.notify();
                });
            });
        }
    }

    pub(crate) fn open_recent_here(
        &mut self,
        workspace: RecentWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if crate::app::updates::busy(cx) {
            return;
        }
        if let Some(host) = self.workspace_window.clone() {
            window.defer(cx, move |window, cx| {
                let _ = host.update(cx, |host, cx| host.open(workspace, window, cx));
            });
        }
    }

    pub(super) fn select_workspace(&self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(host) = self.workspace_window.clone() {
            window.defer(cx, move |window, cx| {
                let _ = host.update(cx, |host, cx| host.select(&id, window, cx));
            });
        }
    }

    pub(super) fn begin_workspace_rename(
        &mut self,
        workspace: RecentWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace_rename = None;
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(workspace.title(), window, cx);
            state
        });
        let subscription = cx.subscribe_in(&input, window, |view, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                view.finish_workspace_rename(window, cx);
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        self.workspace_rename = Some((workspace.id, input, subscription));
        cx.notify();
    }

    fn finish_workspace_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, input, _)) = self.workspace_rename.take() else {
            return;
        };
        let name = input.read(cx).value().trim().to_owned();
        if !name.is_empty() {
            if id == self.recent_id {
                self.workspace_name = Some(name);
                self.remember_current_workspace(cx);
            } else if let Some(host) = self.workspace_window.clone() {
                window.defer(cx, move |_, cx| {
                    let _ = host.update(cx, |host, cx| {
                        if let Some(slot) = host.slots.iter().find(|slot| slot.workspace.id == id) {
                            slot.view.update(cx, |view, cx| {
                                view.workspace_name = Some(name);
                                view.remember_current_workspace(cx);
                            });
                        }
                    });
                });
            }
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn remember_current_workspace(&mut self, cx: &mut Context<Self>) {
        if self.workspace_name.is_none() {
            self.workspace_name = cx
                .try_global::<crate::app::RecentWorkspaces>()
                .and_then(|history| {
                    history
                        .0
                        .iter()
                        .find(|workspace| workspace.folders == self.roots)
                })
                .and_then(|workspace| workspace.name.clone());
        }
        self.sync_workspace(cx);
        crate::app::remember_named_workspace(self.workspace_descriptor(), cx);
    }

    pub(super) fn workspace_navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let slots = self
            .workspace_window
            .as_ref()
            .and_then(|host| host.upgrade())
            .map(|host| {
                host.read(cx)
                    .slots
                    .iter()
                    .filter(|slot| {
                        !slot.workspace.folders.is_empty() || slot.workspace.name.is_some()
                    })
                    .map(|slot| slot.workspace.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        v_flex()
            .id("workspace-navigation")
            .w_full()
            .max_h(relative(0.4))
            .overflow_y_scroll()
            .gap_1()
            .children(slots.into_iter().map(|workspace| {
                if let Some((id, input, _)) = &self.workspace_rename
                    && *id == workspace.id
                {
                    return div()
                        .w_full()
                        .child(Input::new(input))
                        .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" {
                                view.workspace_rename = None;
                                window.focus(&view.focus, cx);
                                cx.notify();
                            }
                        }))
                        .into_any_element();
                }
                let selected = self.page != Page::Start && self.recent_id == workspace.id;
                let background = if selected {
                    crate::views::selection_background(cx)
                } else {
                    cx.theme().sidebar
                };
                let id = workspace.id.clone();
                let title = workspace.title();
                let rename = workspace.clone();
                let weak = cx.entity().downgrade();
                div()
                    .w_full()
                    .child(
                        Button::new(SharedString::from(format!("workspace-{}", workspace.id)))
                            .custom(
                                ButtonCustomVariant::new(cx)
                                    .color(background)
                                    .hover(background.blend(cx.theme().foreground.opacity(0.06)))
                                    .active(background.blend(cx.theme().foreground.opacity(0.10))),
                            )
                            .w_full()
                            .h(px(34.))
                            .px_3()
                            .justify_start()
                            .bg(background)
                            .tooltip(title.clone())
                            .accessibility_label(title.clone())
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap_3()
                                    .child(
                                        Icon::new(gpui_kit::assets::IconName::PanelsTopLeft)
                                            .size(px(16.))
                                            .flex_shrink_0(),
                                    )
                                    .child(div().flex_1().min_w_0().truncate().child(title)),
                            )
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.select_workspace(id.clone(), window, cx)
                            })),
                    )
                    .context_menu(move |menu, _, _| {
                        let weak = weak.clone();
                        let rename = rename.clone();
                        menu.item(
                            PopupMenuItem::new(glim_services::locale::tr("Rename Workspace…"))
                                .on_click(move |_, window, cx| {
                                    let _ = weak.update(cx, |view, cx| {
                                        view.begin_workspace_rename(rename.clone(), window, cx)
                                    });
                                }),
                        )
                    })
                    .into_any_element()
            }))
            .into_any_element()
    }
}
