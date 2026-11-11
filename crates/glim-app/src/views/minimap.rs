mod geometry;
use self::geometry::Geometry;
use gpui_kit::{
    component::{
        ActiveTheme,
        input::{EditorState, Rope},
    },
    *,
};
use std::{cell::Cell, rc::Rc};

pub(crate) const MINIMAP_WIDTH: f32 = 110.;

/// A compact document overview with retained, incrementally updated row geometry.
pub struct Minimap {
    source: Entity<EditorState>,
    geometry: Geometry,
    active: bool,
    preview_scroll: ScrollHandle,
    preview: bool,
    preview_list: Option<ListState>,
    dragging: bool,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    _subscription: Subscription,
}

impl Minimap {
    pub fn new(
        source: Entity<EditorState>,
        preview_scroll: ScrollHandle,
        cx: &mut Context<Self>,
    ) -> Self {
        let geometry = Geometry::new(
            source.read(cx).text(),
            glim_services::resources::pool().clone(),
        );
        let subscription = cx.observe(&source, |_, _, cx| cx.notify());
        Self {
            source,
            geometry,
            active: true,
            preview_scroll,
            preview: false,
            preview_list: None,
            dragging: false,
            bounds: Rc::new(Cell::new(Bounds::default())),
            _subscription: subscription,
        }
    }

    pub fn set_text(&mut self, text: &Rope, cx: &mut Context<Self>) {
        if self.active {
            let _ = self.geometry.update(text);
        } else {
            self.geometry.suspend(text);
        }
        cx.notify();
    }

    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        let text = self.source.read(cx).text().clone();
        if active {
            let _ = self.geometry.update(&text);
        } else {
            self.geometry.suspend(&text);
        }
        cx.notify();
    }

    pub fn set_preview_list(&mut self, list: ListState, _cx: &mut Context<Self>) {
        self.preview_list = Some(list);
    }

    pub fn set_preview(&mut self, preview: bool, cx: &mut Context<Self>) {
        if self.preview != preview {
            self.preview = preview;
            cx.notify();
        }
    }

    fn seek(&self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let bounds = self.bounds.get();
        let total = self.geometry.len().max(1) as f32;
        let rail = position.x >= bounds.origin.x + px(96.);
        let height = if rail {
            f32::from(bounds.size.height)
        } else {
            f32::from(bounds.size.height).min(total * 2.)
        }
        .max(1.);
        let fraction = (f32::from(position.y - bounds.origin.y) / height).clamp(0., 1.);
        if self.preview {
            let (max, visible) = if let Some(list) = &self.preview_list {
                (
                    list.max_offset_for_scrollbar().y,
                    list.viewport_bounds().size.height,
                )
            } else {
                (
                    self.preview_scroll.max_offset().y,
                    self.preview_scroll.bounds().size.height,
                )
            };
            let offset = ((max + visible) * fraction - visible / 2.)
                .max(px(0.))
                .min(max);
            if let Some(list) = &self.preview_list {
                list.set_offset_from_scrollbar(point(px(0.), -offset));
            } else {
                self.preview_scroll.set_offset(point(px(0.), -offset));
            }
            cx.notify();
        } else {
            self.source.update(cx, |state, cx| {
                let line_height = state.line_height().unwrap_or(px(20.));
                let visible = state.visible_row_range().map_or(1, |r| r.len()) as f32;
                let row = (fraction * total - visible / 2.).max(0.);
                state.set_scroll_offset(point(state.scroll_offset().x, -line_height * row), cx);
            });
        }
    }
}

impl Render for Minimap {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = self.geometry.snapshot();
        let line_count = self.geometry.len();
        let bounds_cell = self.bounds.clone();
        let source = self.source.clone();
        let scroll = self.preview_scroll.clone();
        let preview_list = self.preview_list.clone();
        let preview = self.preview;
        let foreground = cx.theme().muted_foreground.opacity(0.45);
        let viewport = cx.theme().primary.opacity(0.12);
        div()
            .id("minimap")
            .w(px(110.))
            .h_full()
            .cursor_pointer()
            .bg(cx.theme().background)
            .border_l_1()
            .border_color(cx.theme().border)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, _, cx| {
                    view.dragging = true;
                    view.seek(event.position, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _, _, _| view.dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _, _, _| view.dragging = false),
            )
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
                if view.dragging && event.pressed_button == Some(MouseButton::Left) {
                    view.seek(event.position, cx);
                }
            }))
            .child(
                canvas(
                    move |bounds, _, _| {
                        bounds_cell.set(bounds);
                    },
                    move |bounds, _, window, cx| {
                        let total = line_count.max(1) as f32;
                        let height = f32::from(bounds.size.height).min(total * 2.);
                        let step = (total / (height / 2.).max(1.)).ceil() as usize;
                        for index in (0..line_count).step_by(step.max(1)) {
                            let Some((indent, width)) =
                                lines.as_ref().and_then(|lines| lines.get(index))
                            else {
                                continue;
                            };
                            if width == 0 {
                                continue;
                            }
                            let y = height * index as f32 / total;

                            window.paint_quad(fill(
                                Bounds::new(
                                    bounds.origin + point(px(4. + indent as f32 * 0.7), px(y)),
                                    size(px((width as f32 * 0.7).min(86.)), px(1.)),
                                ),
                                foreground,
                            ));
                        }
                        let (start, length) = if preview {
                            let (visible, maximum, offset) = if let Some(list) = &preview_list {
                                (
                                    list.viewport_bounds().size.height,
                                    list.max_offset_for_scrollbar().y,
                                    list.scroll_px_offset_for_scrollbar().y,
                                )
                            } else {
                                (
                                    scroll.bounds().size.height,
                                    scroll.max_offset().y,
                                    scroll.offset().y,
                                )
                            };
                            let visible = f32::from(visible);
                            let full = f32::from(maximum) + visible;
                            if full > 0. {
                                (-f32::from(offset) / full, visible / full)
                            } else {
                                (0., 1.)
                            }
                        } else {
                            let range = source.read(cx).visible_row_range().unwrap_or(0..1);
                            (range.start as f32 / total, range.len() as f32 / total)
                        };
                        window.paint_quad(fill(
                            Bounds::new(
                                bounds.origin + point(px(0.), px(height * start.min(1.))),
                                size(px(96.), px((height * length.min(1.)).max(3.))),
                            ),
                            viewport,
                        ));
                        if !preview && length < 1. {
                            let rail_height = f32::from(bounds.size.height);
                            let thumb_height = (rail_height * length).max(24.).min(rail_height);
                            let progress = (start / (1. - length)).clamp(0., 1.);
                            window.paint_quad(fill(
                                Bounds::new(
                                    bounds.origin
                                        + point(
                                            px(99.),
                                            px((rail_height - thumb_height) * progress),
                                        ),
                                    size(px(8.), px(thumb_height)),
                                ),
                                foreground,
                            ));
                        }
                    },
                )
                .size_full(),
            )
    }
}
