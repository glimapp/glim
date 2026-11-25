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
const OVERVIEW_WIDTH: Pixels = px(96.);

#[derive(Clone, Copy)]
enum Drag {
    Overview,
    Rail { grab: Pixels },
}

fn rail_thumb(state: &EditorState, height: Pixels) -> (Pixels, Pixels) {
    let maximum = state.max_vertical_scroll_offset();
    let visible = state.input_bounds().size.height;
    if maximum <= px(0.) {
        return (px(0.), height);
    }
    let length = (height * (visible / (maximum + visible))).max(px(24.).min(height / 2.));
    let progress = (-state.scroll_offset().y / maximum).clamp(0., 1.);
    ((height - length) * progress, length)
}

/// A compact document overview with retained, incrementally updated row geometry.
pub struct Minimap {
    source: Entity<EditorState>,
    geometry: Geometry,
    active: bool,
    preview_scroll: ScrollHandle,
    preview: bool,
    preview_list: Option<ListState>,
    drag: Option<Drag>,
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
            drag: None,
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
        self.drag = None;
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
            self.drag = None;
            cx.notify();
        }
    }

    fn begin_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let bounds = self.bounds.get();
        let drag = if position.x < bounds.origin.x + OVERVIEW_WIDTH {
            Drag::Overview
        } else if self.preview {
            Drag::Rail { grab: px(0.) }
        } else {
            let (top, length) = rail_thumb(self.source.read(cx), bounds.size.height);
            let grab = position.y - bounds.origin.y - top;
            if (px(0.)..=length).contains(&grab) {
                self.drag = Some(Drag::Rail { grab });
                return;
            }
            Drag::Rail { grab: length / 2. }
        };
        self.drag = Some(drag);
        self.seek(position.y, drag, cx);
    }

    fn seek(&self, y: Pixels, drag: Drag, cx: &mut Context<Self>) {
        let bounds = self.bounds.get();
        let y = y - bounds.origin.y;
        let height = match drag {
            Drag::Overview => bounds.size.height.min(px(self.geometry.len() as f32 * 2.)),
            Drag::Rail { .. } => bounds.size.height,
        };
        let fraction = (y / height.max(px(1.))).clamp(0., 1.);
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
                let maximum = state.max_vertical_scroll_offset();
                let offset = match drag {
                    Drag::Overview => {
                        let visible = state.input_bounds().size.height;
                        ((maximum + visible) * fraction - visible / 2.).clamp(px(0.), maximum)
                    }
                    Drag::Rail { grab } => {
                        let (_, length) = rail_thumb(state, height);
                        let travel = height - length;
                        if travel <= px(0.) {
                            return;
                        }
                        maximum * ((y - grab) / travel).clamp(0., 1.)
                    }
                };
                state.set_scroll_offset(point(state.scroll_offset().x, -offset), cx);
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
        let minimap = cx.weak_entity();
        let foreground = cx.theme().muted_foreground.opacity(0.45);
        let viewport = cx.theme().primary.opacity(0.12);
        div()
            .id("minimap")
            .w(px(MINIMAP_WIDTH))
            .h_full()
            .cursor_pointer()
            .bg(cx.theme().background)
            .border_l_1()
            .border_color(cx.theme().border)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, _, cx| {
                    view.begin_drag(event.position, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _, _, _| view.drag = None),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _, _, _| view.drag = None),
            )
            .child(
                canvas(
                    move |bounds, _, _| {
                        bounds_cell.set(bounds);
                    },
                    move |bounds, _, window, cx| {
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture
                                && event.pressed_button == Some(MouseButton::Left)
                            {
                                let _ = minimap.update(cx, |view, cx| {
                                    if let Some(drag) = view.drag {
                                        view.seek(event.position.y, drag, cx);
                                        cx.stop_propagation();
                                    }
                                });
                            }
                        });
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
                                size(OVERVIEW_WIDTH, px((height * length.min(1.)).max(3.))),
                            ),
                            viewport,
                        ));
                        if !preview {
                            let state = source.read(cx);
                            let (top, length) = rail_thumb(state, bounds.size.height);
                            if length < bounds.size.height {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        bounds.origin + point(OVERVIEW_WIDTH + px(3.), top),
                                        size(px(8.), length),
                                    ),
                                    foreground,
                                ));
                            }
                        }
                    },
                )
                .size_full(),
            )
    }
}
