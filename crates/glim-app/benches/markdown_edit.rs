//! Deterministic parser work and wall-time comparison, without a window or I/O
//! in the measured interval. This does not measure first paint or input latency.
use gpui_kit::base::text::measure_markdown_updates;
use serde_json::json;

fn main() {
    let mut reports = Vec::new();
    for paragraphs in [100, 1_000, 5_000] {
        let source = (0..paragraphs).map(|i| format!(
            "Paragraph {i}: Some **中文内容** and ordinary prose with `code`, [links](https://example.com/{i}), and a final sentence.\n\n"
        )).collect::<String>();
        let mut revisions = Vec::new();
        for _ in 0..4 {
            for i in [0, paragraphs / 2, paragraphs - 1] {
                let marker = format!("Paragraph {i}: Some");
                revisions.push(source.replacen(&marker, &format!("{marker} more"), 1));
                revisions.push(source.replacen(&marker, &format!("Paragraph {i}: So"), 1));
                revisions.push(source.replacen(&marker, &format!("{marker} 中文"), 1));
            }
        }
        let mut result =
            measure_markdown_updates(&source, &revisions).expect("equivalent prose edits");
        result["fixture"] = json!(format!("prose-{paragraphs}"));
        reports.push(result);
    }
    let source = "# Heading\n\nSome **中文** text.\n\n- item one\n- item two\n\n| name | value |\n| --- | --- |\n| first | second |\n\n```rs\nlet value = 1;\n```\n\nLast ![image](https://example.com/i.png) paragraph.\n";
    let revisions = [
        ("中文", "中文你好"),
        ("item one", "item once"),
        ("first", "first value"),
        ("value = 1", "value = 12"),
        ("text.", "text.\n\nnew block"),
        ("```\n\nLast", "``\n\nLast"),
    ]
    .into_iter()
    .map(|(from, to)| source.replacen(from, to, 1))
    .collect::<Vec<_>>();
    let mut result = measure_markdown_updates(source, &revisions).expect("equivalent mixed edits");
    result["fixture"] = json!("mixed-and-structural-fallbacks");
    reports.push(result);
    let source = "Some [reference][id] and a footnote[^note].\n\n[id]: /old\n\n[^note]: A note.\n";
    let revisions = vec![
        source.replace("Some", "Some more"),
        source.replace("/old", "/new"),
        source.replace("A note", "A longer note"),
    ];
    let mut result =
        measure_markdown_updates(source, &revisions).expect("equivalent reference edits");
    result["fixture"] = json!("reference-fallbacks");
    reports.push(result);
    for (name, source, delta) in [
        (
            "append-reference",
            "Some [reference][id].",
            "\n\n[id]: /destination",
        ),
        (
            "split-definition",
            "Some [reference][id].\n\n[id]",
            ": /destination",
        ),
        (
            "append-list",
            "Paragraph.\n\n3. item three",
            "\n4. item four",
        ),
        (
            "append-fence",
            "Paragraph.\n\n```rs\nlet value = 1;",
            "\n```\n\nTail.",
        ),
    ] {
        let mut result = measure_markdown_updates(source, &[format!("{source}{delta}")])
            .expect("equivalent append");
        result["fixture"] = json!(name);
        reports.push(result);
    }
    for (name, source) in [
        (
            "indented-list",
            "  - item one\n    continuation words\n\nTail paragraph.",
        ),
        (
            "blockquote",
            "> Some text and **words**.\n> next line\n\nTail paragraph.",
        ),
        ("setext", "Some heading\n============\n\nTail paragraph."),
        ("crlf", "Some 中文文字.\r\n\r\nTail paragraph.\r\n"),
        (
            "html-boundary",
            "Some words.\n\n<script>\nvalue = 1;\n</script>\n\nTail paragraph.",
        ),
    ] {
        let revisions = source
            .char_indices()
            .filter(|(_, ch)| ch.is_alphanumeric())
            .map(|(i, _)| {
                let mut edited = source.to_owned();
                edited.insert(i, '文');
                edited
            })
            .collect::<Vec<_>>();
        let mut result = measure_markdown_updates(source, &revisions)
            .expect("equivalent edits across syntax boundaries");
        result["fixture"] = json!(name);
        reports.push(result);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema": 1, "measurement": "parser replacement CPU wall time; no UI or I/O",
            "reports": reports,
        }))
        .unwrap()
    );
}
