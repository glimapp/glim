//! SSH connection records and SFTP access. Call on background workers.
mod content_search;
mod operations;
mod search_index;
mod sftp;
pub use search_index::{IndexedPage, SearchIndexProgress, SearchIndexUpdate, search_retry_delay};

use crate::{storage, work::Cancellation};
use anyhow::{Context, Result, ensure};
use glim_core::DirectoryEntry;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: Option<u16>,
    pub identity: Option<PathBuf>,
    #[serde(default)]
    pub last_directory: Option<PathBuf>,
}
impl Connection {
    fn ssh_command(&self) -> Result<std::process::Command> {
        use std::{
            os::unix::process::CommandExt,
            process::{Command, Stdio},
        };
        let mut command = Command::new("/usr/bin/ssh");
        command
            .args([
                "-T",
                "-o",
                "ConnectTimeout=15",
                "-o",
                "NumberOfPasswordPrompts=1",
                "-o",
                "ServerAliveInterval=5",
                "-o",
                "ServerAliveCountMax=2",
                "-o",
                "StrictHostKeyChecking=ask",
                "-o",
                "ControlMaster=no",
                "-o",
                "ControlPath=none",
            ])
            .env("SSH_ASKPASS", std::env::current_exe()?)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("GLIM_SSH_ASKPASS", "1")
            .env("DISPLAY", "glim:0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(port) = self.port {
            command.arg("-p").arg(port.to_string());
        }
        if !self.user.is_empty() {
            command.arg("-l").arg(&self.user);
        }
        if let Some(identity) = &self.identity {
            command
                .arg("-i")
                .arg(identity)
                .args(["-o", "IdentitiesOnly=yes"]);
        }
        command.process_group(0);
        Ok(command)
    }
    pub fn label(&self) -> String {
        if self.name.trim().is_empty() {
            self.address()
        } else {
            self.name.clone()
        }
    }
    pub fn address(&self) -> String {
        let host = if self.user.is_empty() {
            self.host.clone()
        } else {
            format!("{}@{}", self.user, self.host)
        };
        self.port
            .map_or(host.clone(), |port| format!("{host}:{port}"))
    }
    pub fn validate(&mut self) -> Result<()> {
        self.host = self.host.trim().to_owned();
        self.user = self.user.trim().to_owned();
        if let Some((user, host)) = self.host.rsplit_once('@') {
            if self.user.is_empty() {
                self.user = user.to_owned();
            }
            self.host = host.to_owned();
        }
        ensure!(
            !self.host.is_empty()
                && !self.host.starts_with('-')
                && !self.host.chars().any(char::is_whitespace),
            "Enter a valid SSH host"
        );
        ensure!(
            !self.user.starts_with('-') && !self.user.chars().any(char::is_whitespace),
            "Enter a valid SSH username"
        );
        ensure!(self.port != Some(0), "SSH port must be between 1 and 65535");
        if let Some(identity) = &self.identity {
            ensure!(identity.is_file(), "The private key file does not exist");
        }
        if self.id.is_empty() {
            self.id = format!(
                "ssh-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteWorkspace {
    pub connection: String,
    pub roots: Vec<PathBuf>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub connections: Vec<Connection>,
    pub workspaces: BTreeMap<String, RemoteWorkspace>,
}
pub fn load_catalog() -> Result<Catalog> {
    storage::record(&storage::open()?, "remote", "catalog")?
        .map(|text| serde_json::from_str(&text).context("Cannot read SSH connections"))
        .transpose()
        .map(Option::unwrap_or_default)
}
pub fn save_catalog(catalog: &Catalog) -> Result<()> {
    storage::set_record(
        &storage::open()?,
        "remote",
        "catalog",
        &serde_json::to_string(catalog)?,
    )
}
pub fn workspace_id(connection: &str, roots: &[PathBuf]) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(connection.as_bytes());
    for root in roots {
        hash.update([0]);
        hash.update(root.as_os_str().as_encoded_bytes());
    }
    format!("ssh-workspace-{:x}", hash.finalize())
}
pub fn is_remote_id(id: &str) -> bool {
    id.starts_with("ssh-workspace-")
}

#[derive(Clone)]
pub struct Session {
    pub connection: Connection,
    pub home: PathBuf,
    transport: Arc<Mutex<sftp::Sftp>>,
    search_transport: Arc<Mutex<Option<sftp::Sftp>>>,
    search_connect_after: Arc<AtomicU64>,
    pub received: Arc<AtomicU64>,
    connected: Arc<std::sync::atomic::AtomicBool>,
    cache: Arc<tempfile::TempDir>,
    versions: Arc<Mutex<BTreeMap<PathBuf, RemoteVersion>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub size: Option<u64>,
    pub modified: Option<u32>,
    pub mode: u32,
}

pub type RemoteVersion = BTreeMap<PathBuf, Option<FileStamp>>;
impl Session {
    pub fn connect(connection: Connection, cancel: &Cancellation) -> Result<Self> {
        let transport = sftp::Sftp::connect(&connection, cancel)?;
        Self::from_transport(connection, transport, cancel)
    }
    pub fn connect_background(connection: Connection, cancel: &Cancellation) -> Result<Self> {
        let transport = sftp::Sftp::connect_background(&connection, cancel)?;
        Self::from_transport(connection, transport, cancel)
    }
    fn from_transport(
        connection: Connection,
        mut transport: sftp::Sftp,
        cancel: &Cancellation,
    ) -> Result<Self> {
        let home = transport.realpath(Path::new("."), cancel)?;
        let cache = tempfile::Builder::new().prefix("glim-remote-").tempdir()?;
        Ok(Self {
            connection,
            home,
            transport: Arc::new(Mutex::new(transport)),
            search_transport: Default::default(),
            search_connect_after: Default::default(),
            received: Arc::new(AtomicU64::new(0)),
            cache: Arc::new(cache),
            versions: Default::default(),
            connected: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        })
    }
    fn with_transport<T>(
        &self,
        cancel: &Cancellation,
        operation: impl FnOnce(&mut sftp::Sftp) -> Result<T>,
    ) -> Result<T> {
        loop {
            cancel.check()?;
            match self.transport.try_lock() {
                Ok(mut transport) => {
                    if !transport.healthy() {
                        self.connected.store(false, Ordering::Relaxed);
                        *transport = sftp::Sftp::connect(&self.connection, cancel)?;
                    }
                    let result = operation(&mut transport);
                    self.connected.store(transport.healthy(), Ordering::Relaxed);
                    return result;
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    anyhow::bail!("SSH connection closed; reconnect to continue")
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
            }
        }
    }
    pub fn is_connected(&self) -> bool {
        match self.transport.try_lock() {
            Ok(transport) => transport.healthy(),
            Err(std::sync::TryLockError::WouldBlock) => self.connected.load(Ordering::Relaxed),
            Err(std::sync::TryLockError::Poisoned(_)) => false,
        }
    }
    pub fn shares_transport(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.transport, &other.transport)
    }
    /// Probe idle sessions and recover dead transports without authentication dialogs.
    pub fn maintain_connection(&self, cancel: &Cancellation) -> Result<()> {
        cancel.check()?;
        let mut transport = match self.transport.try_lock() {
            Ok(transport) => transport,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(()),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                anyhow::bail!("SSH connection closed; reconnect to continue")
            }
        };
        if !transport.healthy() {
            self.connected.store(false, Ordering::Relaxed);
            *transport = sftp::Sftp::connect_background(&self.connection, cancel)?;
        }
        let result = transport.realpath(Path::new("."), cancel).map(|_| ());
        self.connected.store(transport.healthy(), Ordering::Relaxed);
        result
    }
    pub fn is_idle(&self) -> bool {
        self.transport
            .try_lock()
            .is_ok_and(|transport| transport.healthy())
    }
    pub fn loaded_version(&self, path: &Path) -> Option<RemoteVersion> {
        self.versions.lock().ok()?.get(path).cloned()
    }
    pub fn has_changed(&self, version: &RemoteVersion, cancel: &Cancellation) -> Result<bool> {
        self.with_transport(cancel, |transport| {
            for (path, previous) in version {
                cancel.check()?;
                if transport.stat_optional(path, cancel)? != *previous {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }
    pub fn list(
        &self,
        path: &Path,
        cancel: &Cancellation,
    ) -> Result<(PathBuf, Vec<DirectoryEntry>)> {
        self.with_transport(cancel, |transport| {
            let path = transport.realpath(path, cancel)?;
            let entries = transport.list(&path, cancel)?;
            Ok((path, entries))
        })
    }
    pub fn list_entries(&self, path: &Path, cancel: &Cancellation) -> Result<Vec<DirectoryEntry>> {
        self.with_transport(cancel, |transport| transport.list(path, cancel))
    }
    pub fn cache_path(&self, path: &Path) -> Result<PathBuf> {
        ensure!(path.is_absolute(), "Remote path must be absolute");
        let mut cache = self.cache.path().to_path_buf();
        for component in path.components() {
            match component {
                Component::RootDir => {}
                Component::Normal(name) => cache.push(name),
                _ => anyhow::bail!("Invalid remote path"),
            }
        }
        Ok(cache)
    }
    pub fn remote_path(&self, cached: &Path) -> Option<PathBuf> {
        let relative = cached.strip_prefix(self.cache.path()).ok()?;
        let mut path = PathBuf::from("/");
        for component in relative.components() {
            match component {
                Component::Normal(name) => path.push(name),
                Component::ParentDir => {
                    path.pop();
                }
                _ => {}
            }
        }
        Some(path)
    }
    pub fn download(&self, path: &Path, cancel: &Cancellation) -> Result<PathBuf> {
        let target = self.cache_path(path)?;
        std::fs::create_dir_all(target.parent().context("Invalid remote file path")?)?;
        self.received.store(0, Ordering::Relaxed);
        self.with_transport(cancel, |transport| {
            transport.download(path, &target, &self.received, cancel)
        })?;
        Ok(target)
    }
    pub fn read_content(
        &self,
        path: &Path,
        cancel: &Cancellation,
    ) -> Result<crate::documents::Content> {
        let stamp = self.with_transport(cancel, |transport| transport.stat(path, cancel))?;
        let mut version = RemoteVersion::from([(path.to_path_buf(), Some(stamp))]);
        let target = self.download(path, cancel)?;
        let content = crate::documents::read_content_cancellable(&target, cancel)?;
        if matches!(content, crate::documents::Content::Sqlite(_)) {
            for suffix in ["-wal", "-journal"] {
                let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
                if let Ok(cached) = self.cache_path(&sidecar) {
                    let _ = std::fs::remove_file(cached);
                }
                self.read_dependency(&sidecar, &mut version, cancel);
                cancel.check()?;
            }
        }
        if let crate::documents::Content::File(document) = &content {
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let html =
                extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm");
            if html || document.kind == glim_core::DocumentKind::Markdown {
                let base = url::Url::from_file_path(path)
                    .map_err(|_| anyhow::anyhow!("Invalid remote document path"))?;
                for uri in crate::markup_images::image_uris(&document.text, html) {
                    cancel.check()?;
                    let Ok(url) = base.join(&uri) else {
                        continue;
                    };
                    if url.scheme() != "file" || url.host_str().is_some() {
                        continue;
                    }
                    if let Ok(resource) = url.to_file_path()
                        && resource != path
                    {
                        self.read_dependency(&resource, &mut version, cancel);
                    }
                }
            }
        }
        cancel.check()?;
        self.versions
            .lock()
            .map_err(|_| anyhow::anyhow!("Remote version cache unavailable"))?
            .insert(path.to_path_buf(), version);
        Ok(content)
    }

    fn read_dependency(&self, path: &Path, version: &mut RemoteVersion, cancel: &Cancellation) {
        match self.with_transport(cancel, |transport| transport.stat_optional(path, cancel)) {
            Ok(Some(stamp)) if self.download(path, cancel).is_ok() => {
                version.insert(path.to_path_buf(), Some(stamp));
            }
            Ok(None) => {
                if let Ok(cached) = self.cache_path(path) {
                    let _ = std::fs::remove_file(cached);
                }
                version.insert(path.to_path_buf(), None);
            }
            _ => {}
        }
    }
}
