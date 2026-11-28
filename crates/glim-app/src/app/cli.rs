//! Terminal commands shipped in the app bundle.
use anyhow::{Context, Result, bail, ensure};
use glim_services::{
    archive, binary, csv, files, integrations, markdown_export, sqlite, work::Cancellation,
};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Command,
};

const HELP: &str = "Usage: glim PATH
       glim -- PATH
       glim COMMAND [ARGS]

  PATH                       Open a file or folder in Glim
  html MARKDOWN OUTPUT       Write standalone HTML
  pdf MARKDOWN OUTPUT        Write a PDF
  preview MARKDOWN           Open rendered HTML in the default browser
  hex FILE                   Show a bounded hex and ASCII preview
  archive FILE               List archive entries as JSON
  extract FILE DIRECTORY     Extract into a new folder under DIRECTORY
  tables DATABASE            List SQLite tables as JSON
  columns DATABASE TABLE     List SQLite column names as JSON
  rows CSV_OR_TSV [ROW [COL]]  Read a JSON page at zero-based offsets
  formats                    Print the Glim format catalog as JSON
  mcp                        Serve MCP over stdin/stdout
  --version                  Print build information
  --help                     Show this help

Exports print the output path. Choose a new output filename.";

pub fn run() -> Result<()> {
    let args: Vec<OsString> = std::env::args_os().skip(2).collect();
    let Some((name, paths)) = args.split_first() else {
        println!("{HELP}");
        return Ok(());
    };
    match (name.to_str(), paths) {
        (Some("--help" | "help" | "-h"), []) => println!("{HELP}"),
        (Some("--version"), []) => super::print_build_info(),
        (Some("formats"), []) => {
            print!("{}", include_str!("../../../../assets/format-catalog.json"))
        }
        (Some("hex"), [input]) => print!("{}", binary::read_preview(Path::new(input))?.text),
        (Some("archive"), [input]) => {
            let listing = archive::read(Path::new(input), &Cancellation::default())?;
            serde_json::to_writer(std::io::stdout().lock(), listing.entries())?;
            println!();
        }
        (Some("extract"), [input, parent]) => {
            let folder = archive::extract(
                Path::new(input),
                Path::new(parent),
                &Cancellation::default(),
            )?;
            println!("{}", folder.display());
        }
        (Some("tables"), [input]) => {
            let db = sqlite::browser::open(Path::new(input), &Cancellation::default())?;
            println!("{}", serde_json::to_string(&db.tables)?);
        }
        (Some("columns"), [input, table]) => {
            let cancel = Cancellation::default();
            let db = sqlite::browser::open(Path::new(input), &cancel)?;
            let table = table.to_str().context("Table name must be UTF-8")?;
            ensure!(
                db.tables.iter().any(|name| name == table),
                "Unknown table: {table}"
            );
            println!(
                "{}",
                serde_json::to_string(&db.columns(table, &cancel)?.names)?
            );
        }
        (Some("rows"), [input, offsets @ ..]) if offsets.len() <= 2 => {
            let path = Path::new(input);
            use glim_services::documents::presentation::TableFormat;
            let delimiter = TableFormat::for_path(path)
                .and_then(TableFormat::delimiter)
                .context("Expected a .csv or .tsv file")?;
            let offset = |i: usize| -> Result<usize> {
                match offsets.get(i) {
                    Some(value) => Ok(value
                        .to_str()
                        .context("Invalid offset")?
                        .parse()
                        .context("Expected a non-negative offset")?),
                    None => Ok(0),
                }
            };
            let row = offset(0)?;
            let col = offset(1)?;
            let page = csv::read_page(
                std::fs::File::open(path)?,
                delimiter,
                row,
                col,
                &Cancellation::default(),
            )?;
            let next_row = page.more.then(|| row + page.rows.len());
            println!(
                "{}",
                serde_json::json!({"rows": &*page.rows, "next_row": next_row, "columns": page.columns})
            );
        }
        (Some("mcp"), []) => return integrations::mcp::run(),
        (Some("html" | "pdf"), [input, output]) => {
            let output = std::env::current_dir()?.join(output);
            render(input, Some((&output, name == "pdf")))?;
        }
        (Some("preview"), [input]) => render(input, None)?,
        (Some("--"), [input]) => open_path(input)?,
        (_, []) if !name.as_encoded_bytes().starts_with(b"-") => open_path(name)?,
        _ => bail!("Invalid command or arguments.\n{HELP}"),
    }
    Ok(())
}

fn open_path(input: &OsStr) -> Result<()> {
    let input = Path::new(input);
    let path = input
        .canonicalize()
        .with_context(|| format!("Cannot resolve path {}", input.display()))?;
    ensure!(
        Command::new("/usr/bin/open")
            .arg("-a")
            .arg(integrations::bundle()?)
            .arg("--")
            .arg(path)
            .status()?
            .success(),
        "macOS could not open the path in Glim"
    );
    Ok(())
}

fn render(input: &OsString, output: Option<(&std::path::Path, bool)>) -> Result<()> {
    let path = PathBuf::from(input)
        .canonicalize()
        .context("Cannot resolve Markdown input")?;
    let markdown = std::fs::read_to_string(&path)
        .with_context(|| format!("Cannot read UTF-8 Markdown from {}", path.display()))?;
    let base =
        url::Url::from_file_path(&path).map_err(|_| anyhow::anyhow!("Invalid input path"))?;
    let title = path.file_stem().unwrap_or_default().to_string_lossy();
    let cancel = Cancellation::default();
    let (result, unavailable) = if let Some((output, pdf)) = output {
        let destination = files::prepare_new_destination(output, &cancel)?;
        let count = if pdf {
            markdown_export::export_pdf(&markdown, &base, &title, &destination, &cancel)?
        } else {
            markdown_export::export_html_to(&markdown, &base, &title, &destination, &cancel)?
        };
        (output.to_owned(), count)
    } else {
        let (html, count) = markdown_export::export_html(&markdown, &base, &title, &cancel)?;
        let browser = Command::new("/usr/bin/osascript")
            .args(["-l", "JavaScript", "-e", "ObjC.import('AppKit'); $.NSWorkspace.sharedWorkspace.URLForApplicationToOpenURL($.NSURL.URLWithString('https://localhost')).path.js"])
            .output()?;
        ensure!(browser.status.success(), "Cannot locate default browser");
        ensure!(
            Command::new("/usr/bin/open")
                .arg("-a")
                .arg(std::str::from_utf8(&browser.stdout)?.trim_end())
                .arg("--")
                .arg(&html)
                .status()?
                .success(),
            "Could not open browser preview"
        );
        (html, count)
    };
    println!("{}", result.display());
    if unavailable > 0 {
        eprintln!("{unavailable} embedded assets could not be rendered");
    }
    Ok(())
}
