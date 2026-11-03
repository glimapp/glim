//! Bounded JSON structure for an on-demand, read-only tree.
use crate::limits as reader_limits;
use anyhow::{Result, ensure};
use serde_json::Value;

use crate::work::Cancellation;
pub use glim_core::structured::Tree;
use glim_core::{resources::ResourcePool, structured::TreeBuilder};

pub fn parse(text: &str) -> Result<Tree> {
    parse_format(text, "json")
}

pub(crate) fn nodes_for_value(
    value: Value,
    cancel: &Cancellation,
    pool: &ResourcePool,
) -> Result<Tree> {
    let mut nodes = TreeBuilder::new(pool);
    fn add(
        key: String,
        value: Value,
        parent: Option<usize>,
        nodes: &mut TreeBuilder,
        cancel: &Cancellation,
    ) -> Result<usize> {
        cancel.check()?;
        ensure!(
            nodes.len() < reader_limits::JSON_NODES,
            "Tree node count exceeds the preview limit; use Source."
        );
        let (kind, label) = match &value {
            Value::Object(v) => {
                let description = v
                    .get("cell_type")
                    .or_else(|| v.get("type"))
                    .and_then(Value::as_str);
                (
                    "object",
                    match description {
                        Some(description) => format!(
                            "{} · {{ {} keys }}",
                            description
                                .chars()
                                .take(reader_limits::JSON_DESCRIPTION_CHARS)
                                .collect::<String>(),
                            v.len()
                        ),
                        None => format!("{{ {} keys }}", v.len()),
                    },
                )
            }
            Value::Array(v) => {
                let label = if !v.is_empty() && v.iter().all(Value::is_string) {
                    let excerpt: String = v
                        .iter()
                        .filter_map(Value::as_str)
                        .flat_map(str::chars)
                        .take(reader_limits::JSON_ARRAY_SUMMARY_CHARS)
                        .collect();
                    format!(
                        "[ {} items ] {}",
                        v.len(),
                        excerpt.replace(['\n', '\r'], " ")
                    )
                } else {
                    format!("[ {} items ]", v.len())
                };
                ("array", label)
            }
            Value::String(v) => (
                "string",
                format!(
                    "{:?}{}",
                    v.chars()
                        .take(reader_limits::JSON_FIELD_CHARS)
                        .collect::<String>(),
                    if v.chars().count() > reader_limits::JSON_FIELD_CHARS {
                        "…"
                    } else {
                        ""
                    }
                ),
            ),
            Value::Number(v) => ("number", v.to_string()),
            Value::Bool(v) => ("boolean", v.to_string()),
            Value::Null => ("null", "null".into()),
        };
        let full_value = match &value {
            Value::Object(_) | Value::Array(_) => None,
            Value::String(text) => Some(text.as_str()),
            _ => Some(label.as_str()),
        };
        let display_key: String = key.chars().take(reader_limits::JSON_FIELD_CHARS).collect();
        let index = nodes.push(&display_key, &key, &label, full_value, kind, parent)?;
        drop(display_key);
        drop(label);
        drop(key);
        match value {
            Value::Object(values) => {
                for (key, value) in values {
                    add(key, value, Some(index), nodes, cancel)?;
                }
            }
            Value::Array(values) => {
                for (i, value) in values.into_iter().enumerate() {
                    add(format!("[{i}]"), value, Some(index), nodes, cancel)?;
                }
            }
            _ => {}
        }
        Ok(index)
    }
    add("$".into(), value, None, &mut nodes, cancel)?;
    Ok(nodes.finish())
}

/// Strip JSONC comments and trailing commas outside strings without changing offsets.
pub fn parse_jsonc(text: &str) -> Result<Tree> {
    parse_format(text, "jsonc")
}
fn parse_jsonc_with(text: &str, cancel: &Cancellation, pool: &ResourcePool) -> Result<Tree> {
    ensure!(
        text.len() <= reader_limits::JSON_INPUT_BYTES,
        "Tree input exceeds the preview size limit; use Source."
    );
    let _memory = pool.reserve_working(text.len() as u64, text.len() as u64)?;
    let mut bytes = text.as_bytes().to_vec();
    let mut i = 0;
    let mut checkpoint = 0;
    let mut string = false;
    while i < bytes.len() {
        if i >= checkpoint {
            cancel.check()?;
            checkpoint = i.saturating_add(reader_limits::IO_BUFFER_BYTES);
        }
        if string {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                string = false;
            }
            i += 1;
        } else if bytes[i] == b'"' {
            string = true;
            i += 1;
        } else if bytes.get(i..i + 2) == Some(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                if i >= checkpoint {
                    cancel.check()?;
                    checkpoint = i.saturating_add(reader_limits::IO_BUFFER_BYTES);
                }
                bytes[i] = b' ';
                i += 1;
            }
        } else if bytes.get(i..i + 2) == Some(b"/*") {
            bytes[i] = b' ';
            bytes[i + 1] = b' ';
            i += 2;
            while i < bytes.len() && bytes.get(i..i + 2) != Some(b"*/") {
                if i >= checkpoint {
                    cancel.check()?;
                    checkpoint = i.saturating_add(reader_limits::IO_BUFFER_BYTES);
                }
                if !matches!(bytes[i], b'\n' | b'\r') {
                    bytes[i] = b' ';
                }
                i += 1;
            }
            ensure!(i + 1 < bytes.len(), "Unterminated JSONC comment");
            bytes[i] = b' ';
            bytes[i + 1] = b' ';
            i += 2;
        } else {
            i += 1;
        }
    }
    i = 0;
    checkpoint = 0;
    string = false;
    while i < bytes.len() {
        if i >= checkpoint {
            cancel.check()?;
            checkpoint = i.saturating_add(reader_limits::IO_BUFFER_BYTES);
        }
        if string && bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            string = !string;
        }
        if !string && bytes[i] == b',' {
            let mut next = i + 1;
            while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                if next >= checkpoint {
                    cancel.check()?;
                    checkpoint = next.saturating_add(reader_limits::IO_BUFFER_BYTES);
                }
                next += 1;
            }
            if bytes.get(next).is_some_and(|b| matches!(b, b'}' | b']')) {
                bytes[i] = b' ';
            }
        }
        i += 1;
    }
    parse_format_cancellable(std::str::from_utf8(&bytes)?, "json", cancel, pool)
}

pub fn parse_format(text: &str, format: &str) -> Result<Tree> {
    parse_format_cancellable(
        text,
        format,
        &Cancellation::default(),
        crate::resources::pool(),
    )
}

pub fn parse_format_cancellable(
    text: &str,
    format: &str,
    cancel: &Cancellation,
    pool: &ResourcePool,
) -> Result<Tree> {
    cancel.check()?;
    ensure!(
        text.len() <= reader_limits::JSON_INPUT_BYTES,
        "Tree input exceeds the preview size limit"
    );
    let value = match format {
        "json5" => json5::from_str(text)?,
        "ndjson" | "jsonl" => {
            let mut values = Vec::new();
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                cancel.check()?;
                values.push(serde_json::from_str(line)?);
            }
            Value::Array(values)
        }
        "jsonc" => return parse_jsonc_with(text, cancel, pool),
        _ => serde_json::from_str(text)?,
    };
    cancel.check()?;
    nodes_for_value(value, cancel, pool)
}

/// Whitespace-only formatting preserves key order, duplicate keys and numeric literals.
pub fn format_document(text: &str) -> anyhow::Result<String> {
    let _: serde::de::IgnoredAny = serde_json::from_str(text)?;
    let mut compact = Vec::with_capacity(text.len());
    let (mut quoted, mut escaped) = (false, false);
    for ch in text.chars() {
        if quoted {
            compact.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else if ch == '"' {
            quoted = true;
            compact.push(ch);
        } else if !ch.is_ascii_whitespace() {
            compact.push(ch);
        }
    }
    let mut output = String::with_capacity(text.len());
    let (mut depth, mut quoted, mut escaped) = (0_usize, false, false);
    for (index, ch) in compact.iter().copied().enumerate() {
        if quoted {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' => {
                quoted = true;
                output.push(ch);
            }
            '{' | '[' => {
                output.push(ch);
                depth += 1;
                if !matches!(compact.get(index + 1), Some('}' | ']')) {
                    output.push('\n');
                    output.push_str(&"  ".repeat(depth));
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                if index > 0 && !matches!(compact[index - 1], '{' | '[') {
                    output.push('\n');
                    output.push_str(&"  ".repeat(depth));
                }
                output.push(ch);
            }
            ',' => {
                output.push_str(",\n");
                output.push_str(&"  ".repeat(depth));
            }
            ':' => output.push_str(": "),
            _ => output.push(ch),
        }
    }
    output.push('\n');
    Ok(output)
}

#[cfg(test)]
mod jsonc_tests {
    #[test]
    fn comments_do_not_change_strings_and_trailing_commas_are_supported() {
        let nodes = super::parse_jsonc(
            r#"{
            // line comment
            "url": "https://example.invalid/a//b", /* block */
            "literal": "/* text */ \"quoted\"",
            "items": [1, 2,],
        }"#,
        )
        .unwrap();
        assert!(
            nodes
                .iter()
                .any(|node| node.key == "url" && node.value.contains("a//b"))
        );
        assert!(
            nodes
                .iter()
                .any(|node| node.key == "literal" && node.value.contains("/* text */"))
        );
        assert!(super::parse_jsonc("{ /* unfinished").is_err());
        assert!(super::parse("{\"a\":1,}").is_err());
    }
}
