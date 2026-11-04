//! Admit derived graph strings and indices before allocating the published map.
use crate::{limits, work::Cancellation};
use anyhow::{Result, ensure};
use glim_core::{
    graph::{MindMap, PreparedGraph, Sheet, Topic},
    resources::ResourcePool,
    structured::{Node, Tree},
};
use std::path::Path;

fn scalar_fields<'a>(node: &'a Node, nodes: &'a [Node]) -> impl Iterator<Item = &'a Node> + Clone {
    node.children
        .iter()
        .map(|&index| &nodes[index])
        .filter(|child| child.children.is_empty() && child.full_value.is_some())
}
fn branches<'a>(node: &'a Node, nodes: &'a [Node]) -> impl Iterator<Item = usize> + Clone {
    node.children.iter().copied().filter(|&index| {
        node.kind != "object"
            || !nodes[index].children.is_empty()
            || nodes[index].full_value.is_none()
    })
}
fn string_lengths(node: &Node, nodes: &[Node]) -> (usize, usize) {
    let mut title = node.key.len() + 1 + node.value.len();
    let mut notes =
        node.full_key.len() + 1 + node.full_value.as_deref().unwrap_or(&node.value).len();
    if node.kind == "object" {
        for (position, field) in scalar_fields(node, nodes).enumerate() {
            if position < 2 {
                title += 1 + field.key.len() + 2 + field.value.len();
            }
            notes += 1
                + field.full_key.len()
                + 2
                + field.full_value.as_deref().unwrap_or(&field.value).len();
        }
    }
    (title, notes)
}

pub(super) fn prepare(
    path: &Path,
    nodes: &Tree,
    cancel: &Cancellation,
    pool: &ResourcePool,
) -> Result<PreparedGraph> {
    cancel.check()?;
    ensure!(!nodes.is_empty(), "Graph contains no nodes.");
    ensure!(
        nodes.len() <= limits::XMIND_TOPICS,
        "Graph node budget exceeded; use Source."
    );
    {
        // TreeBuilder produces a forest with at most one earlier parent per
        // node, so the traversal stack cannot exceed the total node count.
        let bytes = (nodes.len() * size_of::<(usize, usize)>()) as u64;
        let _memory = pool.reserve_working(bytes, bytes)?;
        let mut pending = Vec::with_capacity(nodes.len());
        pending.push((0usize, 0usize));
        while let Some((index, depth)) = pending.pop() {
            cancel.check()?;
            ensure!(
                depth <= limits::XMIND_NESTING,
                "Graph nesting budget exceeded; use Source."
            );
            pending.extend(
                nodes[index]
                    .children
                    .iter()
                    .map(|&child| (child, depth + 1)),
            );
        }
    }
    let mut bytes = path.as_os_str().len() as u64
        + size_of::<Sheet>() as u64
        + "Graph".len() as u64
        + size_of::<usize>() as u64
        + (nodes.len() * size_of::<Topic>()) as u64;
    for node in nodes.iter() {
        cancel.check()?;
        let (title, notes) = string_lengths(node, nodes);
        bytes += (title
            + notes
            + size_of::<String>()
            + node.kind.len()
            + branches(node, nodes).count() * size_of::<usize>()) as u64;
    }
    // The input tree remains admitted while its derived output is built. Exact
    // capacities avoid string-growth peaks and temporary field/format buffers.
    let mut memory = pool.reserve_working(bytes, bytes)?;
    let mut topics = Vec::with_capacity(nodes.len());
    for node in nodes.iter() {
        cancel.check()?;
        let (title_bytes, notes_bytes) = string_lengths(node, nodes);
        let mut title = String::with_capacity(title_bytes);
        title.push_str(&node.key);
        title.push('\n');
        title.push_str(&node.value);
        let mut notes = String::with_capacity(notes_bytes);
        notes.push_str(&node.full_key);
        notes.push('\n');
        notes.push_str(node.full_value.as_deref().unwrap_or(&node.value));
        if node.kind == "object" {
            for (position, field) in scalar_fields(node, nodes).enumerate() {
                if position < 2 {
                    title.push('\n');
                    title.push_str(&field.key);
                    title.push_str(": ");
                    title.push_str(&field.value);
                }
                notes.push('\n');
                notes.push_str(&field.full_key);
                notes.push_str(": ");
                notes.push_str(field.full_value.as_deref().unwrap_or(&field.value));
            }
        }
        let mut children = Vec::with_capacity(branches(node, nodes).count());
        children.extend(branches(node, nodes));
        topics.push(Topic {
            title,
            notes,
            labels: vec![node.kind.to_owned()],
            link: None,
            children,
        });
    }
    let document = MindMap {
        path: path.to_owned(),
        fingerprint: String::new(),
        sheets: vec![Sheet {
            title: "Graph".into(),
            topics,
            roots: vec![0],
            relationships: Vec::new(),
        }],
    };
    memory.shrink_to(document.heap_bytes());
    Ok(PreparedGraph::new(document, memory))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glim_core::resources::WorkingPressure;

    // Preserve the previous projection semantics as an independent oracle for
    // grouping, full notes, child ordering and UTF-8 string construction.
    fn previous_projection(nodes: &Tree) -> Vec<Topic> {
        nodes
            .iter()
            .map(|node| {
                let grouped = node.kind == "object";
                let fields: Vec<_> = node
                    .children
                    .iter()
                    .copied()
                    .filter(|&i| nodes[i].children.is_empty() && nodes[i].full_value.is_some())
                    .collect();
                let mut title = format!("{}\n{}", node.key, node.value);
                let mut notes = format!(
                    "{}\n{}",
                    node.full_key,
                    node.full_value.as_deref().unwrap_or(&node.value)
                );
                if grouped {
                    for (position, &child) in fields.iter().enumerate() {
                        let field = &nodes[child];
                        if position < 2 {
                            title.push_str(&format!("\n{}: {}", field.key, field.value));
                        }
                        notes.push_str(&format!(
                            "\n{}: {}",
                            field.full_key,
                            field.full_value.as_deref().unwrap_or(&field.value)
                        ));
                    }
                }
                Topic {
                    title,
                    notes,
                    labels: vec![node.kind.into()],
                    link: None,
                    children: node
                        .children
                        .iter()
                        .copied()
                        .filter(|&i| {
                            !grouped
                                || !nodes[i].children.is_empty()
                                || nodes[i].full_value.is_none()
                        })
                        .collect(),
                }
            })
            .collect()
    }

    #[test]
    fn admitted_output_matches_previous_projection_and_outlives_its_input() {
        let pool = ResourcePool::with_budgets(0, 8 * 1024 * 1024);
        let cancel = Cancellation::default();
        let text = r#"{"名字":"中文😀", "空":"", "long":"line\nnext", "items":[1,true,null,{"leaf":"完整值"}], "empty":{}}"#;
        let nodes = crate::json::parse_format_cancellable(text, "json", &cancel, &pool).unwrap();
        let input_bytes = pool.snapshot().working_bytes;
        let expected = previous_projection(&nodes);
        let graph = prepare(Path::new("目录/数据.json"), &nodes, &cancel, &pool).unwrap();
        assert_eq!(graph.sheets[0].topics, expected);
        assert_eq!(
            pool.snapshot().working_bytes,
            input_bytes + graph.heap_bytes()
        );
        drop(nodes);
        let retained = graph.heap_bytes();
        assert_eq!(pool.snapshot().working_bytes, retained);
        let graph = std::thread::spawn(move || graph).join().unwrap();
        assert_eq!(pool.snapshot().working_bytes, retained);
        drop(graph);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }

    #[test]
    fn pressure_and_cancellation_release_output_and_allow_retry() {
        let budget = 128 * 1024;
        let pool = ResourcePool::with_budgets(0, budget);
        let cancel = Cancellation::default();
        let nodes = crate::json::parse_format_cancellable(
            r#"{"a":[1,2,3],"b":"中文"}"#,
            "json",
            &cancel,
            &pool,
        )
        .unwrap();
        let input_bytes = pool.snapshot().working_bytes;
        let prepared = prepare(Path::new("data.json"), &nodes, &cancel, &pool).unwrap();
        let output_bytes = prepared.heap_bytes();
        drop(prepared);
        let blocked_bytes = budget - input_bytes - output_bytes + 1;
        let blocker = pool.reserve_working(blocked_bytes, blocked_bytes).unwrap();
        let before = pool.snapshot().working_bytes;
        let error = prepare(Path::new("data.json"), &nodes, &cancel, &pool)
            .err()
            .unwrap();
        assert!(error.is::<WorkingPressure>(), "{error}");
        assert_eq!(pool.snapshot().working_bytes, before);
        drop(blocker);
        let (cancelled, guard) = Cancellation::task();
        drop(guard);
        assert!(prepare(Path::new("data.json"), &nodes, &cancelled, &pool).is_err());
        assert_eq!(pool.snapshot().working_bytes, input_bytes);
        let graph = prepare(Path::new("data.json"), &nodes, &cancel, &pool).unwrap();
        assert_eq!(graph.sheets[0].roots, [0]);
        drop((graph, nodes));
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
}
