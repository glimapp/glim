//! Local, read-only OPML outlines; subscription URLs are never fetched.
use crate::{limits, work::Cancellation};
use anyhow::{Result, ensure};
use std::path::Path;

pub use glim_core::outline::{Document, Outline};
pub fn supports(path: &Path) -> bool {
    glim_core::formats::FormatSet::Opml.matches(path)
}
fn web_url(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    let url = url::Url::parse(value).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| value.to_string())
}
pub fn read(path: &Path, cancel: &Cancellation) -> Result<Document> {
    let bytes = crate::resources::read_to_end(std::fs::File::open(path)?, cancel)?;
    let _parser_memory = crate::resources::reserve_parser(bytes.len())?;
    ensure!(
        bytes.len() <= limits::OPML_XML_BYTES,
        "OPML exceeds preview XML budget"
    );
    let text = std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))?;
    let document = roxmltree::Document::parse(text)?;
    let root = document.root_element();
    ensure!(root.has_tag_name("opml"), "Not an OPML document");
    let title = root
        .children()
        .find(|n| n.has_tag_name("head"))
        .and_then(|head| head.children().find(|n| n.has_tag_name("title")))
        .and_then(|n| n.text())
        .unwrap_or("")
        .to_string();
    let body = root
        .children()
        .find(|n| n.has_tag_name("body"))
        .ok_or_else(|| anyhow::anyhow!("OPML body is missing"))?;
    let mut nodes: Vec<Outline> = Vec::new();
    let mut roots = Vec::new();
    let mut stack = body
        .children()
        .filter(|n| n.has_tag_name("outline"))
        .rev()
        .map(|n| (n, None::<usize>, 0_usize))
        .collect::<Vec<_>>();
    while let Some((node, parent, depth)) = stack.pop() {
        cancel.check()?;
        crate::resources::check_memory()?;
        ensure!(
            depth < limits::OPML_NESTING && nodes.len() < limits::OPML_NODES,
            "OPML exceeds outline preview budget"
        );
        let index = nodes.len();
        let feed = web_url(node.attribute("xmlUrl"));
        let website = web_url(node.attribute("htmlUrl").or_else(|| node.attribute("url")));
        let title = node
            .attribute("text")
            .filter(|s| !s.trim().is_empty())
            .or_else(|| node.attribute("title"))
            .unwrap_or("")
            .to_string();
        nodes.push(Outline {
            title,
            description: node.attribute("description").unwrap_or("").to_string(),
            feed,
            website,
            children: Vec::new(),
        });
        if let Some(parent) = parent {
            nodes[parent].children.push(index);
        } else {
            roots.push(index);
        }
        stack.extend(
            node.children()
                .filter(|n| n.has_tag_name("outline"))
                .rev()
                .map(|n| (n, Some(index), depth + 1)),
        );
    }
    let stamp = std::fs::metadata(path)?;
    Ok(Document {
        path: path.to_path_buf(),
        fingerprint: format!("opml:{}:{:?}", stamp.len(), stamp.modified().ok()),
        title,
        nodes,
        roots,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_outline_matches_independent_reader_contract() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/reader-contracts");
        let mut actual = read(&root.join("outline.opml"), &Cancellation::default()).unwrap();
        let expected: Document = serde_json::from_str(include_str!(
            "../../../assets/reader-contracts/outline.json"
        ))
        .unwrap();
        actual.path = expected.path.clone();
        actual.fingerprint = expected.fingerprint.clone();
        assert_eq!(actual, expected);
    }
}
