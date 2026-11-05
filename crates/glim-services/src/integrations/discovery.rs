//! Discover configuration instances from known app locations, never execute user launchers.
use super::{Kind, SavedTarget, Target, home, inspect};
use anyhow::Result;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn candidates(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Codex => &["config.toml"],
        Kind::ClaudeDesktop => &["claude_desktop_config.json"],
        Kind::ClaudeCode => &[".claude.json"],
        Kind::Cursor => &["mcp.json", ".cursor/mcp.json"],
        Kind::Gemini => &["settings.json", ".gemini/settings.json"],
        Kind::Copilot => &["mcp-config.json"],
        Kind::VsCode => &["mcp.json", "User/mcp.json"],
        Kind::OpenCode => &["opencode.jsonc", "opencode.json"],
    }
}

pub(super) fn configuration_in(kind: Kind, directory: &Path) -> Option<PathBuf> {
    candidates(kind)
        .iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
        .or_else(|| {
            (kind == Kind::ClaudeCode && directory == home().ok()?.join(".claude"))
                .then(|| directory.parent().unwrap().join(".claude.json"))
                .filter(|path| path.is_file())
        })
}

struct Discovery {
    home: PathBuf,
    targets: BTreeMap<(Kind, PathBuf), Target>,
    visited: BTreeSet<(Kind, PathBuf)>,
}

impl Discovery {
    fn add(&mut self, kind: Kind, config: PathBuf) {
        let config = fs::canonicalize(&config).unwrap_or(config);
        let key = (kind, config.clone());
        if self.targets.contains_key(&key) {
            return;
        }
        let parent = config.parent().unwrap_or(&config);
        let detail = parent
            .strip_prefix(&self.home)
            .map(|p| format!("~/{}", p.display()))
            .unwrap_or_else(|_| parent.display().to_string());
        self.targets.insert(key, inspect(kind, config, detail));
    }

    fn scan_profiles(&mut self, kind: Kind, root: &Path, depth: usize) {
        let Ok(root) = root.canonicalize() else {
            return;
        };
        if !root.is_dir() || !self.visited.insert((kind, root.clone())) {
            return;
        }
        if let Some(config) = configuration_in(kind, &root) {
            self.add(kind, config);
        }
        if depth == 0 {
            return;
        }
        let Ok(children) = fs::read_dir(root) else {
            return;
        };
        for child in children.flatten() {
            let name = child.file_name().to_string_lossy().to_lowercase();
            // Account/profile managers nest configuration homes, but caches and projects are not instances.
            if name.contains("cache")
                || matches!(
                    name.as_str(),
                    "sessions"
                        | "projects"
                        | "worktrees"
                        | "node_modules"
                        | "plugins"
                        | "skills"
                        | "extensions"
                        | "backups"
                        | "logs"
                        | "history"
                        | ".git"
                        | "browser"
                        | "globalstorage"
                        | "workspacestorage"
                        | "service worker"
                        | "local storage"
                        | "session storage"
                        | "indexeddb"
                        | "blob_storage"
                        | "crashpad"
                        | "sentry"
                )
            {
                continue;
            }
            if child.file_type().is_ok_and(|kind| kind.is_dir()) {
                self.scan_profiles(kind, &child.path(), depth - 1);
            }
        }
    }

    fn named_roots(&mut self, directory: &Path) {
        let Ok(children) = fs::read_dir(directory) else {
            return;
        };
        for child in children.flatten() {
            let name = child
                .file_name()
                .to_string_lossy()
                .trim_start_matches('.')
                .to_lowercase();
            let starts = |base: &str| {
                name == base
                    || name
                        .strip_prefix(base)
                        .is_some_and(|rest| rest.starts_with(['-', '_', ' ', '.']))
            };
            let kinds: &[Kind] = if starts("codex") || starts("chatgpt") {
                &[Kind::Codex]
            } else if starts("claude") {
                &[Kind::ClaudeCode, Kind::ClaudeDesktop]
            } else if starts("cursor") {
                &[Kind::Cursor]
            } else if starts("gemini") {
                &[Kind::Gemini]
            } else if starts("copilot") {
                &[Kind::Copilot]
            } else if starts("code") || starts("vscode") {
                &[Kind::VsCode]
            } else if starts("opencode") {
                &[Kind::OpenCode]
            } else {
                &[]
            };
            for &kind in kinds {
                self.scan_profiles(kind, &child.path(), 3);
            }
        }
    }

    fn environment(&mut self, get: impl Fn(&str) -> Option<String>) {
        for (name, kind, file) in [
            ("CODEX_HOME", Kind::Codex, "config.toml"),
            ("CLAUDE_CONFIG_DIR", Kind::ClaudeCode, ".claude.json"),
            ("COPILOT_HOME", Kind::Copilot, "mcp-config.json"),
        ] {
            if let Some(root) = get(name).and_then(|p| self.absolute(&p))
                && root.is_dir()
            {
                self.add(kind, root.join(file));
            }
        }
        if let Some(path) = get("OPENCODE_CONFIG")
            .and_then(|p| self.absolute(&p))
            .filter(|p| p.is_file())
        {
            self.add(Kind::OpenCode, path);
        }
        if let Some(root) = get("OPENCODE_CONFIG_DIR").and_then(|p| self.absolute(&p)) {
            self.scan_profiles(Kind::OpenCode, &root, 2);
        }
        if let Some(root) = get("XDG_CONFIG_HOME").and_then(|p| self.absolute(&p)) {
            self.named_roots(&root);
        }
    }

    fn absolute(&self, value: &str) -> Option<PathBuf> {
        let path = if let Some(rest) = value.strip_prefix("~/") {
            self.home.join(rest)
        } else {
            PathBuf::from(value)
        };
        path.is_absolute().then_some(path)
    }

    fn applications(&mut self, directory: &Path, depth: usize) {
        let Ok(children) = fs::read_dir(directory) else {
            return;
        };
        for child in children.flatten() {
            let path = child.path();
            if path.extension().is_none_or(|extension| extension != "app") {
                if depth > 0 && child.file_type().is_ok_and(|kind| kind.is_dir()) {
                    self.applications(&path, depth - 1);
                }
                continue;
            }
            let Ok(output) = Command::new("/usr/bin/plutil")
                .args(["-convert", "json", "-o", "-", "--"])
                .arg(path.join("Contents/Info.plist"))
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
            else {
                continue;
            };
            if !output.status.success() {
                continue;
            }
            let Ok(info) = serde_json::from_slice::<Value>(&output.stdout) else {
                continue;
            };
            let id = info["CFBundleIdentifier"]
                .as_str()
                .unwrap_or_default()
                .to_lowercase();
            let kind = if id.starts_with("com.openai.codex") {
                Some(Kind::Codex)
            } else if id.starts_with("com.anthropic.claude") {
                Some(Kind::ClaudeDesktop)
            } else if id == "com.todesktop.230313mzl4w4u92" || id.starts_with("com.cursor.") {
                Some(Kind::Cursor)
            } else if id.starts_with("com.microsoft.vscode") {
                Some(Kind::VsCode)
            } else if id.starts_with("ai.opencode") {
                Some(Kind::OpenCode)
            } else {
                None
            };
            let Some(kind) = kind else {
                continue;
            };
            self.environment(|key| info["LSEnvironment"][key].as_str().map(str::to_owned));
            let default = match kind {
                Kind::Codex => ".codex/config.toml",
                Kind::ClaudeDesktop => {
                    "Library/Application Support/Claude/claude_desktop_config.json"
                }
                Kind::Cursor => ".cursor/mcp.json",
                Kind::VsCode if id.contains("insiders") => {
                    "Library/Application Support/Code - Insiders/User/mcp.json"
                }
                Kind::VsCode => "Library/Application Support/Code/User/mcp.json",
                Kind::OpenCode => ".config/opencode/opencode.json",
                _ => continue,
            };
            let config = self.home.join(default);
            let overridden = match kind {
                Kind::Codex => info["LSEnvironment"]["CODEX_HOME"].as_str().is_some(),
                Kind::OpenCode => info["LSEnvironment"]["OPENCODE_CONFIG"].as_str().is_some(),
                _ => false,
            };
            // A known app may not have created its configuration yet.
            if !overridden {
                let config = configuration_in(kind, config.parent().unwrap()).unwrap_or(config);
                self.add(kind, config);
            }
            if kind == Kind::VsCode {
                self.add(Kind::Copilot, self.home.join(".copilot/mcp-config.json"));
            }
            for name in [
                info["CFBundleName"].as_str(),
                info["CFBundleDisplayName"].as_str(),
                path.file_stem().and_then(|n| n.to_str()),
            ]
            .into_iter()
            .flatten()
            {
                if name.contains('/') || name == ".." {
                    continue;
                }
                self.scan_profiles(
                    kind,
                    &self.home.join("Library/Application Support").join(name),
                    3,
                );
            }
        }
    }
}

pub(super) fn discover(defaults: Vec<Target>) -> Result<Vec<Target>> {
    let home = home()?;
    let mut discovery = Discovery {
        home: home.clone(),
        targets: BTreeMap::new(),
        visited: BTreeSet::new(),
    };
    for mut target in defaults {
        target.config = fs::canonicalize(&target.config).unwrap_or(target.config);
        let parent = target.config.parent().unwrap_or(&target.config);
        target.detail = parent
            .strip_prefix(&home)
            .map(|p| format!("~/{}", p.display()))
            .unwrap_or_else(|_| parent.display().to_string());
        discovery
            .targets
            .insert((target.kind, target.config.clone()), target);
    }
    for directory in [
        &home,
        &home.join(".config"),
        &home.join("Library/Application Support"),
    ] {
        discovery.named_roots(directory);
    }
    discovery.environment(|key| std::env::var(key).ok());
    // launchd can hold overrides that a previously launched Glim process did not inherit.
    discovery.environment(|key| {
        Command::new("/bin/launchctl")
            .args(["getenv", key])
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    });
    discovery.applications(Path::new("/Applications"), 1);
    discovery.applications(&home.join("Applications"), 1);
    if let Some(saved) = crate::storage::setting(&crate::storage::open()?, "integration-targets")? {
        for target in serde_json::from_str::<Vec<SavedTarget>>(&saved)? {
            if target.config.is_file() {
                discovery.add(target.kind, target.config);
            }
        }
    }
    Ok(discovery.targets.into_values().collect())
}
