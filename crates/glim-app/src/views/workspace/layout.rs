use super::{DEFAULT_EXPLORER_RATIO, Page, SIDEBAR_COLLAPSE_WIDTH, Workspace};
use gpui_kit::{
    base::{ElementExt as _, InteractiveElementExt as _},
    component::{
        ActiveTheme, Icon, Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
        h_flex,
        resizable::{h_resizable, resizable_panel},
    },
    *,
};

pub(super) fn reader_frame(workspace_outline: &WorkspaceOutline, cx: &App) -> AnyElement {
    let outside = cx.theme().sidebar;
    let surface = crate::views::workspace_background(cx);
    let border = cx.theme().border;
    let reader = workspace_outline.reader.clone();
    let tree = workspace_outline.tree.clone();
    // GPUI overflow clips to a rectangle. Cover child paint outside the frame's
    // rounded corners, then restore the outline above the child backgrounds.
    canvas(
        move |bounds, _, _| reader.set(Some(bounds)),
        move |bounds, _, window, _| {
            let radius = px(12.)
                .min(bounds.size.width / 2.)
                .min(bounds.size.height / 2.);
            for (corner, x, y) in [
                (bounds.origin, 1., 1.),
                (point(bounds.right(), bounds.top()), -1., 1.),
                (point(bounds.right(), bounds.bottom()), -1., -1.),
                (point(bounds.left(), bounds.bottom()), 1., -1.),
            ] {
                let offset = |a: Pixels, b: Pixels| corner + point(a * x, b * y);
                let tangent = radius * (1. - 0.552_284_8);
                let mut path = PathBuilder::fill();
                path.move_to(corner);
                path.line_to(offset(radius, px(0.)));
                path.cubic_bezier_to(
                    offset(px(0.), radius),
                    offset(tangent, px(0.)),
                    offset(px(0.), tangent),
                );
                path.close();
                if let Ok(path) = path.build() {
                    let connected_corner = x > 0.
                        && y < 0.
                        && tree.get().is_some_and(|tree| {
                            (tree.right() - bounds.left()).abs() <= px(1.)
                                && (tree.bottom() - bounds.bottom()).abs() <= px(1.)
                        });
                    window.paint_path(path, if connected_corner { surface } else { outside });
                }
            }
            window.paint_quad(outline(bounds, border, BorderStyle::default()).corner_radii(radius));
        },
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

#[derive(Clone, Default)]
pub(super) struct WorkspaceOutline {
    pub row: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    pub tree: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    pub reader: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
}

impl WorkspaceOutline {
    pub fn tree_marker(&self) -> AnyElement {
        let tree = self.tree.clone();
        canvas(move |bounds, _, _| tree.set(Some(bounds)), |_, _, _, _| {})
            .absolute()
            .inset_0()
            .into_any_element()
    }

    pub fn row_marker(&self) -> AnyElement {
        let row = self.row.clone();
        canvas(
            move |bounds, window, _| {
                row.set(Some(bounds.intersect(&window.content_mask().bounds)));
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }

    pub fn render(self, cx: &App) -> AnyElement {
        let color = cx.theme().border.opacity(0.65);
        let surface = crate::views::workspace_background(cx);
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let Some(tree) = self.tree.get() else { return };
                let inset = px(0.5);
                let left = tree.left() + inset;
                let right = tree.right() - inset;
                let top = tree.top() + inset;
                let bottom = tree.bottom() - inset;
                let bottom_right = self
                    .reader
                    .get()
                    .filter(|reader| {
                        (tree.right() - reader.left()).abs() <= px(1.)
                            && (tree.bottom() - reader.bottom()).abs() <= px(1.)
                    })
                    .map_or(right, |reader| {
                        reader.left()
                            + px(12.)
                                .min(reader.size.width / 2.)
                                .min(reader.size.height / 2.)
                    });
                let mut corners = vec![
                    (point(bottom_right, bottom), px(0.)),
                    (point(left, bottom), px(12.)),
                ];
                if let Some(row) = self.row.get().filter(|row| row.size.height > px(12.)) {
                    corners.extend([
                        (point(left, row.bottom() - inset), px(4.)),
                        (point(row.left() + inset, row.bottom() - inset), px(6.)),
                        (point(row.left() + inset, row.top() + inset), px(6.)),
                        (point(left, row.top() + inset), px(4.)),
                    ]);
                }
                corners.push((point(left, top), px(12.)));
                corners.push((point(right, top), px(0.)));
                corners.dedup_by(|a, b| a.0 == b.0);
                let mut path = PathBuilder::stroke(px(1.));
                let mut fill = PathBuilder::fill();
                for index in 0..corners.len() {
                    let (corner, radius) = corners[index];
                    let previous = corners[(index + corners.len() - 1) % corners.len()].0;
                    let next = corners[(index + 1) % corners.len()].0;
                    let before = previous - corner;
                    let after = next - corner;
                    let before_length = before.x.abs() + before.y.abs();
                    let after_length = after.x.abs() + after.y.abs();
                    let radius = radius.min(before_length / 2.).min(after_length / 2.);
                    let entry = corner + before * (radius / before_length);
                    let exit = corner + after * (radius / after_length);
                    for builder in [&mut path, &mut fill] {
                        if index == 0 {
                            builder.move_to(entry);
                        } else {
                            builder.line_to(entry);
                        }
                        builder.curve_to(exit, corner);
                    }
                }
                if let Some(row) = self
                    .row
                    .get()
                    .filter(|row| row.size.height > px(12.) && row.right() < tree.left())
                {
                    fill.close();
                    if let Ok(fill) = fill.build() {
                        let bridge = Bounds::from_corners(
                            point(row.right() - px(7.), row.top() - px(4.)),
                            point(tree.left() + px(1.), row.bottom() + px(4.)),
                        );
                        window.with_content_mask(Some(ContentMask { bounds: bridge }), |window| {
                            window.paint_path(fill, surface);
                        });
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }
}

impl Workspace {
    pub(super) fn workspace_sidebar_visible(&self) -> bool {
        self.page == Page::Start || self.sidebar_visible
    }

    pub(super) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        if self.page == Page::Start {
            return;
        }
        self.sidebar_visible = !self.sidebar_visible;
        self.sidebar_window_width = 0.;
        cx.notify();
    }

    pub(super) fn workspace_column_width(&self, window_width: f32) -> f32 {
        if self.workspace_sidebar_visible() {
            (window_width * self.sidebar_ratio).clamp(160., window_width * 0.35)
        } else {
            48.
        }
    }

    pub(super) fn explorer_column_width(&self, window_width: f32) -> f32 {
        if self.page == Page::Start {
            return 0.;
        }
        if self.explorer_visible {
            (window_width * self.explorer_ratio).clamp(200., window_width * 0.35)
        } else {
            0.
        }
    }

    pub(super) fn navigation_width(&self) -> f32 {
        let width = self.sidebar_window_width.max(800.);
        self.workspace_column_width(width) + self.explorer_column_width(width)
    }

    pub(super) fn toggle_file_tree(&mut self, cx: &mut Context<Self>) {
        self.path_hint.hide();
        self.explorer_visible = !self.explorer_visible;
        self.sidebar_window_width = 0.;
        cx.notify();
    }

    pub(super) fn file_tree_toggle(&self, cx: &mut Context<Self>) -> Button {
        let hint_bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::default()));
        let painted_bounds = hint_bounds.clone();
        let label = glim_services::locale::tr(if self.explorer_visible {
            "Collapse File Tree"
        } else {
            "Expand File Tree"
        });
        Button::new("toggle-file-tree")
            .small()
            .w(px(28.))
            .h(px(28.))
            .p_0()
            .flex_shrink_0()
            .child(Icon::new(gpui_kit::assets::IconName::PanelLeft).size(px(16.)))
            .child(
                canvas(
                    move |bounds, _, _| painted_bounds.set(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .accessibility_label(label.clone())
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(cx.theme().transparent)
                    .foreground(cx.theme().muted_foreground)
                    .hover(cx.theme().foreground.opacity(0.06))
                    .active(cx.theme().foreground.opacity(0.10)),
            )
            .bg(cx.theme().transparent)
            .on_hover(cx.listener(move |view, hovered: &bool, window, _| {
                if *hovered {
                    view.path_hint
                        .show(&label, "", false, hint_bounds.get(), window);
                } else {
                    view.path_hint.hide_for("", hint_bounds.get());
                }
            }))
            .on_click(cx.listener(|view, _, _, cx| {
                cx.stop_propagation();
                view.toggle_file_tree(cx);
            }))
    }

    pub(super) fn file_tree_columns(
        &self,
        file_tree: AnyElement,
        viewer: AnyElement,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.page == Page::Start || !self.explorer_visible {
            return viewer;
        }
        let window_width = f32::from(window.viewport_size().width);
        let workspace = cx.entity().downgrade();
        h_flex()
            .size_full()
            .min_w_0()
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
