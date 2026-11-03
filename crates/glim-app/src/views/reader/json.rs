use crate::app::reader_jobs::{self, JobClass, Priority};
use glim_services::json::JsonNode;
use glim_services::limits as reader_limits;
use gpui_kit::{
    base::InteractiveElementExt as _,
    component::{ActiveTheme, Icon, v_flex},
    prelude::FluentBuilder,
    *,
};
use std::collections::HashSet;

pub(super) struct JsonTree {
    memory: glim_services::resources::ResourceLease,
    suspended: bool,
    initialized: bool,
    resume_scroll: Option<Point<Pixels>>,
    nodes: Vec<JsonNode>,
    expanded: HashSet<usize>,
    rows: Vec<(usize, usize)>,
    task: Option<Task<()>>,
    error: Option<String>,
    scroll: UniformListScrollHandle,
    format: String,
    loaded_text: Option<String>,
}
impl JsonTree {
    pub fn new(format: String) -> Self {
        Self {
            memory: glim_services::resources::pool()
                .track(0, glim_services::resources::ResourceClass::Working),
            suspended: false,
            initialized: false,
            resume_scroll: None,
            nodes: Vec::new(),
            expanded: HashSet::new(),
            rows: Vec::new(),
            task: None,
            error: None,
            scroll: UniformListScrollHandle::new(),
            format,
            loaded_text: None,
        }
    }
    pub fn load(&mut self, text: String, cx: &mut Context<Self>) {
        let same = self.loaded_text.as_deref() == Some(text.as_str());
        if same && !self.suspended {
            return;
        }
        self.suspended = false;
        let preserve_expansion = same && self.initialized;
        if !same {
            self.resume_scroll = None;
            self.initialized = false;
        }
        self.loaded_text = Some(text.clone());
        let format = self.format.clone();
        self.nodes.clear();
        self.rows.clear();
        if !same {
            self.expanded.clear();
        }
        self.error = None;
        self.account();
        self.scroll = UniformListScrollHandle::new();
        let telemetry_started = std::time::Instant::now();
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |_cancel| glim_services::json::parse_format(&text, &format),
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result("data_tree_load", None, telemetry_started, &result);
            let _ = view.update(cx, |v, cx| {
                v.task = None;
                match result {
                    Ok(nodes) => {
                        v.nodes = nodes;
                        v.initialized = true;
                        if !preserve_expansion {
                            v.expanded.insert(0);
                        }
                        if !preserve_expansion && let Some(root) = v.nodes.first() {
                            for &child in root
                                .children
                                .iter()
                                .take(reader_limits::JSON_AUTO_EXPAND_CHILDREN)
                            {
                                if v.nodes[child].children.len()
                                    <= reader_limits::JSON_AUTO_EXPAND_CHILDREN
                                {
                                    v.expanded.insert(child);
                                }
                            }
                        }
                        v.rebuild();
                        if let Some(offset) = v.resume_scroll.take() {
                            v.scroll.0.borrow().base_handle.set_offset(offset);
                        }
                    }
                    Err(e) => {
                        v.error = Some(format!(
                            "Cannot show JSON tree: {e}. Switch to Source to view or edit."
                        ))
                    }
                }
                v.account();
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub fn suspend(&mut self) {
        if !self.suspended && self.task.is_none() && self.error.is_none() {
            self.resume_scroll = Some(self.scroll.0.borrow().base_handle.offset());
        }
        self.suspended = true;
        self.task = None;
        self.nodes = Vec::new();
        self.rows = Vec::new();
        self.account();
    }
    pub fn resume(&mut self, cx: &mut Context<Self>) {
        if self.suspended
            && let Some(text) = self.loaded_text.clone()
        {
            self.load(text, cx);
        }
    }
    fn account(&self) {
        let nodes = (self.nodes.capacity() * std::mem::size_of::<JsonNode>()) as u64
            + self
                .nodes
                .iter()
                .map(|node| {
                    (node.key.capacity() + node.full_key.capacity() + node.value.capacity()) as u64
                        + node.full_value.as_ref().map_or(0, |v| v.capacity() as u64)
                        + (node.children.capacity() * std::mem::size_of::<usize>()) as u64
                })
                .sum::<u64>();
        self.memory.resize(
            nodes
                + self.loaded_text.as_ref().map_or(0, |v| v.capacity() as u64)
                + (self.rows.capacity() * std::mem::size_of::<(usize, usize)>()) as u64
                + (self.expanded.capacity() * std::mem::size_of::<usize>()) as u64,
        );
    }
    fn rebuild(&mut self) {
        self.rows.clear();
        if self.nodes.is_empty() {
            return;
        }
        let mut pending = vec![(0, 0)];
        while let Some((index, depth)) = pending.pop() {
            self.rows.push((index, depth));
            if self.expanded.contains(&index) {
                pending.extend(
                    self.nodes[index]
                        .children
                        .iter()
                        .rev()
                        .map(|i| (*i, depth + 1)),
                );
            }
        }
        self.account();
    }
}
impl Render for JsonTree {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .min_h_0()
            .when(self.task.is_some(), |v| {
                v.child(
                    div()
                        .p_4()
                        .child(glim_services::locale::tr("Loading JSON tree…")),
                )
            })
            .when_some(self.error.clone(), |v, error| {
                v.child(
                    div()
                        .p_4()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(error),
                )
            })
            .child(
                uniform_list(
                    "json-tree",
                    self.rows.len(),
                    cx.processor(|v, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|row| {
                                let (index, depth) = v.rows[row];
                                let node = &v.nodes[index];
                                let expandable = !node.children.is_empty();
                                let expanded = v.expanded.contains(&index);
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
                                        glim_services::telemetry::reader_feature(
                                            "data_tree",
                                            "value_detail",
                                            [],
                                        );
                                        if let Some(node) = view.nodes.get(index) {
                                            let text = node.full_value.as_ref().map_or_else(
                                                || node.full_key.clone(),
                                                |value| format!("{}\n\n{value}", node.full_key),
                                            );
                                            super::source::show_text_dialog(
                                                glim_services::locale::tr("Full cell value"),
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
                                            glim_services::telemetry::reader_feature(
                                                "data_tree",
                                                "toggle_node",
                                                [(
                                                    "expanded",
                                                    glim_services::telemetry::flag(
                                                        !v.expanded.contains(&index),
                                                    ),
                                                )],
                                            );
                                            if !v.expanded.remove(&index) {
                                                v.expanded.insert(index);
                                            }
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
    use super::JsonTree;
    use glim_core::resources::{ResourceClass, ResourcePool};
    #[test]
    fn suspension_releases_nodes_but_preserves_source_and_fold_state() {
        let pool = ResourcePool::new(0);
        let mut tree = JsonTree::new("json".into());
        tree.memory = pool.track(0, ResourceClass::Working);
        let source = r#"{"group":[1,2,3]}"#.to_owned();
        tree.nodes = glim_services::json::parse(&source).unwrap();
        tree.loaded_text = Some(source.clone());
        tree.initialized = true;
        tree.expanded.insert(0);
        tree.rebuild();
        let before = pool.snapshot().working_bytes;
        tree.suspend();
        assert!(tree.nodes.is_empty() && tree.rows.is_empty());
        assert_eq!(tree.loaded_text.as_deref(), Some(source.as_str()));
        assert!(tree.expanded.contains(&0));
        assert!(tree.initialized);
        assert!(pool.snapshot().working_bytes < before);
        drop(tree);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
}
