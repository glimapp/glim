//! Codex's JSON-stdin file handler. SSH metadata is data, never a shell command.
use crate::remote::Connection;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{io::Read, path::PathBuf, process::Command};

// Bound handoff metadata independently of document size.
const REQUEST_BYTES: u64 = 64 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    path: PathBuf,
    host_config: Option<Host>,
    remote_workspace_root: Option<PathBuf>,
    remote_path: Option<PathBuf>,
}

#[derive(Deserialize)]
struct Host {
    kind: String,
    #[serde(default)]
    display_name: String,
    ssh_websocket_v0: Option<Ssh>,
    #[serde(default)]
    terminal_command: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ssh {
    ssh_alias: Option<String>,
    ssh_host: String,
    ssh_port: Option<u16>,
    identity: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
pub struct RemoteOpen {
    pub connection: Connection,
    pub root: PathBuf,
    pub file: Option<PathBuf>,
}

impl RemoteOpen {
    pub fn validate(&mut self) -> Result<()> {
        self.connection.id.clear();
        self.connection.last_directory = None;
        if let Some(key) = &self.connection.identity
            && let Ok(relative) = key.strip_prefix("~")
        {
            self.connection.identity = Some(super::home()?.join(relative));
        }
        self.connection.validate()?;
        ensure!(
            !self.connection.host.chars().any(char::is_control)
                && !self.connection.user.chars().any(char::is_control),
            "Invalid SSH address"
        );
        for path in std::iter::once(&self.root).chain(self.file.iter()) {
            ensure!(path.is_absolute(), "Remote paths must be absolute");
            ensure!(
                !path.as_os_str().as_encoded_bytes().contains(&0),
                "Invalid remote path"
            );
        }
        if self.file.as_ref() == Some(&self.root) {
            self.file = None;
        }
        Ok(())
    }

    pub fn from_url(url: &url::Url) -> Result<Self> {
        let data = url
            .query_pairs()
            .find_map(|(key, value)| (key == "request").then_some(value))
            .context("Missing remote open request")?;
        ensure!(
            data.len() as u64 <= REQUEST_BYTES,
            "Remote open request is too large"
        );
        let mut request: Self = serde_json::from_str(&data)?;
        request.validate()?;
        Ok(request)
    }
}

fn connection(host: Host) -> Result<Connection> {
    ensure!(host.kind == "ssh", "This remote connection is not SSH");
    let mut connection = Connection {
        name: host.display_name,
        ..Default::default()
    };
    if let Some(ssh) = host.ssh_websocket_v0 {
        connection.host = ssh
            .ssh_alias
            .filter(|alias| !alias.trim().is_empty())
            .unwrap_or(ssh.ssh_host);
        connection.port = ssh.ssh_port;
        connection.identity = ssh.identity;
        return Ok(connection);
    }
    let mut args = host.terminal_command.iter();
    ensure!(
        args.next()
            .is_some_and(|command| matches!(command.as_str(), "ssh" | "/usr/bin/ssh")),
        "Missing SSH connection metadata"
    );
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" => connection.port = Some(args.next().context("Missing SSH port")?.parse()?),
            "-i" => connection.identity = Some(args.next().context("Missing SSH identity")?.into()),
            "-l" => connection.user = args.next().context("Missing SSH user")?.clone(),
            "-T" | "-t" => {}
            value if !value.starts_with('-') && connection.host.is_empty() => {
                connection.host = value.into()
            }
            _ => bail!("Unsupported SSH arguments; use a Codex SSH connection or SSH config alias"),
        }
    }
    Ok(connection)
}

pub fn run() -> Result<()> {
    let mut data = Vec::new();
    std::io::stdin()
        .lock()
        .take(REQUEST_BYTES + 1)
        .read_to_end(&mut data)?;
    ensure!(
        data.len() as u64 <= REQUEST_BYTES,
        "Codex open request is too large"
    );
    let request: Request = serde_json::from_slice(&data).context("Invalid Codex open request")?;
    let target = if let Some(host) = request.host_config.filter(|host| host.kind != "local") {
        let file = request.remote_path;
        let root = request
            .remote_workspace_root
            .or_else(|| {
                file.as_ref()
                    .and_then(|path| path.parent().map(PathBuf::from))
            })
            .context("Missing remote workspace path")?;
        let mut remote = RemoteOpen {
            connection: connection(host)?,
            root,
            file,
        };
        remote.validate()?;
        let mut url = url::Url::parse("glim://open-remote")?;
        url.query_pairs_mut()
            .append_pair("request", &serde_json::to_string(&remote)?);
        url.to_string()
    } else {
        ensure!(
            request.remote_path.is_none() && request.remote_workspace_root.is_none(),
            "Missing SSH host metadata"
        );
        ensure!(
            request.path.is_absolute() && request.path.exists(),
            "Local path does not exist"
        );
        request
            .path
            .to_str()
            .context("Local path is not UTF-8")?
            .to_owned()
    };
    ensure!(
        Command::new("/usr/bin/open")
            .arg("-a")
            .arg(super::bundle()?)
            .arg("--")
            .arg(target)
            .status()?
            .success(),
        "macOS could not open Glim"
    );
    Ok(())
}
