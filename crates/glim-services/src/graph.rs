//! Local structured-data graph projection. No network, embedded scripts or source writes.
mod projection;
use crate::{limits, work::Cancellation};
use anyhow::{Result, ensure};
use glim_core::graph::PreparedGraph;
use serde::Deserialize;
use std::path::Path;

pub fn supports(path: &Path) -> bool {
    glim_core::formats::FormatSet::StructuredGraph.matches(path)
}
pub fn read(path: &Path, text: Option<String>, cancel: &Cancellation) -> Result<PreparedGraph> {
    cancel.check()?;
    let text = match text {
        Some(text) => text,
        None => {
            let input = crate::work::Reader {
                inner: std::fs::File::open(path)?,
                cancel: cancel.clone(),
            };
            String::from_utf8(crate::resources::read_to_end(input, cancel)?)?
        }
    };
    ensure!(
        text.len() <= limits::JSON_INPUT_BYTES,
        "Graph input exceeds the preview budget; use Source."
    );
    let format = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let nodes = match format.as_str() {
        "yaml" | "yml" => {
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
            crate::json::nodes_for_value(value, cancel, crate::resources::pool())?
        }
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
            crate::json::nodes_for_value(
                serde_json::Value::Array(rows),
                cancel,
                crate::resources::pool(),
            )?
        }
        "xml" => xml_nodes(text, cancel)?,
        _ => {
            crate::json::parse_format_cancellable(text, &format, cancel, crate::resources::pool())?
        }
    };
    projection::prepare(path, &nodes, cancel, crate::resources::pool())
}

fn xml_nodes(text: &str, cancel: &Cancellation) -> Result<glim_core::structured::Tree> {
    let document = roxmltree::Document::parse(text)?;
    let mut nodes = glim_core::structured::TreeBuilder::new(crate::resources::pool());
    let mut pending = vec![(document.root_element(), None::<usize>, 0usize)];
    while let Some((element, parent, depth)) = pending.pop() {
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
            ensure!(
                nodes.len() < limits::XMIND_TOPICS,
                "Graph node budget exceeded; use Source."
            );
            let label: String = value.chars().take(limits::JSON_FIELD_CHARS).collect();
            nodes.push(&key, &key, &label, Some(value), "string", Some(index))?;
        }
        pending.extend(
            element
                .children()
                .rev()
                .filter(|n| n.is_element())
                .map(|n| (n, Some(index), depth + 1)),
        );
    }
    Ok(nodes.finish())
}
