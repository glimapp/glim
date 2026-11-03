use crate::views::reader::contract::{ReaderHost, ReaderMetric};
use glim_core::{
    resources::{WorkingPressure, WorkingReservation},
    structured::Tree,
};
use gpui_kit::{
    base::InteractiveElementExt as _,
    component::{ActiveTheme, Icon, v_flex},
    prelude::FluentBuilder,
    *,
};

pub(super) struct JsonView {
    host: ReaderHost,
    show_detail: fn(String, String, &mut Window, &mut App),
    auto_expand_children: usize,
    pub(super) initialized: bool,
    resume_scroll: Option<Point<Pixels>>,
    nodes: Tree,
    expanded: Vec<u8>,
    rows: Vec<(usize, usize)>,
    pending: Vec<(usize, usize)>,
    scroll: UniformListScrollHandle,
    memory: Option<WorkingReservation>,
    fold_memory: Option<WorkingReservation>,
}
impl JsonView {
    pub fn new(
        host: ReaderHost,
        auto_expand_children: usize,
        show_detail: fn(String, String, &mut Window, &mut App),
    ) -> Self {
        Self {
            host,
            show_detail,
            auto_expand_children,
            fold_memory: None,
            initialized: false,
            resume_scroll: None,
            nodes: Tree::default(),
            expanded: Vec::new(),
            rows: Vec::new(),
            pending: Vec::new(),
            scroll: UniformListScrollHandle::new(),
            memory: None,
        }
    }
    pub fn publish(
        &mut self,
        nodes: Tree,
        preserve_expansion: bool,
    ) -> Result<(), WorkingPressure> {
        let reset_folds = !preserve_expansion || self.expanded.len() != nodes.len();
        let folds = if reset_folds {
            let bytes = nodes.len() as u64;
            let memory = self.host.resources.reserve_working(bytes, bytes)?;
            Some((vec![0_u8; nodes.len()], memory))
        } else {
            None
        };
        // TreeBuilder links each node to at most one earlier parent, so both
        // traversal storage and visible rows are bounded by the node count.
        let bytes = (2 * nodes.len() * size_of::<(usize, usize)>()) as u64;
        let memory = self.host.resources.reserve_working(bytes, bytes)?;
        let rows = Vec::with_capacity(nodes.len());
        let pending = Vec::with_capacity(nodes.len());
        self.nodes = nodes;
        self.rows = rows;
        self.pending = pending;
        self.memory = Some(memory);
        if let Some((expanded, memory)) = folds {
            self.expanded = expanded;
            self.fold_memory = Some(memory);
            if let Some(root) = self.nodes.first() {
                self.expanded[0] = 1;
                for &child in root.children.iter().take(self.auto_expand_children) {
                    if self.nodes[child].children.len() <= self.auto_expand_children {
                        self.expanded[child] = 1;
                    }
                }
            }
        }
        self.initialized = true;
        self.scroll = UniformListScrollHandle::new();
        self.rebuild();
        if let Some(offset) = self.resume_scroll.take() {
            self.scroll.0.borrow().base_handle.set_offset(offset);
        }
        Ok(())
    }
    pub fn clear(&mut self, reset: bool) {
        self.nodes = Tree::default();
        self.rows = Vec::new();
        self.pending = Vec::new();
        self.memory = None;
        if reset {
            self.initialized = false;
            self.resume_scroll = None;
            self.expanded = Vec::new();
            self.fold_memory = None;
        }
    }
    pub fn suspend(&mut self, remember_scroll: bool) {
        if remember_scroll {
            self.resume_scroll = Some(self.scroll.0.borrow().base_handle.offset());
        }
        self.clear(false);
    }
    fn rebuild(&mut self) {
        self.rows.clear();
        self.pending.clear();
        if !self.nodes.is_empty() {
            self.pending.push((0, 0));
        }
        while let Some((index, depth)) = self.pending.pop() {
            self.rows.push((index, depth));
            if self.expanded[index] != 0 {
                self.pending.extend(
                    self.nodes[index]
                        .children
                        .iter()
                        .rev()
                        .map(|i| (*i, depth + 1)),
                );
            }
        }
    }
}
impl Render for JsonView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().size_full().min_h_0().child(
            uniform_list(
                "json-tree",
                self.rows.len(),
                cx.processor(|v, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|row| {
                            let (index, depth) = v.rows[row];
                            let node = &v.nodes[index];
                            let expandable = !node.children.is_empty();
                            let expanded = v.expanded[index] != 0;
                            let color = match node.kind {
                                "string" => cx.theme().green,
                                "number" => cx.theme().blue,
                                "boolean" => cx.theme().magenta,
                                _ => cx.theme().muted_foreground,
                            };
                            div()
                                .id(("json-node", index))
                                .flex()
                                .items_center()
                                .gap_2()
                                .h(px(32.))
                                .font_family("Menlo")
                                .when(row % 2 == 1, |row| {
                                    row.bg(cx.theme().foreground.opacity(0.025))
                                })
                                .pl(px(12. + depth.min(20) as f32 * 16.))
                                .pr_3()
                                .text_sm()
                                .on_double_click(cx.listener(move |view, _, window, cx| {
                                    (view.host.action)("data_tree", "value_detail", &[]);
                                    if let Some(node) = view.nodes.get(index) {
                                        let text = node.full_value.as_ref().map_or_else(
                                            || node.full_key.clone(),
                                            |value| format!("{}\n\n{value}", node.full_key),
                                        );
                                        (view.show_detail)(
                                            (view.host.translate)("Full cell value"),
                                            text,
                                            window,
                                            cx,
                                        );
                                    }
                                }))
                                .when(expandable, |row| row.cursor_pointer())
                                .child(div().w(px(14.)).flex_shrink_0().when(expandable, |v| {
                                    v.child(Icon::new(if expanded {
                                        gpui_kit::assets::IconName::ChevronDown
                                    } else {
                                        gpui_kit::assets::IconName::ChevronRight
                                    }))
                                }))
                                .child(
                                    div()
                                        .max_w(relative(0.45))
                                        .truncate()
                                        .text_color(cx.theme().blue)
                                        .child(node.key.clone()),
                                )
                                .child(div().text_color(cx.theme().muted_foreground).child(":"))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_color(color)
                                        .child(node.value.clone()),
                                )
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    if expandable {
                                        (v.host.action)(
                                            "data_tree",
                                            "toggle_node",
                                            &[ReaderMetric::Flag(
                                                "expanded",
                                                v.expanded[index] == 0,
                                            )],
                                        );
                                        v.expanded[index] ^= 1;
                                        v.rebuild();
                                        cx.notify();
                                    }
                                }))
                        })
                        .collect()
                }),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .w_full(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonView, ReaderHost};
    use glim_core::{resources::ResourcePool, structured::TreeBuilder};

    #[test]
    fn suspension_releases_projection_and_preserves_fold_state() {
        let pool = ResourcePool::with_budgets(0, 8192);
        let host = ReaderHost {
            resources: pool.clone(),
            translate: str::to_owned,
            action: |_, _, _| {},
        };
        let mut view = JsonView::new(host, 8, |_, _, _, _| {});
        let mut nodes = TreeBuilder::new(&pool);
        nodes
            .push("$", "$", "object", None, "object", None)
            .unwrap();
        nodes
            .push("group", "group", "array", None, "array", Some(0))
            .unwrap();
        nodes
            .push("[0]", "[0]", "1", Some("1"), "number", Some(1))
            .unwrap();
        view.publish(nodes.finish(), false).unwrap();
        assert_eq!(view.rows.len(), 3);
        assert!(pool.snapshot().working_bytes > 0);
        view.suspend(true);
        assert!(view.nodes.is_empty() && view.rows.is_empty());
        assert_eq!(&view.expanded[..2], &[1, 1]);
        assert!(view.initialized);
        assert_eq!(pool.snapshot().working_bytes, 3);
        drop(view);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
}
