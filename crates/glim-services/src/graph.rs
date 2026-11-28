//! Local structured-data graph projection. No network, embedded scripts or source writes.
mod projection;
use crate::{limits, work::Cancellation};
use anyhow::{Result, ensure};
use glim_core::{
    graph::PreparedGraph,
    resources::ResourcePool,
    structured::{Tree, TreeBuilder},
};
use serde::Deserialize;
use std::path::Path;

pub fn supports(path: &Path) -> bool {
    glim_core::formats::FormatSet::StructuredGraph.matches(path)
}
pub fn read(path: &Path, text: Option<String>, cancel: &Cancellation) -> Result<PreparedGraph> {
    cancel.check()?;
    let (source, input_memory) = match text {
        Some(text) => (text, None),
        None => {
            let input = crate::work::Reader {
                inner: std::fs::File::open(path)?,
                cancel: cancel.clone(),
            };
            let (bytes, reservation) = crate::resources::read_reserved(input, cancel)?;
            (String::from_utf8(bytes)?, Some(reservation))
        }
    };
    ensure!(
        source.len() <= limits::JSON_INPUT_BYTES,
        "Graph input exceeds the preview budget; use Source."
    );
    let format = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let text = source.strip_prefix('\u{feff}').unwrap_or(&source);
    let pool = crate::resources::pool();
    let nodes = match format.as_str() {
        "yaml" | "yml" => yaml_nodes(text, cancel, pool)?,
        "csv" | "tsv" => {
            // Keep every column and duplicate header by representing records as arrays.
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(false)
                .delimiter(if format == "tsv" { b'\t' } else { b',' })
                .from_reader(text.as_bytes());
            let mut rows = Vec::new();
            let mut count = 1usize;
            for record in reader.records() {
                cancel.check()?;
                let record = record?;
                count = count.saturating_add(record.len() + 1);
                ensure!(
                    count <= limits::XMIND_TOPICS,
                    "Graph node budget exceeded; use Source or Table."
                );
                rows.push(serde_json::Value::Array(
                    record
                        .iter()
                        .map(|v| serde_json::Value::String(v.into()))
                        .collect(),
                ));
            }
            crate::json::nodes_for_value(serde_json::Value::Array(rows), cancel, pool)?
        }
        "xml" => xml_nodes(text, cancel, pool)?,
        _ => crate::json::parse_format_cancellable(text, &format, cancel, pool)?,
    };
    drop((source, input_memory));
    projection::prepare(path, &nodes, cancel, pool)
}

fn yaml_nodes(text: &str, cancel: &Cancellation, pool: &ResourcePool) -> Result<Tree> {
    cancel.check()?;
    let _parser_memory = crate::resources::reserve_parser_with_pool(text.len(), pool)?;
    let mut documents = Vec::new();
    for document in serde_yaml_ng::Deserializer::from_str(text) {
        cancel.check()?;
        let value = serde_yaml_ng::Value::deserialize(document)?;
        documents.push(serde_json::to_value(value)?);
    }
    let value = if documents.len() == 1 {
        documents.remove(0)
    } else {
        serde_json::Value::Array(documents)
    };
    crate::json::nodes_for_value(value, cancel, pool)
}

fn xml_nodes(text: &str, cancel: &Cancellation, pool: &ResourcePool) -> Result<Tree> {
    cancel.check()?;
    let _parser_memory = crate::resources::reserve_parser_with_pool(text.len(), pool)?;
    let document = roxmltree::Document::parse(text)?;
    let mut nodes = TreeBuilder::new(pool);

    fn append(
        element: roxmltree::Node<'_, '_>,
        parent: Option<usize>,
        depth: usize,
        nodes: &mut TreeBuilder,
        cancel: &Cancellation,
    ) -> Result<()> {
        cancel.check()?;
        ensure!(
            depth <= limits::XMIND_NESTING,
            "Graph nesting budget exceeded; use Source."
        );
        ensure!(
            nodes.len() < limits::XMIND_TOPICS,
            "Graph node budget exceeded; use Source."
        );
        let name = element.tag_name().name();
        let index = nodes.push(name, name, "element", None, "element", parent)?;
        for (key, value) in element
            .attributes()
            .map(|a| (format!("@{}", a.name()), a.value()))
            .chain(
                element
                    .children()
                    .filter(|n| n.is_text())
                    .filter_map(|n| n.text())
                    .filter(|t| !t.trim().is_empty())
                    .map(|t| ("#text".into(), t)),
            )
        {
            cancel.check()?;
            ensure!(
                nodes.len() < limits::XMIND_TOPICS,
                "Graph node budget exceeded; use Source."
            );
            let label: String = value.chars().take(limits::JSON_FIELD_CHARS).collect();
            nodes.push(&key, &key, &label, Some(value), "string", Some(index))?;
        }
        for child in element.children().filter(|node| node.is_element()) {
            append(child, Some(index), depth + 1, nodes, cancel)?;
        }
        Ok(())
    }

    append(document.root_element(), None, 0, &mut nodes, cancel)?;
    Ok(nodes.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_and_xml_keep_parser_admission_until_tree_handoff() {
        let cancel = Cancellation::default();
        let cases = [
            (
                yaml_nodes as fn(&str, &Cancellation, &ResourcePool) -> Result<Tree>,
                "root: 中文\n",
            ),
            (xml_nodes, "<root>中文</root>"),
        ];
        for (parse, input) in cases {
            let input = format!("{input}{}", " ".repeat(512));
            let pool = ResourcePool::with_budgets(0, 0);
            let error = parse(&input, &cancel, &pool).err().unwrap();
            assert!(crate::resources::is_memory_pressure(&error));
            assert_eq!(pool.snapshot().working_bytes, 0);

            let pool = ResourcePool::new(0);
            let parser = crate::resources::reserve_parser_with_pool(input.len(), &pool).unwrap();
            let parser_bytes = parser.bytes();
            drop(parser);
            let parser_only = ResourcePool::with_budgets(0, parser_bytes);
            let error = parse(&input, &cancel, &parser_only).err().unwrap();
            assert!(crate::resources::is_memory_pressure(&error));
            assert_eq!(parser_only.snapshot().working_bytes, 0);

            let tree = parse(&input, &cancel, &pool).unwrap();
            assert_eq!(tree[1].full_value.as_deref(), Some("中文"));
            let retained = pool.snapshot().working_bytes;
            assert!(retained > 0 && retained < parser_bytes);
            drop(tree);
            assert_eq!(pool.snapshot().working_bytes, 0);
        }
    }

    #[test]
    fn xml_graph_preserves_attribute_text_and_element_order() {
        let tree = xml_nodes(
            "<root attr='attribute'>before<first/>after<second><nested/></second></root>",
            &Cancellation::default(),
            &ResourcePool::new(0),
        )
        .unwrap();
        let keys = tree
            .iter()
            .map(|node| node.key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            [
                "root", "@attr", "#text", "#text", "first", "second", "nested"
            ]
        );
        assert_eq!(tree[0].children, [1, 2, 3, 4, 5]);
        assert_eq!(tree[5].children, [6]);
        assert_eq!(tree[2].full_value.as_deref(), Some("before"));
        assert_eq!(tree[3].full_value.as_deref(), Some("after"));
    }
}
