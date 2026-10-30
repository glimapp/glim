//! Application preferences and workspace state backed by the shared SQLite store.
use crate::storage::{self, set_setting, setting};
use anyhow::{Context, Result};
use std::{collections::BTreeMap, path::PathBuf};

pub(crate) fn directory() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("Home directory is unavailable")?)
            .join("Library/Application Support/Glim"),
    )
}
pub fn workspace_folders() -> Result<Vec<PathBuf>> {
    storage::paths(&storage::open()?, "workspace", "")
}
pub fn save_workspace_folders(folders: &[PathBuf]) -> Result<()> {
    let mut db = storage::open()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    storage::save_paths(&tx, "workspace", "", folders)?;
    tx.commit()?;
    Ok(())
}
pub type ExplorerExpansion = BTreeMap<PathBuf, Vec<PathBuf>>;
pub fn explorer_expansion(roots: &[PathBuf]) -> Result<ExplorerExpansion> {
    let db = storage::open()?;
    roots
        .iter()
        .map(|root| {
            Ok((
                root.clone(),
                storage::paths(&db, "explorer", storage::path_text(root)?)?,
            ))
        })
        .collect()
}
pub fn save_explorer_expansion(roots: &[PathBuf], expanded: &ExplorerExpansion) -> Result<()> {
    let mut db = storage::open()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    for root in roots {
        let paths = expanded
            .get(root)
            .into_iter()
            .flatten()
            .filter(|p| p.starts_with(root))
            .cloned()
            .collect::<Vec<_>>();
        storage::save_paths(&tx, "explorer", storage::path_text(root)?, &paths)?;
    }
    tx.commit()?;
    Ok(())
}
/// A workspace's ordered folder roots, including repeated paths.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RecentWorkspace {
    pub id: String,
    pub folders: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl RecentWorkspace {
    pub fn title(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            self.folders
                .iter()
                .map(|path| {
                    path.file_name()
                        .unwrap_or(path.as_os_str())
                        .to_string_lossy()
                        .into_owned()
                })
                .collect::<Vec<_>>()
                .join(" · ")
        })
    }
}

pub(crate) fn recent_at(db: &rusqlite::Connection) -> Result<Vec<RecentWorkspace>> {
    let values = db
        .prepare("SELECT id,name FROM recent_workspaces ORDER BY position")?
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    values
        .into_iter()
        .map(|(id, name)| {
            Ok(RecentWorkspace {
                folders: storage::paths(db, "recent", &id)?,
                id,
                name,
            })
        })
        .collect()
}
pub fn recent_workspaces() -> Result<Vec<RecentWorkspace>> {
    let mut db = storage::open()?;
    let tx = db.transaction()?;
    let recent = recent_at(&tx)?;
    tx.commit()?;
    Ok(recent)
}
pub(crate) fn save_recent_at(db: &rusqlite::Connection, recent: &[RecentWorkspace]) -> Result<()> {
    db.execute("DELETE FROM recent_workspaces", [])?;
    db.execute("DELETE FROM paths WHERE scope='recent'", [])?;
    for (position, entry) in recent.iter().enumerate() {
        db.execute(
            "INSERT INTO recent_workspaces VALUES(?1,?2,?3)",
            rusqlite::params![entry.id, entry.name, position as i64],
        )?;
        storage::save_paths(db, "recent", &entry.id, &entry.folders)?;
    }
    Ok(())
}
pub fn remember_recent_workspace(mut workspace: RecentWorkspace) -> Result<Vec<RecentWorkspace>> {
    let mut db = storage::open()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut recent = recent_at(&tx)?;
    if workspace.name.is_none() {
        workspace.name = recent
            .iter()
            .find(|e| e.id == workspace.id || e.folders == workspace.folders)
            .and_then(|e| e.name.clone());
    }
    recent.retain(|e| {
        e.id != workspace.id && (workspace.folders.is_empty() || e.folders != workspace.folders)
    });
    if !workspace.folders.is_empty() {
        recent.insert(0, workspace);
    }
    save_recent_at(&tx, &recent)?;
    tx.commit()?;
    Ok(recent)
}
pub fn remove_recent_workspace(id: &str) -> Result<Vec<RecentWorkspace>> {
    let mut db = storage::open()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut recent = recent_at(&tx)?;
    recent.retain(|e| e.id != id);
    save_recent_at(&tx, &recent)?;
    tx.commit()?;
    Ok(recent)
}
fn get(key: &str) -> Result<Option<String>> {
    setting(&storage::open()?, key)
}
fn set(key: &str, value: &str) -> Result<()> {
    set_setting(&storage::open()?, key, value)
}
pub fn word_wrap() -> bool {
    get("word-wrap")
        .ok()
        .flatten()
        .is_some_and(|v| v.trim() == "true")
}
pub fn save_word_wrap(enabled: bool) -> Result<()> {
    set("word-wrap", &enabled.to_string())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}
pub fn appearance() -> Appearance {
    match get("appearance").ok().flatten().as_deref().map(str::trim) {
        Some("light") => Appearance::Light,
        Some("dark") => Appearance::Dark,
        _ => Appearance::System,
    }
}
pub fn save_appearance(value: Appearance) -> Result<()> {
    set(
        "appearance",
        match value {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        },
    )
}
fn preview_key(kind: &str) -> Result<String> {
    anyhow::ensure!(
        matches!(kind, "html" | "markdown" | "csv"),
        "Unknown preview preference"
    );
    Ok(format!("{kind}-view"))
}
pub fn preview_mode(kind: &str) -> bool {
    preview_key(kind)
        .ok()
        .and_then(|key| get(&key).ok().flatten())
        .map_or(kind != "csv", |v| v.trim() != "source")
}
pub fn save_preview_mode(kind: &str, preview: bool) -> Result<()> {
    set(
        &preview_key(kind)?,
        if preview { "preview" } else { "source" },
    )
}
pub fn language() -> Option<String> {
    get("language")
        .ok()
        .flatten()
        .filter(|v| crate::locale::supported(v.trim()))
        .map(|v| v.trim().to_owned())
}
pub fn save_language(language: &str) -> Result<()> {
    anyhow::ensure!(crate::locale::supported(language), "Unknown language");
    set("language", language)
}
/// Unreadable preferences fail closed.
pub fn telemetry_enabled() -> bool {
    get("telemetry").is_ok_and(|v| v.is_none_or(|v| v.trim() == "true"))
}
pub fn save_telemetry_enabled(enabled: bool) -> Result<()> {
    set("telemetry", &enabled.to_string())
}
pub fn workspace_sessions() -> Result<BTreeMap<String, glim_core::update::WorkspaceSession>> {
    Ok(storage::sessions::load(&storage::open()?, "workspace")?
        .into_iter()
        .filter_map(|(id, sessions)| sessions.into_iter().next().map(|s| (id, s)))
        .collect())
}
pub fn save_workspace_sessions(
    sessions: &BTreeMap<String, glim_core::update::WorkspaceSession>,
) -> Result<()> {
    save_groups(
        "workspace",
        &sessions
            .iter()
            .map(|(id, s)| (id.clone(), vec![s.clone()]))
            .collect(),
    )
}
pub fn window_sessions() -> Result<BTreeMap<String, Vec<glim_core::update::WorkspaceSession>>> {
    storage::sessions::load(&storage::open()?, "window")
}
pub fn save_window_sessions(
    windows: &BTreeMap<String, Vec<glim_core::update::WorkspaceSession>>,
) -> Result<()> {
    save_groups("window", windows)
}
fn save_groups(
    scope: &str,
    groups: &BTreeMap<String, Vec<glim_core::update::WorkspaceSession>>,
) -> Result<()> {
    let mut db = storage::open()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    storage::sessions::save(&tx, scope, groups)?;
    tx.commit()?;
    Ok(())
}
