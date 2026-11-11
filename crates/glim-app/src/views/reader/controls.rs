//! Readers contribute controls; the workspace paints one shared capsule.
use gpui_kit::{
    component::{ActiveTheme, v_flex},
    prelude::FluentBuilder,
    *,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

type ComposeToolbar = Rc<dyn Fn(AnyElement, &mut App) -> AnyElement>;

pub(crate) struct ReaderToolbarHost {
    collapsed: bool,
    compose: ComposeToolbar,
    hide_native: Rc<dyn Fn(&Window)>,
    controls: RefCell<Vec<(&'static str, AnyElement)>>,
}
impl ReaderToolbarHost {
    pub(crate) fn new(
        collapsed: bool,
        compose: ComposeToolbar,
        hide_native: Rc<dyn Fn(&Window)>,
    ) -> Self {
        Self {
            collapsed,
            compose,
            hide_native,
            controls: RefCell::new(Vec::new()),
        }
    }
}
#[derive(Default)]
struct ToolbarHosts(HashMap<WindowId, Rc<ReaderToolbarHost>>);
impl Global for ToolbarHosts {}

pub(crate) fn set_toolbar_host(host: ReaderToolbarHost, window: &Window, cx: &mut App) {
    let windows: Vec<_> = cx
        .windows()
        .iter()
        .map(|window| window.window_id())
        .collect();
    if !cx.has_global::<ToolbarHosts>() {
        cx.set_global(ToolbarHosts::default());
    }
    let hosts = &mut cx.global_mut::<ToolbarHosts>().0;
    hosts.retain(|id, _| windows.contains(id));
    hosts.insert(window.window_handle().window_id(), Rc::new(host));
}
fn host(window: &Window, cx: &App) -> Option<Rc<ReaderToolbarHost>> {
    cx.try_global::<ToolbarHosts>()?
        .0
        .get(&window.window_handle().window_id())
        .cloned()
}

#[derive(IntoElement)]
struct ReaderControls {
    id: &'static str,
    controls: AnyElement,
}
impl RenderOnce for ReaderControls {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if let Some(host) = host(window, cx) {
            host.controls.borrow_mut().push((self.id, self.controls));
            div().into_any_element()
        } else {
            capsule(self.controls, false, cx)
        }
    }
}

#[derive(IntoElement)]
pub(crate) struct ReaderToolbarDock;
impl RenderOnce for ReaderToolbarDock {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(host) = host(window, cx) else {
            return div().into_any_element();
        };
        let controls = std::mem::take(&mut *host.controls.borrow_mut());
        if controls.is_empty() {
            return div().into_any_element();
        }
        let groups = v_flex().gap_1().children(
            controls
                .into_iter()
                .map(|(id, control)| div().id(id).child(control)),
        );
        let controls = (host.compose)(groups.into_any_element(), cx);
        let collapsed = host.collapsed;
        div()
            .absolute()
            .size_full()
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| (host.hide_native)(window),
                )
                .absolute()
                .size_full(),
            )
            .child(capsule(controls, collapsed, cx))
            .into_any_element()
    }
}
fn capsule(controls: AnyElement, collapsed: bool, cx: &App) -> AnyElement {
    div()
        .absolute()
        .left(px(16.))
        .right(px(16.))
        .bottom(px(16.))
        .flex()
        .justify_center()
        .when(!collapsed, |view| {
            view.child(
                div()
                    .id("reader-capsule")
                    .max_w(relative(1.))
                    .min_w_0()
                    .px_3()
                    .py_2()
                    .rounded(px(24.))
                    .border_1()
                    .border_color(cx.theme().border.opacity(0.5))
                    .bg(cx.theme().background.opacity(0.96))
                    .shadow_sm()
                    .overflow_x_scroll()
                    .occlude()
                    .child(controls),
            )
        })
        .into_any_element()
}
pub(super) fn floating(
    id: &'static str,
    controls: impl IntoElement,
    _cx: &App,
) -> impl IntoElement {
    ReaderControls {
        id,
        controls: controls.into_any_element(),
    }
}
pub(super) fn loading(label: String, cx: &App) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(label)
        .into_any_element()
}
