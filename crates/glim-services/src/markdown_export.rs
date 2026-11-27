//! Renders Markdown and its assets for PDF printing and standalone HTML.
use crate::{
    inline_images, limits, markdown_preview as diagrams, markup_images,
    work::{self, Cancellation},
};
use anyhow::{Context, Result, ensure};
use base64::{engine::general_purpose::STANDARD, write::EncoderWriter};
use diagrams::{Diagram, DiagramSvg};
use pulldown_cmark::{BlockQuoteKind, CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use url::Url;

pub const PRINT_ARGUMENT: &str = "--print-markdown-pdf";

#[path = "markdown_export_html.rs"]
mod raw_html;

pub fn export_pdf(
    markdown: &str,
    base: &Url,
    title: &str,
    destination: &crate::files::DraftDestination,
    cancel: &Cancellation,
) -> Result<usize> {
    let directory = tempfile::tempdir()?;
    let html_path = directory.path().join("document.html");
    let pdf_path = directory.path().join("document.pdf");
    let unavailable = write_html(
        markdown,
        base,
        title,
        directory.path(),
        File::create(&html_path)?,
        false,
        cancel,
    )?;
    let mut command = Command::new(diagrams::renderer_executable()?);
    command
        .arg(PRINT_ARGUMENT)
        .arg(&html_path)
        .arg(&pdf_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    ensure!(
        work::run(&mut command, cancel)?.success(),
        "{}",
        crate::locale::tr("Cannot create PDF pages")
    );
    crate::files::publish_document(destination, cancel, |output| {
        cancel.copy(File::open(&pdf_path)?, output)?;
        Ok(())
    })?;
    Ok(unavailable)
}

pub fn export_html_to(
    markdown: &str,
    base: &Url,
    title: &str,
    destination: &crate::files::DraftDestination,
    cancel: &Cancellation,
) -> Result<usize> {
    let scratch = tempfile::tempdir()?;
    let mut unavailable = 0;
    crate::files::publish_document(destination, cancel, |output| {
        unavailable = write_html(markdown, base, title, scratch.path(), output, true, cancel)?;
        Ok(())
    })?;
    Ok(unavailable)
}

pub fn export_html(
    markdown: &str,
    base: &Url,
    title: &str,
    cancel: &Cancellation,
) -> Result<(PathBuf, usize)> {
    cancel.check()?;
    let directory = crate::preferences::directory()?.join("MarkdownPreviews");
    fs::create_dir_all(&directory)?;
    let scratch = tempfile::tempdir()?;
    let mut output = tempfile::Builder::new()
        .prefix("preview-")
        .suffix(".partial")
        .tempfile_in(&directory)?;
    let unavailable = write_html(
        markdown,
        base,
        title,
        scratch.path(),
        output.as_file_mut(),
        true,
        cancel,
    )?;
    let path = output.path().with_extension("html");
    cancel.check()?;
    output.persist_noclobber(&path)?;
    Ok((path, unavailable))
}

fn write_html(
    markdown: &str,
    base: &Url,
    title: &str,
    directory: &Path,
    output: impl Write,
    embedded: bool,
    cancel: &Cancellation,
) -> Result<usize> {
    cancel.check()?;
    let markdown = diagrams::normalize_tables(markdown);
    let mut assets = Assets {
        directory,
        diagrams: HashMap::new(),
        images: HashMap::new(),
        next_asset: 0,
        unavailable: 0,
        embedded,
        cancel,
    };
    assets.prepare_diagrams(&markdown)?;
    let mut output = BufWriter::new(output);
    let (image_source, screen_style) = if embedded {
        ("data:", include_str!("markdown_browser.css"))
    } else {
        ("file:", "")
    };
    write!(
        output,
        "<!doctype html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src {image_source}\"><title>{}</title><style>{}{screen_style}</style><body>",
        escaped(title),
        include_str!("markdown_pdf.css")
    )?;
    let mut headings = HashMap::<String, usize>::new();
    let events = document_events(&markdown).map_while(|event| {
        cancel.check().ok()?;
        Some(match event {
            DocumentEvent::Diagram(diagram) => Event::Html(assets.diagram_html(&diagram).into()),
            DocumentEvent::Markdown(Event::Start(Tag::BlockQuote(Some(kind))), _) => {
                let (class, label) = match kind {
                    BlockQuoteKind::Note => ("note", "Note"),
                    BlockQuoteKind::Tip => ("tip", "Tip"),
                    BlockQuoteKind::Important => ("important", "Important"),
                    BlockQuoteKind::Warning => ("warning", "Warning"),
                    BlockQuoteKind::Caution => ("caution", "Caution"),
                };
                Event::Html(format!(
                    "<blockquote class=\"markdown-alert-{class}\"><p class=\"markdown-alert-title\">{}</p>",
                    escaped(&crate::locale::tr(label))
                ).into())
            }
            DocumentEvent::Markdown(Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs,
            }), source) => {
                let slug = source.lines().next().unwrap_or_default()
                    .trim().trim_start_matches('#').trim().trim_end_matches('#').trim()
                    .to_lowercase().chars()
                    .filter(|c| c.is_alphanumeric() || c.is_whitespace() || matches!(c, '-' | '_'))
                    .map(|c| if c.is_whitespace() { '-' } else { c })
                    .collect::<String>();
                let count = headings.entry(slug.clone()).or_default();
                let slug = if *count == 0 { slug } else { format!("{slug}-{count}") };
                *count += 1;
                Event::Start(Tag::Heading { level, id: id.or(Some(slug.into())), classes, attrs })
            }
            DocumentEvent::Markdown(Event::Start(Tag::Image { link_type, dest_url, title, id }), _) => {
                let path = assets.image(base, &dest_url);
                Event::Start(Tag::Image { link_type, dest_url: path.into(), title, id })
            }
            DocumentEvent::Markdown(Event::Start(Tag::Link { link_type, dest_url, title, id }), _) => {
                let url = link_target(base, &dest_url).unwrap_or_else(|| "#".into());
                Event::Start(Tag::Link { link_type, dest_url: url.into(), title, id })
            }
            DocumentEvent::Markdown(Event::Html(fragment) | Event::InlineHtml(fragment), _) => {
                Event::Html(raw_html::render(&fragment, &mut assets, base).into())
            }
            DocumentEvent::Markdown(event, _) => event,
        })
    });
    html::write_html_io(&mut output, events)?;
    output.flush()?;
    cancel.check()?;
    Ok(assets.unavailable)
}

enum DocumentEvent<'a> {
    Markdown(Event<'a>, &'a str),
    Diagram(Diagram),
}

fn document_events(markdown: &str) -> impl Iterator<Item = DocumentEvent<'_>> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut parser = Parser::new_ext(markdown, options).into_offset_iter();
    let mut replay = Vec::new().into_iter();
    std::iter::from_fn(move || {
        if let Some(event) = replay.next() {
            return Some(event);
        }
        let (event, range) = parser.next()?;
        let source = &markdown[range];
        if matches!(event, Event::Start(Tag::HtmlBlock)) {
            // Keep multiline attributes together when tokenizing raw HTML.
            let mut raw = String::new();
            for (event, _) in parser.by_ref() {
                match event {
                    Event::Html(fragment) => raw.push_str(&fragment),
                    Event::End(TagEnd::HtmlBlock) => break,
                    _ => {}
                }
            }
            return Some(DocumentEvent::Markdown(Event::Html(raw.into()), source));
        }
        let diagram = match &event {
            Event::InlineMath(value) => Some(Diagram {
                kind: "inline-math".into(),
                source: value.to_string(),
            }),
            Event::DisplayMath(value) => Some(Diagram {
                kind: "math".into(),
                source: value.to_string(),
            }),
            Event::Start(Tag::Paragraph) => {
                let math = source
                    .trim()
                    .strip_prefix("\\[")
                    .and_then(|s| s.strip_suffix("\\]"))
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && !s.contains("\\[") && !s.contains("\\]"));
                if math.is_some() {
                    parser
                        .by_ref()
                        .find(|(event, _)| *event == Event::End(TagEnd::Paragraph));
                }
                math.map(|value| Diagram {
                    kind: "math".into(),
                    source: value.into(),
                })
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    CodeBlockKind::Indented => "",
                    CodeBlockKind::Fenced(language) => language,
                };
                // Fenced content uses the parser's container-relative indentation.
                let mut code = String::new();
                for (event, _) in parser.by_ref() {
                    match event {
                        Event::Text(text) => code.push_str(&text),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                let diagram = diagrams::fenced(language, code.trim_end_matches('\n'));
                if diagram.is_none() {
                    replay = vec![
                        DocumentEvent::Markdown(Event::Text(code.into()), ""),
                        DocumentEvent::Markdown(Event::End(TagEnd::CodeBlock), ""),
                    ]
                    .into_iter();
                }
                diagram
            }
            _ => None,
        };
        Some(diagram.map_or(
            DocumentEvent::Markdown(event, source),
            DocumentEvent::Diagram,
        ))
    })
}

struct Assets<'a> {
    directory: &'a Path,
    diagrams: HashMap<[u8; 32], std::result::Result<VectorImage, String>>,
    images: HashMap<Url, Option<StagedImage>>,
    next_asset: usize,
    unavailable: usize,
    embedded: bool,
    cancel: &'a Cancellation,
}

#[derive(Clone)]
struct StagedImage {
    path: String,
    media_type: &'static str,
}

struct VectorImage {
    path: String,
    height: f32,
    descent: f32,
}

impl Assets<'_> {
    fn prepare_diagrams(&mut self, markdown: &str) -> Result<()> {
        let mut batch = Vec::new();
        // The child admits bounded batches. Completed assets remain on disk for reuse.
        for event in document_events(markdown) {
            self.cancel.check()?;
            let DocumentEvent::Diagram(diagram) = event else {
                continue;
            };
            let key = diagram_key(&diagram);
            if self.diagrams.contains_key(&key) || batch.contains(&diagram) {
                continue;
            }
            if diagram.source.len() > diagrams::MAX_SOURCE {
                self.diagrams
                    .insert(key, Err(crate::locale::tr("Cannot display diagram")));
                continue;
            }
            batch.push(diagram);
            if batch.len() == diagrams::MAX_DIAGRAMS {
                self.write_diagrams(&batch)?;
                batch.clear();
            }
        }
        if !batch.is_empty() {
            self.write_diagrams(&batch)?;
        }
        self.cancel.check()
    }

    fn write_diagrams(&mut self, batch: &[Diagram]) -> Result<()> {
        let cancel = self.cancel;
        diagrams::render_svg_batch(
            batch,
            || cancel.check().is_err(),
            |diagram, result| {
                let result = result.and_then(|svg| self.vector(svg).map_err(|e| e.to_string()));
                self.diagrams.insert(diagram_key(diagram), result);
            },
        )
    }

    fn next_path(&mut self, extension: &str) -> String {
        let path = format!("asset-{}.{extension}", self.next_asset);
        self.next_asset += 1;
        path
    }

    fn asset_url(&self, path: &str, media_type: &str) -> Option<String> {
        if !self.embedded {
            return Some(path.to_owned());
        }
        let mut url = format!("data:{media_type};base64,").into_bytes();
        let mut encoder = EncoderWriter::new(&mut url, &STANDARD);
        self.cancel
            .copy(File::open(self.directory.join(path)).ok()?, &mut encoder)
            .ok()?;
        encoder.finish().ok()?;
        drop(encoder);
        String::from_utf8(url).ok()
    }

    fn vector(&mut self, svg: DiagramSvg) -> Result<VectorImage> {
        use resvg::usvg::{Options, Tree, WriteOptions};
        let options = Options {
            fontdb: diagrams::diagram_fonts(),
            ..Default::default()
        };
        let tree = Tree::from_str(&svg.svg, &options)?;
        let height = tree.size().height();
        let descent = height
            - svg
                .baseline
                .unwrap_or(f64::from(height))
                .clamp(0., f64::from(height)) as f32;
        let path = self.next_path("svg");
        fs::write(
            self.directory.join(&path),
            tree.to_string(&WriteOptions::default()),
        )?;
        Ok(VectorImage {
            path,
            height,
            descent,
        })
    }

    fn diagram_html(&mut self, diagram: &Diagram) -> String {
        let inline = diagram.kind == "inline-math";
        let result = self.diagrams.get(&diagram_key(diagram));
        let image = result
            .and_then(|value| value.as_ref().ok())
            .and_then(|image| {
                self.asset_url(&image.path, "image/svg+xml")
                    .map(|url| (image, url))
            });
        match image {
            Some((image, url)) if inline => format!(
                "<img class=\"formula\" src=\"{}\" alt=\"{}\" style=\"height:{}px;vertical-align:-{}px\">",
                url,
                escaped(&diagram.source),
                image.height,
                image.descent
            ),
            Some((_, url)) => format!(
                "<figure class=\"diagram\"><img src=\"{}\" alt=\"{}\"></figure>\n",
                url,
                escaped(&diagram.kind)
            ),
            None => {
                self.unavailable += 1;
                let reason = result
                    .and_then(|value| value.as_ref().err())
                    .map(String::as_str)
                    .unwrap_or_default();
                let label = escaped(&crate::locale::tr("Cannot display diagram"));
                if inline {
                    format!(
                        "<code title=\"{label}\">{}</code>",
                        escaped(&diagram.source)
                    )
                } else {
                    format!(
                        "<div class=\"asset-error\">{label} ({})</div><pre><code>{}</code></pre>\n",
                        escaped(if reason.is_empty() {
                            &diagram.kind
                        } else {
                            reason
                        }),
                        escaped(&diagram.source)
                    )
                }
            }
        }
    }

    fn image(&mut self, base: &Url, uri: &str) -> String {
        let url = base.join(uri).ok();
        let path = url.and_then(|url| {
            if let Some(path) = self.images.get(&url) {
                return path.clone();
            }
            let path = self.prepare_image(&url).ok();
            self.images.insert(url, path.clone());
            path
        });
        path.and_then(|image| self.asset_url(&image.path, image.media_type))
            .unwrap_or_else(|| {
                self.unavailable += 1;
                "image-unavailable".into()
            })
    }

    fn prepare_image(&mut self, url: &Url) -> Result<StagedImage> {
        let downloaded;
        let source = match url.scheme() {
            "file" => url.to_file_path().ok().context("Invalid image path")?,
            "http" | "https" => {
                downloaded = markup_images::fetch_image_file(url.as_str(), self.cancel)?;
                downloaded.path().to_owned()
            }
            _ => anyhow::bail!("Unsupported image URL"),
        };
        ensure!(source.metadata()?.is_file(), "Image is not a regular file");
        if crate::svg_preview::dimensions(&source, self.cancel)
            .ok()
            .flatten()
            .is_some()
        {
            // Isolated image documents preserve vectors without launching Quick Look.
            let svg = crate::svg_preview::prepare_browser_image(&source, self.cancel)?;
            let path = self.next_path("svg");
            self.cancel
                .copy(svg.reopen()?, File::create(self.directory.join(&path))?)?;
            return Ok(StagedImage {
                path,
                media_type: "image/svg+xml",
            });
        }
        // Raster exports use a bounded still image, including for animated inputs.
        let bytes = inline_images::thumbnail_png_cancellable(
            &source,
            limits::INLINE_IMAGE_PREVIEW_EDGE,
            self.cancel,
        )?;
        let path = self.next_path("png");
        fs::write(self.directory.join(&path), bytes)?;
        Ok(StagedImage {
            path,
            media_type: "image/png",
        })
    }
}

fn link_target(base: &Url, target: &str) -> Option<String> {
    if target.starts_with('#') {
        return Some(target.into());
    }
    let url = base.join(target).ok()?;
    matches!(url.scheme(), "https" | "http" | "mailto" | "file").then(|| url.into())
}

fn diagram_key(diagram: &Diagram) -> [u8; 32] {
    let mut key = Sha256::new();
    key.update(diagram.kind.as_bytes());
    key.update([0]);
    key.update(diagram.source.as_bytes());
    key.finalize().into()
}

fn escaped(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(ch),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_html_preserves_structure_and_assets() -> Result<()> {
        let directory = tempfile::tempdir()?;
        fs::write(
            directory.path().join("badge.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10"/></svg>"#,
        )?;
        let base = Url::from_file_path(directory.path().join("README.md"))
            .expect("absolute temporary path");
        let markdown = r##"<p align="center"><a
href="#intro" onclick="ignore()"><img src="badge.svg" width="100%" alt="Badge" /> English &amp; 简体中文</a></p>

<details>
<summary>Read more</summary>

## Intro

`<p>`

</details>

<script>ignore <a href="https://example.com">this</a></script>
"##;
        for embedded in [false, true] {
            let mut html = Vec::new();
            let missing = write_html(
                markdown,
                &base,
                "README",
                directory.path(),
                &mut html,
                embedded,
                &Cancellation::default(),
            )?;
            let html = String::from_utf8(html)?;
            assert_eq!(missing, 0);
            assert!(html.contains(r##"<p align="center"><a href="#intro"><img src=""##));
            assert!(html.contains(r#"width="100%" alt="Badge"> English &amp; 简体中文</a></p>"#));
            assert!(html.contains(if embedded {
                "<details>"
            } else {
                "<details open>"
            }));
            assert!(html.contains("<code>&lt;p&gt;</code>"));
            assert!(html.contains(if embedded {
                "data:image/svg+xml;base64,"
            } else {
                "asset-0.svg"
            }));
            assert!(!html.contains("onclick") && !html.contains("ignore"));
        }
        Ok(())
    }
}
