//! User-level app integrations. Discovery and configuration edits run off the UI thread.
mod discovery;
mod json_edit;
pub mod mcp;

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

static CONFIG_WRITE: Mutex<()> = Mutex::new(());

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Codex,
    ClaudeDesktop,
    ClaudeCode,
    Cursor,
    Gemini,
    Copilot,
    VsCode,
    OpenCode,
}

impl Kind {
    pub const ALL: [Self; 8] = [
        Self::Codex,
        Self::ClaudeDesktop,
        Self::ClaudeCode,
        Self::Cursor,
        Self::Gemini,
        Self::Copilot,
        Self::VsCode,
        Self::OpenCode,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::ClaudeDesktop => "Claude Desktop",
            Self::ClaudeCode => "Claude Code",
            Self::Cursor => "Cursor",
            Self::Gemini => "Gemini CLI",
            Self::Copilot => "GitHub Copilot",
            Self::VsCode => "GitHub Copilot · VS Code",
            Self::OpenCode => "OpenCode",
        }
    }

    fn keys(self) -> &'static [&'static str] {
        match self {
            Self::Codex => &["desktop", "custom_file_handlers", "glim"],
            Self::VsCode => &["servers", "glim"],
            Self::OpenCode => &["mcp", "glim"],
            _ => &["mcpServers", "glim"],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Available,
    Configured,
    Conflict,
    Unreadable,
}

#[derive(Clone)]
pub struct Target {
    pub kind: Kind,
    pub config: PathBuf,
    pub detail: String,
    pub status: Status,
    original: Option<Arc<str>>,
}

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("Home directory is unavailable")
}

pub fn bundle() -> Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let app = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
        .unwrap_or(Path::new("/Applications/Glim.app"));
    ensure!(
        app.join("Contents/MacOS/glim").is_file(),
        "Install Glim before adding integrations"
    );
    Ok(app.to_path_buf())
}

fn entry(kind: Kind) -> Result<Value> {
    let app = bundle()?;
    Ok(if kind == Kind::Codex {
        json!({
            "label":"Glim",
            "icon":crate::preferences::directory()?.join("Integrations/glim.png"),
            "command":"/usr/bin/open",
            "args":["-a",app,"--"],
            "input":"path"
        })
    } else if kind == Kind::OpenCode {
        json!({"type":"local", "command":[app.join("Contents/MacOS/glim"), "--mcp"], "enabled":true})
    } else if kind == Kind::Copilot {
        json!({"type":"stdio", "command":app.join("Contents/MacOS/glim"), "args":["--mcp"], "tools":["*"]})
    } else {
        json!({"type":"stdio", "command":app.join("Contents/MacOS/glim"), "args":["--mcp"]})
    })
}

fn value(source: &str, kind: Kind) -> Result<Option<Value>> {
    let document: Value = if kind == Kind::Codex {
        toml_edit::de::from_str(source)?
    } else {
        json5::from_str(source)?
    };
    let mut item = &document;
    for key in kind.keys() {
        ensure!(item.is_object(), "Invalid integration configuration");
        match item.get(key) {
            Some(next) => item = next,
            None => return Ok(None),
        }
    }
    Ok(Some(item.clone()))
}

fn managed(value: &Value, kind: Kind) -> Result<bool> {
    let expected = entry(kind)?;
    if kind == Kind::OpenCode {
        return Ok(value.get("command") == expected.get("command")
            && value.get("type").and_then(Value::as_str) == Some("local")
            && value.get("enabled").and_then(Value::as_bool) != Some(false));
    }
    // Adopt the equivalent file handler installed manually before this settings page existed.
    Ok(value.get("command") == expected.get("command")
        && value.get("args") == expected.get("args")
        && if kind == Kind::Codex {
            value.get("label").and_then(Value::as_str) == Some("Glim")
                && value.get("input").and_then(Value::as_str).unwrap_or("path") == "path"
                && value.get("supports_ssh").and_then(Value::as_bool) != Some(true)
        } else {
            value.get("type").and_then(Value::as_str).unwrap_or("stdio") == "stdio"
        })
}

fn inspect(kind: Kind, config: PathBuf, detail: String) -> Target {
    let (original, status) = match fs::read_to_string(&config) {
        Ok(source) => {
            let status = match value(&source, kind) {
                Ok(None) => Status::Available,
                Ok(Some(value)) if managed(&value, kind).unwrap_or(false) => Status::Configured,
                Ok(Some(_)) => Status::Conflict,
                Err(_) => Status::Unreadable,
            };
            (Some(Arc::from(source)), status)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (None, Status::Available),
        Err(_) => (None, Status::Unreadable),
    };
    Target {
        kind,
        config,
        detail,
        status,
        original,
    }
}

pub fn discover() -> Result<Vec<Target>> {
    discovery::discover(default_targets()?)
}

fn default_targets() -> Result<Vec<Target>> {
    let home = home()?;
    let mut roots = BTreeSet::new();
    let app_exists = |name: &str| {
        Path::new("/Applications").join(name).is_dir()
            || home.join("Applications").join(name).is_dir()
    };
    for child in fs::read_dir(&home)? {
        let child = child?;
        if child
            .file_name()
            .to_str()
            .is_some_and(|name| name == ".codex" || name.starts_with(".codex-"))
            && child.path().join("config.toml").is_file()
        {
            roots.insert(fs::canonicalize(child.path())?);
        }
    }
    if let Some(custom) = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
    {
        roots.insert(fs::canonicalize(custom)?);
    }
    if let Some(saved) =
        crate::storage::setting(&crate::storage::open()?, "integration-codex-roots")?
    {
        roots.extend(
            serde_json::from_str::<Vec<PathBuf>>(&saved)?
                .into_iter()
                .filter(|p| p.is_dir()),
        );
    }
    if roots.is_empty() && app_exists("Codex.app") {
        roots.insert(home.join(".codex"));
    }
    let mut targets = roots
        .into_iter()
        .map(|root| {
            let detail = root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            inspect(Kind::Codex, root.join("config.toml"), detail)
        })
        .collect::<Vec<_>>();
    for (kind, config, marker, app) in [
        (
            Kind::ClaudeDesktop,
            "Library/Application Support/Claude/claude_desktop_config.json",
            "Library/Application Support/Claude",
            "Claude.app",
        ),
        (Kind::ClaudeCode, ".claude.json", ".claude", ""),
        (Kind::Cursor, ".cursor/mcp.json", ".cursor", "Cursor.app"),
        (Kind::Gemini, ".gemini/settings.json", ".gemini", ""),
        (
            Kind::Copilot,
            ".copilot/mcp-config.json",
            ".copilot",
            "Visual Studio Code.app",
        ),
        (
            Kind::VsCode,
            "Library/Application Support/Code/User/mcp.json",
            "Library/Application Support/Code/User",
            "Visual Studio Code.app",
        ),
    ] {
        let config = if kind == Kind::Copilot {
            std::env::var_os("COPILOT_HOME")
                .map(PathBuf::from)
                .map(|p| p.join("mcp-config.json"))
                .unwrap_or_else(|| home.join(config))
        } else {
            home.join(config)
        };
        if config.exists() || home.join(marker).is_dir() || (!app.is_empty() && app_exists(app)) {
            targets.push(inspect(kind, config, String::new()));
        }
    }
    let opencode = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
        .join("opencode");
    let custom_opencode = std::env::var_os("OPENCODE_CONFIG").map(PathBuf::from);
    if opencode.is_dir() || app_exists("OpenCode.app") || custom_opencode.is_some() {
        let config = if opencode.join("opencode.jsonc").is_file() {
            "opencode.jsonc"
        } else {
            "opencode.json"
        };
        targets.push(inspect(
            Kind::OpenCode,
            custom_opencode.unwrap_or_else(|| opencode.join(config)),
            String::new(),
        ));
    }
    Ok(targets)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedTarget {
    kind: Kind,
    config: PathBuf,
}

pub fn remember_configuration(kind: Kind, selected: &Path) -> Result<()> {
    let config = if selected.is_file() {
        selected.to_path_buf()
    } else {
        discovery::configuration_in(kind, selected)
            .context("No supported configuration in this folder")?
    };
    let config = fs::canonicalize(config)?;
    let source = fs::read_to_string(&config)?;
    value(&source, kind)?;
    let mut db = crate::storage::open()?;
    let tx = db.transaction()?;
    let mut targets: Vec<SavedTarget> = crate::storage::setting(&tx, "integration-targets")?
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default();
    if !targets
        .iter()
        .any(|target| target.kind == kind && target.config == config)
    {
        targets.push(SavedTarget { kind, config });
    }
    crate::storage::set_setting(
        &tx,
        "integration-targets",
        &serde_json::to_string(&targets)?,
    )?;
    tx.commit()?;
    Ok(())
}

fn edit_codex(source: &str, install: bool) -> Result<String> {
    let mut document: toml_edit::DocumentMut = source.parse()?;
    if install {
        let entry = entry(Kind::Codex)?;
        let mut table = toml_edit::Table::new();
        for key in ["label", "icon", "command", "input"] {
            table[key] = toml_edit::value(entry[key].as_str().context("Invalid handler field")?);
        }
        let mut args = toml_edit::Array::new();
        for arg in entry["args"]
            .as_array()
            .context("Invalid handler arguments")?
        {
            args.push(arg.as_str().context("Invalid handler argument")?);
        }
        table["args"] = toml_edit::value(args);
        let desktop = &mut document["desktop"];
        if desktop.is_none() {
            *desktop = toml_edit::Item::Table(toml_edit::Table::new());
        }
        let inline = desktop.is_inline_table();
        let handlers = &mut desktop["custom_file_handlers"];
        if handlers.is_none() {
            *handlers = if inline {
                toml_edit::value(toml_edit::InlineTable::new())
            } else {
                toml_edit::Item::Table(toml_edit::Table::new())
            };
        }
        if handlers.is_inline_table() {
            handlers["glim"] = toml_edit::value(table.into_inline_table());
        } else {
            handlers["glim"] = toml_edit::Item::Table(table);
        }
    } else if let Some(handlers) = document
        .get_mut("desktop")
        .and_then(|v| v.get_mut("custom_file_handlers"))
        .and_then(toml_edit::Item::as_table_like_mut)
    {
        handlers.remove("glim");
    }
    Ok(document.to_string())
}

pub fn configure(target: &Target, install: bool) -> Result<()> {
    let _lock = CONFIG_WRITE
        .lock()
        .map_err(|_| anyhow::anyhow!("Configuration writer unavailable"))?;
    ensure!(
        matches!(target.status, Status::Available | Status::Configured),
        "Resolve the existing configuration before continuing"
    );
    let current = match fs::read_to_string(&target.config) {
        Ok(source) => Some(source),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    ensure!(
        current.as_deref() == target.original.as_deref(),
        "Configuration changed; refresh and retry"
    );
    let source = current.as_deref().unwrap_or(if target.kind == Kind::Codex {
        ""
    } else {
        "{}\n"
    });
    if let Some(existing) = value(source, target.kind)? {
        ensure!(
            managed(&existing, target.kind)?,
            "An unrelated integration already uses this name"
        );
        if install {
            return Ok(());
        }
    } else if !install {
        return Ok(());
    }
    let updated = if target.kind == Kind::Codex {
        edit_codex(source, install)?
    } else {
        json_edit::edit(
            source,
            target.kind.keys(),
            install.then(|| entry(target.kind)).transpose()?.as_ref(),
        )?
    };
    let actual = value(&updated, target.kind)?;
    ensure!(
        if install {
            actual.as_ref() == Some(&entry(target.kind)?)
        } else {
            actual.is_none()
        },
        "Integration update validation failed"
    );
    let directory = crate::preferences::directory()?.join("Integrations");
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    if install && target.kind == Kind::Codex {
        fs::write(
            directory.join("glim.png"),
            include_bytes!("../../../assets/branding/glim.png"),
        )?;
    }
    if let Some(original) = &current {
        let hash = format!(
            "{:x}",
            Sha256::digest(target.config.as_os_str().as_encoded_bytes())
        );
        let backup_dir = directory.join("Backups").join(hash);
        fs::create_dir_all(&backup_dir)?;
        fs::set_permissions(&backup_dir, fs::Permissions::from_mode(0o700))?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let mut backup = tempfile::NamedTempFile::new_in(&backup_dir)?;
        backup.write_all(original.as_bytes())?;
        backup.as_file().sync_all()?;
        backup.persist_noclobber(backup_dir.join(format!("{stamp}.bak")))?;
        let mut backups = fs::read_dir(&backup_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "bak"))
            .collect::<Vec<_>>();
        backups.sort();
        let remove = backups.len().saturating_sub(2);
        for backup in backups.into_iter().take(remove) {
            fs::remove_file(backup)?;
        }
    }
    let destination = if target.config.exists() {
        fs::canonicalize(&target.config)?
    } else {
        target.config.clone()
    };
    let parent = destination
        .parent()
        .context("Missing configuration directory")?;
    fs::create_dir_all(parent)?;
    let mut pending = tempfile::NamedTempFile::new_in(parent)?;
    if let Ok(metadata) = fs::metadata(&destination) {
        pending.as_file().set_permissions(metadata.permissions())?;
    }
    pending.write_all(updated.as_bytes())?;
    pending.as_file().sync_all()?;
    let latest = match fs::read_to_string(&target.config) {
        Ok(source) => Some(source),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    ensure!(
        latest == current,
        "Configuration changed; refresh and retry"
    );
    pending.persist(destination)?;
    Ok(())
}
