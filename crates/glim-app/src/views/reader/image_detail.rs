use glim_services::locale::tr;
use gpui_kit::{
    component::{ActiveTheme, Disableable, WindowExt, button::ButtonVariants, h_flex, v_flex},
    *,
};
use std::{any::Any, rc::Rc};

pub(super) fn show(source: ImageSource, window: &mut Window, cx: &mut App) {
    show_pinned(source, (), window, cx);
}

pub(super) fn show_pinned(
    source: ImageSource,
    resource: impl Any,
    window: &mut Window,
    cx: &mut App,
) {
    glim_services::telemetry::reader_feature("embedded_image", "open_detail", []);
    let content = cx.new(|cx| {
        cx.on_release(|view: &mut ImageDetail, cx| {
            if let Some(fullscreen) = view.fullscreen_window {
                let _ = fullscreen.update(cx, |_, window, _| window.remove_window());
            }
        })
        .detach();
        ImageDetail {
            source,
            zoom: 1.,
            scroll: ScrollHandle::new(),
            pan: None,
            zoom_reported: false,
            resource: Rc::new(resource),
            fullscreen_window: None,
            presentation: Presentation::Popup,
            focus: cx.focus_handle(),
            _subscriptions: Vec::new(),
        }
    });
    window.open_dialog(cx, move |dialog, window, _| {
        let size = popup_size(window);
        dialog
            .width(size.width)
            .margin_top((window.viewport_size().height - size.height) / 2.)
            .p_0()
            .close_button(false)
            .child(content.clone())
    });
}

fn popup_size(window: &Window) -> Size<Pixels> {
    let viewport = window.viewport_size();
    size(
        (viewport.width - px(48.)).max(px(0.)).min(px(1120.)),
        (viewport.height - px(64.)).max(px(0.)).min(px(800.)),
    )
}

struct ImageDetail {
    source: ImageSource,
    zoom: f32,
    scroll: ScrollHandle,
    pan: Option<(Point<Pixels>, Point<Pixels>)>,
    zoom_reported: bool,
    resource: Rc<dyn Any>,
    fullscreen_window: Option<AnyWindowHandle>,
    presentation: Presentation,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

enum Presentation {
    Popup,
    Fullscreen {
        parent: AnyWindowHandle,
        seen_fullscreen: bool,
    },
}

impl ImageDetail {
    fn enter_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = self.fullscreen_window
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        let parent = window.window_handle();
        let popup = cx.weak_entity();
        let source = self.source.clone();
        let resource = self.resource.clone();
        let zoom = self.zoom;
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Fullscreen(window.bounds())),
            display_id: window.display(cx).map(|display| display.id()),
            ..Default::default()
        };
        match gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                let bounds = cx.observe_window_bounds(window, |view: &mut Self, window, _| {
                    if let Presentation::Fullscreen {
                        seen_fullscreen, ..
                    } = &mut view.presentation
                    {
                        if window.is_fullscreen() {
                            *seen_fullscreen = true;
                        } else if *seen_fullscreen {
                            // The macOS fullscreen control returns to the original popup too.
                            window.remove_window();
                        }
                    }
                });
                cx.on_release(move |_, cx| {
                    let _ = popup.update(cx, |view, cx| {
                        view.fullscreen_window = None;
                        cx.notify();
                    });
                    let _ = parent.update(cx, |_, window, _| window.activate_window());
                })
                .detach();
                Self {
                    source,
                    zoom,
                    scroll: ScrollHandle::new(),
                    pan: None,
                    zoom_reported: false,
                    resource,
                    fullscreen_window: None,
                    presentation: Presentation::Fullscreen {
                        parent,
                        seen_fullscreen: window.is_fullscreen(),
                    },
                    focus,
                    _subscriptions: vec![bounds],
                }
            })
        }) {
            Ok((handle, _)) => self.fullscreen_window = Some(handle),
            Err(error) => {
                log::error!("Cannot open image fullscreen: {error:#}");
                window.push_notification(tr("Could not open image fullscreen"), cx);
            }
        }
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.presentation {
            Presentation::Popup => window.close_dialog(cx),
            Presentation::Fullscreen { parent, .. } => {
                let _ = parent.update(cx, |_, window, cx| window.close_dialog(cx));
                window.remove_window();
            }
        }
    }

    fn zoom_at(&mut self, zoom: f32, anchor: Point<Pixels>, cx: &mut Context<Self>) {
        if !zoom.is_finite() {
            return;
        }
        if zoom.clamp(1., 8.) != self.zoom && !self.zoom_reported {
            glim_services::telemetry::reader_feature("embedded_image", "zoom", []);
            self.zoom_reported = true;
        }
        let ratio = zoom.clamp(1., 8.) / self.zoom;
        self.zoom = zoom.clamp(1., 8.);
        let offset = self.scroll.offset();
        self.scroll.set_offset(point(
            ((offset.x - anchor.x) * ratio + anchor.x).min(px(0.)),
            ((offset.y - anchor.y) * ratio + anchor.y).min(px(0.)),
        ));
        cx.notify();
    }

    fn zoom_by(&mut self, factor: f32, cx: &mut Context<Self>) {
        let bounds = self.scroll.bounds();
        self.zoom_at(
            self.zoom * factor,
            point(bounds.size.width / 2., bounds.size.height / 2.),
            cx,
        );
    }
}

impl Render for ImageDetail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fullscreen = matches!(self.presentation, Presentation::Fullscreen { .. });
        let size = if fullscreen {
            window.viewport_size()
        } else {
            popup_size(window)
        };
        let viewport = gpui_kit::size(
            (size.width - px(2.)).max(px(0.)),
            (size.height - px(54.)).max(px(0.)),
        );
        v_flex()
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(move |_, event: &KeyDownEvent, window, cx| {
                if fullscreen && event.keystroke.key == "escape" {
                    window.remove_window();
                    cx.stop_propagation();
                }
            }))
            .on_action(
                cx.listener(|view, _: &crate::app::actions::CloseWindow, window, cx| {
                    if matches!(view.presentation, Presentation::Fullscreen { .. }) {
                        window.remove_window();
                        cx.stop_propagation();
                    }
                }),
            )
            .w_full()
            .h(size.height)
            .bg(cx.theme().background)
            .overflow_hidden()
            .child(
                h_flex()
                    .h(px(52.))
                    .flex_shrink_0()
                    .px_3()
                    .gap_2()
                    .child(
                        crate::views::icon_button(
                            "image-zoom-out",
                            gpui_kit::assets::IconName::ZoomOut,
                            tr("Zoom out"),
                        )
                        .disabled(self.zoom <= 1.)
                        .on_click(cx.listener(|view, _, _, cx| view.zoom_by(1. / 1.25, cx))),
                    )
                    .child(
                        crate::views::icon_button(
                            "image-fit",
                            gpui_kit::component::Icon::empty().path("navigation/fit-window.svg"),
                            tr("Fit image"),
                        )
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.zoom = 1.;
                            view.scroll.set_offset(point(px(0.), px(0.)));
                            cx.notify();
                        })),
                    )
                    .child(
                        crate::views::icon_button(
                            "image-zoom-in",
                            gpui_kit::assets::IconName::ZoomIn,
                            tr("Zoom in"),
                        )
                        .disabled(self.zoom >= 8.)
                        .on_click(cx.listener(|view, _, _, cx| view.zoom_by(1.25, cx))),
                    )
                    .child(div().flex_1())
                    .child(
                        crate::views::icon_button(
                            "image-fullscreen",
                            gpui_kit::component::Icon::empty().path(if fullscreen {
                                "navigation/file-fullscreen-exit.svg"
                            } else {
                                "navigation/file-fullscreen.svg"
                            }),
                            if fullscreen {
                                tr("Exit Image Fullscreen")
                            } else {
                                tr("Enter Image Fullscreen")
                            },
                        )
                        .on_click(cx.listener(
                            move |view, _, window, cx| {
                                if fullscreen {
                                    window.remove_window();
                                } else {
                                    view.enter_fullscreen(window, cx);
                                }
                            },
                        )),
                    )
                    .child(
                        crate::views::icon_button(
                            "image-close",
                            gpui_kit::assets::IconName::X,
                            tr("Close"),
                        )
                        .ghost()
                        .on_click(cx.listener(|view, _, window, cx| view.close(window, cx))),
                    ),
            )
            .child(
                div()
                    .id("image-detail-viewport")
                    .w_full()
                    .h(viewport.height)
                    .overflow_scroll()
                    .track_scroll(&self.scroll)
                    .bg(cx.theme().muted)
                    .on_pinch(cx.listener(|view, event: &PinchEvent, _, cx| {
                        view.pan = None;
                        let anchor = event.position - view.scroll.bounds().origin;
                        view.zoom_at(view.zoom * (1. + event.delta), anchor, cx);
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, event: &MouseDownEvent, _, _| {
                            view.pan = Some((event.position, view.scroll.offset()));
                        }),
                    )
                    .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            if let Some((origin, offset)) = view.pan {
                                view.scroll.set_offset(offset + event.position - origin);
                                cx.notify();
                            }
                        } else {
                            view.pan = None;
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|view, _, _, _| view.pan = None),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|view, _, _, _| view.pan = None),
                    )
                    .child(
                        img(self.source.clone())
                            .w(viewport.width * self.zoom)
                            .h(viewport.height * self.zoom)
                            .flex_shrink_0()
                            .object_fit(ObjectFit::Contain),
                    ),
            )
    }
}
