//! Local stdio MCP transport. Only opens an existing path in the signed Glim application.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Read, Write},
    path::Path,
    process::{Command, Stdio},
};

// Bound protocol message memory, independently of the size of a file being opened.
const MESSAGE_BYTES: u64 = 64 * 1024;

pub fn run() -> Result<()> {
    let app = super::bundle()?;
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    let mut initialized = false;
    loop {
        let mut line = Vec::new();
        if input
            .by_ref()
            .take(MESSAGE_BYTES + 1)
            .read_until(b'\n', &mut line)?
            == 0
        {
            return Ok(());
        }
        if line.len() as u64 > MESSAGE_BYTES {
            // The request ID cannot be trusted until the whole message has been parsed.
            write_response(
                &mut output,
                error(Value::Null, -32700, "Message is too large"),
            )?;
            return Ok(());
        }
        let request: Value = match serde_json::from_slice(&line) {
            Ok(request) => request,
            Err(_) => {
                write_response(&mut output, error(Value::Null, -32700, "Invalid JSON"))?;
                continue;
            }
        };
        let Some(method) = request.get("method").and_then(Value::as_str) else {
            write_response(
                &mut output,
                error(
                    request.get("id").cloned().unwrap_or(Value::Null),
                    -32600,
                    "Invalid request",
                ),
            )?;
            continue;
        };
        if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            write_response(
                &mut output,
                error(Value::Null, -32600, "Invalid JSON-RPC version"),
            )?;
            continue;
        }
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        if !(id.is_string() || id.is_number()) {
            write_response(
                &mut output,
                error(Value::Null, -32600, "Invalid request ID"),
            )?;
            continue;
        }
        let params = &request["params"];
        let response = match method {
            "initialize" => {
                initialized = true;
                let requested = params["protocolVersion"].as_str().unwrap_or_default();
                let version = match requested {
                    "2024-11-05" | "2025-03-26" | "2025-06-18" => requested,
                    _ => "2025-06-18",
                };
                success(
                    id,
                    json!({
                        "protocolVersion":version,
                        "capabilities":{"tools":{}},
                        "serverInfo":{"name":"glim", "version":env!("CARGO_PKG_VERSION")},
                        "instructions":"Use open_in_glim only when the user asks to view a local file or folder in Glim. This tool opens the viewer; it does not read or upload file contents."
                    }),
                )
            }
            "ping" => success(id, json!({})),
            _ if !initialized => error(id, -32002, "Initialize the server first"),
            "tools/list" => success(
                id,
                json!({"tools":[{
                    "name":"open_in_glim",
                    "description":"Open an existing local file or folder in Glim on this Mac, when requested by the user. A folder opens as a workspace. Requires an absolute filesystem path; remote paths and URLs are not supported. Does not return file contents.",
                    "inputSchema":{"type":"object", "properties":{"path":{"type":"string", "description":"Absolute local path to a file or directory"}}, "required":["path"], "additionalProperties":false},
                    "annotations":{"title":"Open in Glim", "readOnlyHint":true, "destructiveHint":false, "openWorldHint":false}
                }]}),
            ),
            "tools/call" => {
                if params["name"].as_str() != Some("open_in_glim") {
                    error(id, -32602, "Unknown tool")
                } else if let Some(path) = params["arguments"]["path"].as_str() {
                    match open(&app, Path::new(path)) {
                        Ok(()) => success(
                            id,
                            json!({"content":[{"type":"text", "text":"Sent the path to Glim."}]}),
                        ),
                        Err(error) => success(
                            id,
                            json!({"isError":true, "content":[{"type":"text", "text":error.to_string()}]}),
                        ),
                    }
                } else {
                    error(id, -32602, "An absolute local path is required")
                }
            }
            _ => error(id, -32601, "Method not found"),
        };
        write_response(&mut output, response)?;
    }
}

fn open(app: &Path, path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "Use an absolute local path");
    let path = path
        .canonicalize()
        .context("The path does not exist or cannot be accessed")?;
    let metadata = path.metadata().context("Cannot inspect this path")?;
    ensure!(
        metadata.is_file() || metadata.is_dir(),
        "Only regular files and directories can be opened"
    );
    let status = Command::new("/usr/bin/open")
        .arg("-a")
        .arg(app)
        .arg("--")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Could not launch Glim")?;
    ensure!(status.success(), "macOS could not open Glim");
    Ok(())
}

fn success(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "result":result})
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
}

fn write_response(output: &mut impl Write, response: Value) -> Result<()> {
    serde_json::to_writer(&mut *output, &response)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}
