use super::{DEFAULT_EXPLORER_RATIO, SIDEBAR_COLLAPSE_WIDTH, Workspace};
use gpui_kit::{
    base::{ElementExt as _, InteractiveElementExt as _},
    component::{
        button::Button,
        h_flex,
        resizable::{h_resizable, resizable_panel},
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};

impl Workspace {
    pub(super) fn workspace_column_width(&self, window_width: f32) -> f32 {
        if self.sidebar_visible {
            (window_width * self.sidebar_ratio).clamp(160., window_width * 0.35)
        } else {
            48.
        }
    }

    pub(super) fn explorer_column_width(&self, window_width: f32) -> f32 {
        if self.explorer_visible {
            (window_width * self.explorer_ratio).clamp(200., window_width * 0.35)
        } else {
            40.
        }
    }

    pub(super) fn navigation_width(&self) -> f32 {
        let width = self.sidebar_window_width.max(800.);
        self.workspace_column_width(width) + self.explorer_column_width(width)
    }

    pub(super) fn toggle_file_tree(&mut self, cx: &mut Context<Self>) {
        self.explorer_visible = !self.explorer_visible;
        self.sidebar_window_width = 0.;
        cx.notify();
    }

    pub(super) fn file_tree_toggle(&self, cx: &mut Context<Self>) -> Button {
        crate::views::icon_button(
            "toggle-file-tree",
            if self.explorer_visible {
                gpui_kit::assets::IconName::ChevronLeft
            } else {
                gpui_kit::assets::IconName::ChevronRight
            },
            glim_services::locale::tr(if self.explorer_visible {
                "Collapse File Tree"
            } else {
                "Expand File Tree"
            }),
        )
        .on_click(cx.listener(|view, _, _, cx| view.toggle_file_tree(cx)))
    }

    pub(super) fn file_tree_columns(
        &self,
        file_tree: AnyElement,
        viewer: AnyElement,
        header_above_content: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let window_width = f32::from(window.viewport_size().width);
        let workspace = cx.entity().downgrade();
        h_flex()
            .size_full()
            .min_w_0()
            .when(!self.explorer_visible, |row| {
                row.child(
                    v_flex()
                        .w(px(40.))
                        .h_full()
                        .flex_shrink_0()
                        .items_center()
                        .bg(crate::views::workspace_background(cx))
                        .pt(px(if header_above_content {
                            4.
                        } else {
                            super::TITLE_BAR_HEIGHT - super::CONTENT_INSET + 4.
                        }))
                        .child(self.file_tree_toggle(cx)),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .on_prepaint(move |bounds, window, cx| {
                        let width = f32::from(bounds.size.width);
                        let Some(workspace) = workspace.upgrade() else {
                            return;
                        };
                        if (workspace.read(cx).explorer_container_width - width).abs() <= 0.5 {
                            return;
                        }
                        // A change to the workspace rail must give space to the reader,
                        // without proportionally stretching the adjacent file tree.
                        window.defer(cx, move |window, cx| {
                            workspace.update(cx, |view, cx| {
                                view.explorer_container_width = width;
                                if view.explorer_visible {
                                    let desired = view.explorer_column_width(f32::from(
                                        window.viewport_size().width,
                                    ));
                                    view.explorer_layout.update(cx, |state, cx| {
                                        state.resize_panel(0, px(desired), window, cx);
                                    });
                                }
                            });
                        });
                    })
                    .child(
                        h_resizable("file-tree-reader-panels")
                            .with_state(&self.explorer_layout)
                            .on_resize(cx.listener(
                                |view,
                                 state: &Entity<gpui_kit::component::resizable::ResizableState>,
                                 window,
                                 cx| {
                                    if let Some(width) = state.read(cx).sizes().first().copied() {
                                        if f32::from(width) <= SIDEBAR_COLLAPSE_WIDTH {
                                            view.explorer_visible = false;
                                            view.sidebar_window_width = 0.;
                                        } else {
                                            view.explorer_ratio = (f32::from(width)
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
                                let workspace = cx.entity().downgrade();
                                std::rc::Rc::new(move |handle, window, cx| {
                                    appearance(handle, window, cx).map(|line| {
                                        let workspace = workspace.clone();
                                        div()
                                            .id("file-tree-resize-handle")
                                            .w(px(9.))
                                            .ml(px(-4.))
                                            .h_full()
                                            .flex()
                                            .justify_center()
                                            .py_3()
                                            .child(line)
                                            .on_double_click(move |_, _, cx| {
                                                let _ = workspace.update(cx, |view, cx| {
                                                    view.explorer_ratio = DEFAULT_EXPLORER_RATIO;
                                                    view.sidebar_window_width = 0.;
                                                    cx.notify();
                                                });
                                                cx.stop_propagation();
                                            })
                                            .into_any_element()
                                    })
                                })
                            })
                            .child(
                                resizable_panel()
                                    .visible(self.explorer_visible)
                                    .size(px(self.explorer_column_width(window_width)))
                                    .flex_none()
                                    .size_range(px(80.)..px(window_width * 0.35))
                                    .child(file_tree),
                            )
                            .child(
                                resizable_panel()
                                    .size_range(px(280.)..px(10000.))
                                    .child(viewer),
                            ),
                    ),
            )
            .into_any_element()
    }
}
