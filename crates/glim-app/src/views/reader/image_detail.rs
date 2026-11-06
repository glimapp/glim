use glim_services::locale::tr;
use gpui_kit::{
    component::{ActiveTheme, Disableable, WindowExt, button::ButtonVariants, h_flex, v_flex},
    *,
};
use std::any::Any;

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
    let content = cx.new(|_| ImageDetail {
        source,
        zoom: 1.,
        scroll: ScrollHandle::new(),
        pan: None,
        zoom_reported: false,
        _resource: Box::new(resource),
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
    _resource: Box<dyn Any>,
}

impl ImageDetail {
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
        let size = popup_size(window);
        let viewport = gpui_kit::size(size.width - px(2.), (size.height - px(54.)).max(px(0.)));
        v_flex()
            .w_full()
            .h(size.height)
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
                            gpui_kit::assets::IconName::Maximize,
                            tr("Fit"),
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
                            "image-close",
                            gpui_kit::assets::IconName::X,
                            tr("Close"),
                        )
                        .ghost()
                        .on_click(|_, window, cx| window.close_dialog(cx)),
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
