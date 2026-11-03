//! Reparse a top-level block for prose edits that cannot change block boundaries.
use super::parse;
use crate::text::{
    document::ParsedDocument,
    node::{BlockNode, NodeContext, Paragraph, SourceSegment, Span},
};
use std::{ops::Range, sync::Arc};

/// Structural edits and document-wide references keep the full parser path.
/// The unchanged line prefix must contain a letter, protecting list ordinals,
/// indentation, fences, table delimiters and other line-opening syntax.
pub(crate) fn reparse(
    previous: &ParsedDocument,
    source: &str,
    cx: &mut NodeContext,
    parsed_bytes: &mut usize,
) -> Option<ParsedDocument> {
    let old = previous.source.as_ref();
    if old.is_empty() || old == source || old.contains("]:") || source.contains("]:") {
        return None;
    }
    let (removed, inserted) = changed_ranges(old, source);
    if !old[removed.clone()]
        .chars()
        .chain(source[inserted.clone()].chars())
        .all(|ch| ch.is_alphanumeric() || ch == ' ')
    {
        return None;
    }
    let line_start = old[..removed.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = old[removed.end..]
        .find('\n')
        .map_or(old.len(), |i| removed.end + i);
    if !old[line_start..removed.start]
        .chars()
        .any(char::is_alphabetic)
        || old[line_start..line_end].contains('<')
    {
        return None;
    }
    let (index, block) = previous.blocks.iter().enumerate().find(|(_, block)| {
        block
            .span()
            .is_some_and(|span| span.start <= removed.start && removed.end <= span.end)
    })?;
    if !editable(block) {
        return None;
    }
    let span = block.span()?;
    let delta = source.len() as isize - old.len() as isize;
    let end = span.end.checked_add_signed(delta)?;
    let range = span.start..end;
    cx.offset = span.start;
    // A span may omit leading indentation or depend on its surrounding parse
    // context. Reuse it only if the old fragment reproduces the stored block.
    *parsed_bytes += span.end - span.start;
    let baseline = parse(old.get(span.start..span.end)?, cx).ok()?;
    if baseline.blocks.as_slice() != std::slice::from_ref(block) {
        return None;
    }
    *parsed_bytes += range.len();
    let replacement = parse(source.get(range.clone())?, cx).ok()?;
    // Besides the lexical gate, require a single block of the original kind.
    // A parser extension or a newly recognized construct may change that kind.
    let [new_block] = replacement.blocks.as_slice() else {
        return None;
    };
    if std::mem::discriminant(block) != std::mem::discriminant(new_block)
        || new_block.span()
            != Some(Span {
                start: span.start,
                end,
            })
    {
        return None;
    }
    let mut blocks = previous.blocks.as_ref().clone();
    blocks[index] = new_block.clone();
    if delta != 0 {
        for block in &mut blocks[index + 1..] {
            shift_block(block, delta)?;
        }
    }
    Some(ParsedDocument {
        source: source.to_owned().into(),
        blocks: Arc::new(blocks),
    })
}

fn changed_ranges(old: &str, new: &str) -> (Range<usize>, Range<usize>) {
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let mut suffix = old.as_bytes()[start..]
        .iter()
        .rev()
        .zip(new.as_bytes()[start..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    (start..old.len() - suffix, start..new.len() - suffix)
}

fn editable(block: &BlockNode) -> bool {
    match block {
        BlockNode::Paragraph(_)
        | BlockNode::Heading { .. }
        | BlockNode::CodeBlock(_)
        | BlockNode::Table(_) => true,
        BlockNode::List { children, .. }
        | BlockNode::ListItem { children, .. }
        | BlockNode::Blockquote { children, .. } => children.iter().all(editable),
        _ => false,
    }
}

fn shift_span(span: &mut Option<Span>, delta: isize) -> Option<()> {
    if let Some(span) = span {
        span.start = span.start.checked_add_signed(delta)?;
        span.end = span.end.checked_add_signed(delta)?;
    }
    Some(())
}

fn shift_segments(segments: &mut [SourceSegment], delta: isize) -> Option<()> {
    for segment in segments {
        segment.source.start = segment.source.start.checked_add_signed(delta)?;
        segment.source.end = segment.source.end.checked_add_signed(delta)?;
    }
    Some(())
}

fn shift_paragraph(paragraph: &mut Paragraph, delta: isize) -> Option<()> {
    shift_span(&mut paragraph.span, delta)?;
    for inline in &mut paragraph.children {
        // Extension payloads are opaque and may retain their own source ranges.
        // Rebuilding the document is required until an extension can rebase them.
        if inline.custom.is_some() {
            return None;
        }
        if let Some(image) = &mut inline.image {
            shift_span(&mut image.span, delta)?;
        }
        shift_segments(&mut inline.source_segments, delta)?;
    }
    Some(())
}

fn shift_block(block: &mut BlockNode, delta: isize) -> Option<()> {
    match block {
        BlockNode::Root { children, span }
        | BlockNode::Blockquote { children, span }
        | BlockNode::List { children, span, .. }
        | BlockNode::ListItem { children, span, .. } => {
            shift_span(span, delta)?;
            for child in children {
                shift_block(child, delta)?;
            }
        }
        BlockNode::Paragraph(paragraph) => shift_paragraph(paragraph, delta)?,
        BlockNode::Heading { children, span, .. } => {
            shift_span(span, delta)?;
            shift_paragraph(children, delta)?;
        }
        BlockNode::CodeBlock(code) => {
            shift_span(&mut code.span, delta)?;
            shift_segments(&mut code.source_segments, delta)?;
        }
        BlockNode::Table(table) => {
            shift_span(&mut table.span, delta)?;
            for row in &mut table.children {
                for cell in &mut row.children {
                    shift_paragraph(&mut cell.children, delta)?;
                }
            }
        }
        BlockNode::Break { span, .. }
        | BlockNode::HorizontalRule { span }
        | BlockNode::Definition { span, .. } => shift_span(span, delta)?,
        BlockNode::Custom(_) | BlockNode::Unknown => return None,
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_edits_match_full_parsing_and_rebase_nested_source_ranges() {
        let original = "# Title\n\nSome **中文** text and `code`.\n\n- item one\n- item two\n\n| name | value |\n| --- | --- |\n| first | second |\n\n```rs\nlet value = 1;\n```\n\nLast ![image](https://example.com/i.png) paragraph.\n";
        for (from, to) in [
            ("中文", "中文你好"),
            ("item one", "item once"),
            ("first", "first value"),
            ("value = 1", "value = 12"),
            ("Some", "So"),
        ] {
            let previous = parse(original, &mut NodeContext::default()).unwrap();
            let source = original.replacen(from, to, 1);
            let mut parsed_bytes = 0;
            let edited = reparse(
                &previous,
                &source,
                &mut NodeContext::default(),
                &mut parsed_bytes,
            )
            .unwrap();
            let full = parse(&source, &mut NodeContext::default()).unwrap();
            assert_eq!(edited, full, "{from} -> {to}");
            assert!(parsed_bytes < source.len());
        }
    }

    #[test]
    fn boundary_and_reference_edits_require_full_parsing() {
        for (old, new) in [
            ("one\n\ntwo", "one\ntwo"),
            ("one\n\ntwo", "# one\n\ntwo"),
            ("- one\n- two", "- one\n  two"),
            (
                "```rs\nlet x = 1;\n```\n\ntail",
                "```rs\nlet x = 1;\n``\n\ntail",
            ),
            ("[link][id]\n\n[id]: /old", "[link][id]\n\n[id]: /new"),
            ("[link][id]\n\n[id]: /old", "[links][id]\n\n[id]: /old"),
            (
                "text <span>word</span>\n\ntail",
                "text <span>words</span>\n\ntail",
            ),
        ] {
            let previous = parse(old, &mut NodeContext::default()).unwrap();
            assert!(
                reparse(&previous, new, &mut NodeContext::default(), &mut 0).is_none(),
                "{old}"
            );
        }
    }

    #[test]
    fn common_byte_prefix_and_suffix_never_split_unicode() {
        for (old, new) in [
            ("abcédef", "abcêdef"),
            ("文字", "文宇"),
            ("x🙂y", "x🙃y"),
            ("ab", "a中文b"),
        ] {
            let (removed, inserted) = changed_ranges(old, new);
            let mut rebuilt = old.to_owned();
            rebuilt.replace_range(removed, &new[inserted]);
            assert_eq!(rebuilt, new);
        }
    }
}
