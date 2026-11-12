use super::{DEFAULT_SIDEBAR_RATIO, Page, SIDEBAR_COLLAPSE_WIDTH, Workspace};
use crate::{
    app::actions::{
        AddFolder, CloseWindow, CloseWorkspace, ManageConnections, NavigateBack, NavigateForward,
        NewFile, NextTab, OpenFile, OpenFolder, OpenRemote, OpenSettings, PreviousTab, Refresh,
        SaveFile, SearchWorkspace, ToggleFileTree, TogglePreview, ToggleSidebar, ToggleWordWrap,
    },
    views::welcome::welcome,
};
use gpui_kit::{
    base::InteractiveElementExt as _,
    component::{
        ActiveTheme, Disableable, Icon, Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
        h_flex,
        menu::{ContextMenuExt, PopupMenuItem},
        resizable::{h_resizable, resizable_panel},
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};

const FILE_PANEL_HEADING_HEIGHT: f32 = 40.;
const FILE_PANEL_HEADING_BUTTON_HEIGHT: f32 = 34.;

impl Workspace {
    fn file_panel_heading_style(cx: &App) -> ButtonCustomVariant {
        ButtonCustomVariant::new(cx)
            .foreground(cx.theme().foreground)
            .hover(cx.theme().foreground.opacity(0.04))
            .active(cx.theme().foreground.opacity(0.08))
    }

    fn changes_panel_preview(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.changes_expanded {
            return v_flex()
                .id("changes-panel-preview")
                .w_full()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(
                    h_flex()
                        .w_full()
                        .h(px(FILE_PANEL_HEADING_HEIGHT))
                        .flex_shrink_0()
                        .px_2()
                        .gap(px(2.))
                        .child(
                            Button::new("expanded-changes-heading")
                                .custom(Self::file_panel_heading_style(cx))
                                .flex_1()
                                .min_w_0()
                                .h(px(FILE_PANEL_HEADING_BUTTON_HEIGHT))
                                .px_1()
                                .rounded(px(6.))
                                .justify_start()
                                .accessibility_label(glim_services::locale::tr("Show Files"))
                                .child(
                                    h_flex()
                                        .w_full()
                                        .min_w_0()
                                        .gap_2()
                                        .child(
                                            Icon::new(gpui_kit::assets::IconName::GitCompareArrows)
                                                .size(px(16.))
                                                .flex_shrink_0(),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .text_sm()
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(glim_services::locale::tr("Changes")),
                                        ),
                                )
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.changes_expanded = false;
                                    window.focus(&view.focus, cx);
                                    cx.notify();
                                })),
                        )
                        .child(self.file_tree_toggle(cx).w(px(24.))),
                )
                .children(self.git_repositories.iter().map(|repository| {
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .h(px(34.))
                        .flex_shrink_0()
                        .pl_3()
                        .pr_1()
                        .gap_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            Icon::new(gpui_kit::assets::IconName::FolderGit2)
                                .size(px(14.))
                                .flex_shrink_0(),
                        )
                        .child(
                            div().flex_1().min_w_0().truncate().child(
                                repository
                                    .file_name()
                                    .unwrap_or(repository.as_os_str())
                                    .to_string_lossy()
                                    .into_owned(),
                            ),
                        )
                }))
                .child(
                    v_flex()
                        .flex_1()
                        .min_h_0()
                        .justify_center()
                        .items_center()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(glim_services::locale::tr("Changes preview")),
                )
                .into_any_element();
        }
        h_flex()
            .id("collapsed-changes-heading")
            .w_full()
            .h(px(FILE_PANEL_HEADING_HEIGHT))
            .flex_shrink_0()
            .px_2()
            .cursor_pointer()
            .on_click(cx.listener(|view, _, window, cx| {
                view.path_hint.hide();
                view.changes_expanded = true;
                window.focus(&view.focus, cx);
                cx.notify();
            }))
            .child(
                Button::new("file-panel-changes")
                    .custom(Self::file_panel_heading_style(cx))
                    .w_full()
                    .min_w_0()
                    .h(px(FILE_PANEL_HEADING_BUTTON_HEIGHT))
                    .px_1()
                    .rounded(px(6.))
                    .justify_start()
                    .text_left()
                    .accessibility_label(glim_services::locale::tr("Show Changes"))
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .gap_2()
                            .child(
                                Icon::new(gpui_kit::assets::IconName::GitCompareArrows)
                                    .size(px(16.))
                                    .flex_shrink_0(),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(glim_services::locale::tr("Changes")),
                            )
                            .child(
                                Icon::new(gpui_kit::assets::IconName::ChevronRight)
                                    .size(px(12.))
                                    .flex_shrink_0()
                                    .text_color(cx.theme().muted_foreground),
                            ),
                    )
                    .on_click(cx.listener(|view, _, window, cx| {
                        cx.stop_propagation();
                        view.path_hint.hide();
                        view.changes_expanded = true;
                        window.focus(&view.focus, cx);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let outline = super::layout::WorkspaceOutline::default();
        self.update_usage(window, cx);
        let owner = cx.entity().downgrade();
        let native_toolbar = std::rc::Rc::downgrade(&self.reader_toolbar);
        crate::views::reader::set_toolbar_host(
            crate::views::reader::ReaderToolbarHost::new(
                self.reader_toolbar_collapsed,
                std::rc::Rc::new(move |controls, window, cx| {
                    if let Some(owner) = owner.upgrade() {
                        owner.update(cx, |view, cx| view.content_toolbar(controls, window, cx))
                    } else {
                        div().into_any_element()
                    }
                }),
                std::rc::Rc::new(move |window| {
                    if let Some(toolbar) = native_toolbar.upgrade() {
                        toolbar.update(Bounds::default(), b"[]", false, true, window);
                    }
                }),
            ),
            window,
            cx,
        );
        if self.page != Page::Document || self.active_reader().is_none() {
            self.set_file_fullscreen(false, window);
        }
        for (index, reader) in self.tabs.iter().enumerate() {
            if self.page != Page::Document || self.active != Some(index) {
                reader.deactivate(cx);
            } else {
                reader.activate(cx);
            }
        }
        let notice = self
            .error
            .clone()
            .map(|message| {
                (
                    glim_services::locale::tr("Operation incomplete").into(),
                    message,
                    true,
                )
            })
            .or_else(|| {
                self.refresh_notice
                    .clone()
                    .map(|(title, message)| (title, message, false))
            });
        let window_width = f32::from(window.viewport_size().width);
        let sidebar_visible = self.workspace_sidebar_visible();
        let sidebar_width = self.workspace_column_width(window_width);
        let explorer_width = self.explorer_column_width(window_width);
        let navigation_width = sidebar_width + explorer_width;
        let header_width = navigation_width.max(self.header_controls_width(window, cx));
        let rounded_header_join = header_width - navigation_width > 0.5;
        let header_cutout = rounded_header_join.then(|| {
            size(
                px(header_width - navigation_width),
                px(super::TITLE_BAR_HEIGHT),
            )
        });
        let has_tabs = self.page == Page::Document
            && (!self.tabs.is_empty() || self.pending_open_path().is_some());
        let tab_bar = self.tab_bar((header_width - navigation_width).max(0.), window, cx);
        let header = self.header(None, rounded_header_join, cx);
        let view = cx.entity().downgrade();
        let open: crate::views::reader::OpenLink =
            std::sync::Arc::new(move |path, anchor, window, cx| {
                let _ = view.update(cx, |v, cx| {
                    v.open_path(path.clone(), window, cx);
                    if let Some(anchor) = anchor {
                        if let Some(i) = v.active.filter(|i| v.tabs[*i].document.path == path) {
                            v.tabs[i].reveal_anchor(&anchor, cx);
                        } else {
                            v.pending_anchor = Some((path, anchor));
                        }
                    }
                    cx.notify();
                });
            });
        if (self.sidebar_window_width - window_width).abs() > 0.5 {
            self.sidebar_window_width = window_width;
            self.explorer_container_width =
                (window_width - sidebar_width - super::CONTENT_INSET).max(0.);
            // Let the panels' initial sizes take effect before layout, rather
            // than painting stale sizes and correcting them in later frames.
            self.sidebar_layout.update(cx, |state, _| state.clear());
            self.explorer_layout.update(cx, |state, _| state.clear());
        }
        let task_workspace = cx.entity().downgrade();
        let toggle_task: crate::views::reader::ToggleTask =
            std::sync::Arc::new(move |editor, range, checked, revision, window, cx| {
                let _ = task_workspace.update(cx, |workspace, cx| {
                    workspace.toggle_markdown_task(editor, range, checked, revision, window, cx);
                });
            });
        let archive_owner = cx.entity().downgrade();
        let archive_action_kind = if self.picking {
            super::reader_actions::ToolbarAction::CancelExtract
        } else {
            super::reader_actions::ToolbarAction::Extract
        };
        let archive_label = glim_services::locale::tr(if self.picking {
            "Extracting… · Cancel"
        } else {
            "Extract…"
        });
        let archive_action: crate::views::reader::ArchiveAction =
            std::sync::Arc::new(move |window, cx| {
                let _ = archive_owner.update(cx, |v, cx| {
                    v.run_toolbar_action(archive_action_kind, window, cx)
                });
            });
        let content = match self.page {
            Page::Settings => self.settings_page(cx),
            Page::Start => welcome(self, cx.entity_id(), cx).into_any_element(),
            Page::Document => match self.active.and_then(|i| self.tabs.get_mut(i)) {
                Some(reader) => {
                    reader.set_archive_action(archive_label, archive_action, cx);
                    reader.set_task_toggle_handler(toggle_task, cx);
                    reader.render(open, cx)
                }
                None => welcome(self, cx.entity_id(), cx).into_any_element(),
            },
        };
        // Preview actions use the same live capabilities and dispatcher as the
        // toolbar. Text editors keep their selection-aware editing menu.
        let content = if self.page == Page::Document
            && self.active_reader().is_some_and(|reader| {
                reader.is_archive()
                    || (reader.can_preview() && reader.mode() == glim_core::ReaderMode::Preview)
            }) {
            let workspace = cx.entity().downgrade();
            div()
                .size_full()
                .child(content)
                .context_menu(move |mut menu, _, cx| {
                    if let Some(workspace) = workspace.upgrade() {
                        if let Some(state) = workspace
                            .read(cx)
                            .active_reader()
                            .and_then(|reader| reader.preview_text_state())
                        {
                            menu = super::reader_actions::preview_text_menu(menu, state, cx);
                        }
                        for item in workspace.read(cx).reader_actions(cx) {
                            let action = item.action;
                            let workspace = workspace.downgrade();
                            menu = menu.item(
                                PopupMenuItem::new(glim_services::locale::tr(item.label))
                                    .disabled(!item.enabled)
                                    .checked(item.selected)
                                    .on_click(move |_, window, cx| {
                                        let _ = workspace.update(cx, |workspace, cx| {
                                            workspace.run_toolbar_action(action, window, cx)
                                        });
                                    }),
                            );
                        }
                    }
                    menu
                })
                .into_any_element()
        } else {
            content
        };
        let document_page = self.page == Page::Document;
        let gesture_view = cx.entity().downgrade();
        let viewer = v_flex()
            .relative()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .when(!self.file_fullscreen, |view| view.rounded(px(12.)))
            .bg(cx.theme().background)
            .when(
                !has_tabs && self.page != Page::Start && !self.file_fullscreen,
                |view| view.pt(px(super::TITLE_BAR_HEIGHT)),
            )
            .when(has_tabs && !self.file_fullscreen, |view| {
                view.child(tab_bar)
            })
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(content)
                    .child(self.floating_reader_toolbar(window, cx))
                    .child(crate::views::reader::ReaderToolbarDock)
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                let view = gesture_view.clone();
                                window.on_mouse_event(
                                    move |event: &ScrollWheelEvent, phase, window, cx| {
                                        if document_page
                                            && phase == DispatchPhase::Capture
                                            && bounds.contains(&event.position)
                                        {
                                            let _ = view.update(cx, |v, cx| {
                                                v.record_scroll(event, window, cx);
                                                v.history_swipe(event, window, cx)
                                            });
                                        }
                                    },
                                );
                            },
                        )
                        .absolute()
                        .size_full(),
                    ),
            );
        let viewer = div()
            .relative()
            .size_full()
            .min_w_0()
            .bg(cx.theme().sidebar)
            .child(viewer)
            .when(!self.file_fullscreen, |view| {
                view.child(super::layout::reader_frame(&outline, header_cutout, cx))
            });
        let panel_content = self
            .explorer
            .as_ref()
            .map(|v| v.clone().into_any_element())
            .unwrap_or_else(|| div().into_any_element());
        let navigation = |id: &'static str, icon, label: &'static str, selected: bool| {
            let background = if selected {
                crate::views::selection_background(cx)
            } else {
                cx.theme().sidebar
            };
            Button::new(id)
                .custom(
                    ButtonCustomVariant::new(cx)
                        .color(background)
                        .hover(background.blend(cx.theme().foreground.opacity(0.06)))
                        .active(background.blend(cx.theme().foreground.opacity(0.10))),
                )
                // Override the component's translucent idle custom fill.
                .bg(background)
                .font_weight(if selected {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .w_full()
                .h(px(super::WORKSPACE_ROW_HEIGHT))
                .rounded(px(8.))
                .justify_start()
                .accessibility_label(glim_services::locale::tr(label))
                .child(
                    h_flex()
                        .w_full()
                        .gap_3()
                        .child(Icon::new(icon).size(px(17.)))
                        .child(glim_services::locale::tr(label)),
                )
        };
        let start = navigation(
            "start-navigation",
            gpui_kit::assets::IconName::House,
            "Start",
            self.page == Page::Start,
        );
        let workspace_action_style = ButtonCustomVariant::new(cx)
            .foreground(cx.theme().muted_foreground)
            .hover(cx.theme().foreground.opacity(0.04))
            .active(cx.theme().foreground.opacity(0.08));
        let sidebar = v_flex()
            .size_full()
            .pt(px(super::TITLE_BAR_HEIGHT))
            .pl_3()
            .pb_1()
            .gap_1()
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .pr_2()
                    .child(start.on_click(cx.listener(|view, _, _, cx| {
                        view.show_home(cx);
                        cx.notify();
                    }))),
            )
            .child(self.workspace_navigation(&outline, cx))
            .id("workspace-sidebar");
        let has_git = !self.git_repositories.is_empty();
        let show_changes = has_git && self.changes_expanded;
        let files_heading = h_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .text_left()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.theme().foreground)
            .child(
                Icon::new(gpui_kit::assets::IconName::Folder)
                    .size(px(16.))
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(glim_services::locale::tr("Files")),
            )
            .when(show_changes, |row| {
                row.child(
                    Icon::new(gpui_kit::assets::IconName::ChevronRight)
                        .size(px(12.))
                        .flex_shrink_0(),
                )
            });
        let files_header = h_flex()
            .id("file-panel-files-header")
            .w_full()
            .min_w_0()
            .h(px(FILE_PANEL_HEADING_HEIGHT))
            .flex_shrink_0()
            .px_2()
            .gap(px(2.))
            .when(has_git, |header| header.cursor_pointer())
            .on_click(cx.listener(move |view, _, window, cx| {
                if has_git {
                    view.path_hint.hide();
                    view.changes_expanded = !view.changes_expanded;
                    window.focus(&view.focus, cx);
                    cx.notify();
                }
            }))
            .child(if has_git {
                Button::new("file-panel-files")
                    .custom(Self::file_panel_heading_style(cx))
                    .small()
                    .flex_1()
                    .min_w_0()
                    .h(px(FILE_PANEL_HEADING_BUTTON_HEIGHT))
                    .rounded(px(6.))
                    .px_1()
                    .justify_start()
                    .text_left()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().foreground)
                    .accessibility_label(glim_services::locale::tr(if show_changes {
                        "Show Files"
                    } else {
                        "Show Changes"
                    }))
                    .child(files_heading)
                    .on_click(cx.listener(|view, _, window, cx| {
                        cx.stop_propagation();
                        view.path_hint.hide();
                        view.changes_expanded = !view.changes_expanded;
                        window.focus(&view.focus, cx);
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .h(px(FILE_PANEL_HEADING_BUTTON_HEIGHT))
                    .px_1()
                    .child(files_heading)
                    .into_any_element()
            })
            .when(!show_changes, |header| {
                header
                    .child(
                        Button::new("workspace-new-file")
                            .disabled(glim_services::remote::is_remote_id(&self.recent_id))
                            .custom(workspace_action_style)
                            .small()
                            .w(px(24.))
                            .h(px(28.))
                            .p_0()
                            .flex_shrink_0()
                            .icon(gpui_kit::assets::IconName::FilePlus)
                            .tooltip(glim_services::locale::tr("New File"))
                            .accessibility_label(glim_services::locale::tr("New File"))
                            .on_click(cx.listener(|view, _, window, cx| {
                                cx.stop_propagation();
                                view.choose_new_file(window, cx)
                            })),
                    )
                    .child(
                        Button::new("workspace-new-folder")
                            .custom(workspace_action_style)
                            .small()
                            .w(px(24.))
                            .h(px(28.))
                            .p_0()
                            .flex_shrink_0()
                            .icon(gpui_kit::assets::IconName::FolderPlus)
                            .disabled(
                                self.roots.is_empty()
                                    || glim_services::remote::is_remote_id(&self.recent_id),
                            )
                            .tooltip(glim_services::locale::tr("New Folder…"))
                            .accessibility_label(glim_services::locale::tr("New Folder…"))
                            .on_click(cx.listener(|view, _, window, cx| {
                                cx.stop_propagation();
                                view.choose_new_folder(window, cx)
                            })),
                    )
                    .child(
                        Button::new("workspace-add-folder")
                            .custom(workspace_action_style)
                            .small()
                            .w(px(24.))
                            .h(px(28.))
                            .p_0()
                            .flex_shrink_0()
                            .icon(gpui_kit::assets::IconName::Folders)
                            .tooltip(glim_services::locale::tr("Add Folder to Workspace…"))
                            .accessibility_label(glim_services::locale::tr(
                                "Add Folder to Workspace…",
                            ))
                            .on_click(cx.listener(|view, _, window, cx| {
                                cx.stop_propagation();
                                view.choose_add_folder(window, cx)
                            })),
                    )
                    .child(
                        self.file_tree_toggle(cx)
                            .custom(workspace_action_style)
                            .w(px(24.)),
                    )
            });
        let file_tree = v_flex()
            .relative()
            .child(outline.tree_marker())
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .bg(crate::views::workspace_background(cx))
            .rounded_tl(px(12.))
            .rounded_bl(px(12.))
            .child(files_header)
            .when(!show_changes, |view| {
                view.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .px_2()
                        .child(panel_content),
                )
            })
            .when(has_git, |view| {
                view.child(
                    div().w_full().flex_shrink_0().px_3().child(
                        div()
                            .w_full()
                            .h(px(1.))
                            .bg(cx.theme().foreground.opacity(0.08)),
                    ),
                )
                .child(self.changes_panel_preview(cx))
            })
            .id("workspace-file-tree");
        let file_tree = if self.explorer.is_none() {
            file_tree
                .context_menu(|menu, _, _| {
                    menu.item(
                        PopupMenuItem::new(glim_services::locale::tr("Add Folder to Workspace…"))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(AddFolder), cx);
                            }),
                    )
                })
                .into_any_element()
        } else {
            file_tree.into_any_element()
        };
        let file_tree = div()
            .size_full()
            .pt(px(super::TITLE_BAR_HEIGHT))
            .child(file_tree)
            .into_any_element();
        let viewer = if self.file_fullscreen {
            viewer.into_any_element()
        } else {
            self.file_tree_columns(file_tree, viewer.into_any_element(), window, cx)
        };
        let body = if self.file_fullscreen {
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(viewer)
                .into_any_element()
        } else {
            h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .pr(px(super::CONTENT_INSET))
                .pb(px(super::CONTENT_INSET))
                .pt(px(super::CONTENT_INSET))
                .when(!sidebar_visible, |view| {
                    view.child(
                        v_flex()
                            .id("collapsed-sidebar")
                            .w(px(48.))
                            .h_full()
                            .flex_shrink_0()
                            .items_center()
                            .pt(px(super::TITLE_BAR_HEIGHT))
                            .gap_1()
                            .bg(cx.theme().sidebar)
                            .child(
                                Button::new("collapsed-home")
                                    .custom(
                                        ButtonCustomVariant::new(cx)
                                            .color(cx.theme().sidebar)
                                            .foreground(cx.theme().foreground)
                                            .hover(cx.theme().sidebar)
                                            .active(cx.theme().sidebar),
                                    )
                                    .bg(cx.theme().sidebar)
                                    .w(px(32.))
                                    .h(px(super::WORKSPACE_ROW_HEIGHT))
                                    .flex_shrink_0()
                                    .icon(
                                        Icon::new(gpui_kit::assets::IconName::House).size(px(17.)),
                                    )
                                    .tooltip(glim_services::locale::tr("Start"))
                                    .accessibility_label(glim_services::locale::tr("Start"))
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.show_home(cx);
                                        cx.notify();
                                    })),
                            )
                            .child(self.collapsed_workspace_navigation(&outline, window, cx)),
                    )
                })
                .child(
                    div().flex_1().min_w_0().h_full().child(
                        h_resizable("workspace-panels")
                            .with_state(&self.sidebar_layout)
                            .on_resize(cx.listener(
                                |view,
                                 state: &Entity<gpui_kit::component::resizable::ResizableState>,
                                 window,
                                 cx| {
                                    if let Some(width) = state.read(cx).sizes().first().copied() {
                                        if view.page != Page::Start
                                            && f32::from(width) <= SIDEBAR_COLLAPSE_WIDTH
                                        {
                                            view.sidebar_visible = false;
                                            view.sidebar_window_width = 0.;
                                        } else {
                                            view.sidebar_ratio = (f32::from(width)
                                                / f32::from(window.viewport_size().width))
                                            .min(0.35);
                                        }
                                        cx.notify();
                                    }
                                },
                            ))
                            .with_handle_appearance({
                                let appearance =
                                    gpui_kit::component::resizable::resize_handle_appearance();
                                let layout = self.sidebar_layout.clone();
                                let workspace = cx.entity().downgrade();
                                let can_collapse = self.page != Page::Start;
                                std::rc::Rc::new(move |handle, window, cx| {
                                    if can_collapse
                                        && handle.is_active()
                                        && layout.read(cx).sizes().first().is_some_and(|size| {
                                            f32::from(*size) <= SIDEBAR_COLLAPSE_WIDTH
                                        })
                                    {
                                        let workspace = workspace.clone();
                                        cx.defer(move |cx| {
                                            let _ = workspace.update(cx, |view, cx| {
                                                view.sidebar_visible = false;
                                                view.sidebar_window_width = 0.;
                                                cx.notify();
                                            });
                                        });
                                    }
                                    appearance(handle, window, cx).map(|line| {
                                        let layout = layout.clone();
                                        let workspace = workspace.clone();
                                        // Keep the visible line inset, but use the full drag band for double clicks.
                                        div()
                                            .id("workspace-sidebar-resize-handle")
                                            .flex()
                                            .justify_center()
                                            .w(px(9.))
                                            .ml(px(-4.))
                                            .h_full()
                                            .pt(px(14.))
                                            .pb(px(14.))
                                            .when(handle.is_active(), |view| view.child(line))
                                            .on_double_click(move |_, window, cx| {
                                                let _ = workspace.update(cx, |view, cx| {
                                                    view.sidebar_ratio = DEFAULT_SIDEBAR_RATIO;
                                                    view.sidebar_window_width = 0.;
                                                    cx.notify();
                                                });
                                                layout.update(cx, |state, cx| {
                                                    state.resize_panel(
                                                        0,
                                                        window.viewport_size().width
                                                            * DEFAULT_SIDEBAR_RATIO,
                                                        window,
                                                        cx,
                                                    );
                                                });
                                                cx.stop_propagation();
                                            })
                                            .into_any_element()
                                    })
                                })
                            })
                            .child(
                                resizable_panel()
                                    .visible(sidebar_visible)
                                    .size(px(sidebar_width))
                                    .flex_none()
                                    .size_range(
                                        px(if self.page == Page::Start { 160. } else { 80. })
                                            ..px(window_width * 0.35),
                                    )
                                    .child(sidebar),
                            )
                            .child(
                                resizable_panel()
                                    .size_range(
                                        px(if self.page == Page::Start {
                                            280.
                                        } else if self.explorer_visible {
                                            480.
                                        } else {
                                            280.
                                        })..px(10000.),
                                    )
                                    .child(viewer),
                            ),
                    ),
                )
                .into_any_element()
        };
        v_flex()
            .relative()
            .size_full()
            .track_focus(&self.focus)
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().foreground)
            .capture_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                if view.search_visible {
                    view.search_key_down(event, window, cx);
                } else if view.file_fullscreen && event.keystroke.key == "escape" {
                    view.set_file_fullscreen(false, window);
                    view.sidebar_window_width = 0.;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|v, _, w, cx| {
                    v.history_swipe = None;
                    v.navigate(false, w, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|v, _, w, cx| {
                    v.history_swipe = None;
                    v.navigate(true, w, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(cx.listener(|v, _: &OpenSettings, window, cx| {
                v.show_settings(window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|v, _: &SearchWorkspace, w, cx| {
                v.show_search(w, cx);
            }))
            .on_action(
                cx.listener(|v, _: &gpui_kit::component::input::Search, window, cx| {
                    if !v.search_visible
                        && v.page == Page::Document
                        && let Some(reader) = v.active_reader().filter(|reader| reader.can_find(cx))
                    {
                        reader.find(window, cx);
                    } else {
                        v.show_search(window, cx);
                    }
                }),
            )
            .on_action(
                cx.listener(|v, _: &gpui_kit::component::input::Replace, window, cx| {
                    if !v.search_visible
                        && v.page == Page::Document
                        && let Some(reader) = v.active_reader()
                    {
                        reader.replace(window, cx);
                    }
                }),
            )
            .on_action(cx.listener(|v, _: &ToggleSidebar, _, cx| v.toggle_sidebar(cx)))
            .on_action(cx.listener(|v, _: &ToggleFileTree, _, cx| v.toggle_file_tree(cx)))
            .on_action(cx.listener(|v, _: &TogglePreview, w, cx| {
                if let Some(reader) = v.active_reader().filter(|reader| reader.can_preview()) {
                    let preview = reader.mode() != glim_core::ReaderMode::Preview;
                    v.set_reader_preview(preview, w, cx);
                }
            }))
            .on_action(cx.listener(|v, _: &ToggleWordWrap, w, cx| v.toggle_word_wrap(w, cx)))
            .on_action(
                cx.listener(|v, _: &gpui_kit::component::input::Copy, _, cx| {
                    if !v
                        .active_reader()
                        .is_some_and(|reader| reader.copy_image(cx))
                    {
                        cx.propagate();
                    }
                }),
            )
            .on_action(cx.listener(|v, _: &NextTab, w, cx| {
                if !v.tabs.is_empty() {
                    v.activate_tab((v.active.unwrap_or(0) + 1) % v.tabs.len(), w, cx);
                }
            }))
            .on_action(cx.listener(|v, _: &PreviousTab, w, cx| {
                if !v.tabs.is_empty() {
                    v.activate_tab(
                        (v.active.unwrap_or(0) + v.tabs.len() - 1) % v.tabs.len(),
                        w,
                        cx,
                    );
                }
            }))
            .on_action(cx.listener(|v, _: &NavigateBack, w, cx| v.navigate(false, w, cx)))
            .on_action(cx.listener(|v, _: &NavigateForward, w, cx| v.navigate(true, w, cx)))
            .on_action(cx.listener(|v, _: &CloseWorkspace, w, cx| {
                v.request_close_window(w, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|v, _: &AddFolder, w, cx| v.choose_add_folder(w, cx)))
            .on_action(cx.listener(|v, _: &NewFile, w, cx| v.choose_new_file(w, cx)))
            .on_action(cx.listener(|v, _: &OpenFile, w, cx| v.choose_path(false, w, cx)))
            .on_action(cx.listener(|v, _: &OpenFolder, w, cx| v.choose_path(true, w, cx)))
            .on_action(
                cx.listener(|v, _: &OpenRemote, w, cx| v.show_remote(None, false, false, w, cx)),
            )
            .on_action(cx.listener(|v, _: &ManageConnections, w, cx| {
                v.show_remote(None, true, false, w, cx)
            }))
            .on_action(cx.listener(|v, _: &Refresh, w, cx| v.refresh(w, cx)))
            .on_action(cx.listener(|v, _: &SaveFile, w, cx| v.save_active(w, cx)))
            .on_action(cx.listener(|v, _: &CloseWindow, w, cx| {
                if let Some(i) = v.active {
                    v.close_tab(i, w, cx);
                } else {
                    v.request_close_window(w, cx);
                }
                cx.stop_propagation();
            }))
            .child(body)
            .child(outline.render(cx))
            .when(!self.file_fullscreen, |view| {
                view.child(
                    div()
                        .absolute()
                        .top(px(super::CONTENT_INSET))
                        .left_0()
                        .w(px(header_width))
                        .child(header),
                )
            })
            .when(
                self.file_fullscreen && self.reader_toolbar_collapsed,
                |view| {
                    view.child(
                        div()
                            .absolute()
                            .top(px(12.))
                            .left(px(12.))
                            .child(self.restore_reader_toolbar(cx)),
                    )
                },
            )
            .children(self.remote_dialog.clone())
            .when(self.search_visible, |view| {
                view.child(self.search_dialog(window, cx))
            })
            .when_some(notice, |view, (title, error, danger)| {
                view.child(
                    deferred(
                        v_flex()
                            .absolute()
                            .right(px(16.))
                            .bottom(px(16.))
                            .w(px(420.))
                            .max_w(relative(0.9))
                            .p_3()
                            .gap_2()
                            .rounded_lg()
                            .bg(cx.theme().background)
                            .border_1()
                            .border_color(cx.theme().border)
                            .shadow_lg()
                            .occlude()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(if danger {
                                                cx.theme().danger
                                            } else {
                                                cx.theme().foreground
                                            })
                                            .child(title),
                                    )
                                    .child(
                                        Button::new("dismiss-error")
                                            .ghost()
                                            .icon(
                                                Icon::new(gpui_kit::assets::IconName::X)
                                                    .size(px(14.)),
                                            )
                                            .tooltip(glim_services::locale::tr("Dismiss error"))
                                            .accessibility_label(glim_services::locale::tr(
                                                "Dismiss error",
                                            ))
                                            .on_click(cx.listener(|v, _, _, cx| {
                                                v.error = None;
                                                v.refresh_notice = None;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .id("workspace-error-message")
                                    .max_h(px(160.))
                                    .overflow_y_scroll()
                                    .text_sm()
                                    .whitespace_normal()
                                    .text_color(cx.theme().foreground)
                                    .child(error),
                            ),
                    )
                    .with_priority(2),
                )
            })
    }
}
