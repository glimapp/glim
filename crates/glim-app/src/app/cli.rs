//! Terminal commands shipped in the app bundle.
use anyhow::{Context, Result, bail, ensure};
use glim_services::{files, integrations, markdown_export, work::Cancellation};
use std::{ffi::OsString, path::PathBuf, process::Command};

const HELP: &str = "Usage: glim COMMAND [ARGS]

  open PATH                  Open a file or folder in Glim
  html MARKDOWN OUTPUT       Write standalone HTML
  pdf MARKDOWN OUTPUT        Write a PDF
  preview MARKDOWN           Open rendered HTML in the default browser
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
        (Some("mcp"), []) => return integrations::mcp::run(),
        (Some("open"), [input]) => {
            let path = PathBuf::from(input)
                .canonicalize()
                .context("Cannot resolve input path")?;
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
        }
        (Some("html" | "pdf"), [input, output]) => {
            let output = std::env::current_dir()?.join(output);
            render(input, Some((&output, name == "pdf")))?;
        }
        (Some("preview"), [input]) => render(input, None)?,
        _ => bail!("Invalid command or arguments.\n{HELP}"),
    }
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
