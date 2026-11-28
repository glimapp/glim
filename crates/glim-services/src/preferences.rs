//! Application preferences and workspace state backed by the shared SQLite store.
use crate::storage::{self, set_setting, setting};
use anyhow::Result;
use glim_core::{ExplorerExpansion, RecentWorkspace};
use std::{collections::BTreeMap, path::PathBuf};

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
pub fn recent_workspaces() -> Result<Vec<RecentWorkspace>> {
    let mut db = storage::open()?;
    let tx = db.transaction()?;
    let recent = storage::sessions::recent_at(&tx)?;
    tx.commit()?;
    Ok(recent)
}
pub fn remember_recent_workspace(mut workspace: RecentWorkspace) -> Result<Vec<RecentWorkspace>> {
    let mut db = storage::open()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut recent = storage::sessions::recent_at(&tx)?;
    if workspace.name.is_none() {
        workspace.name = recent
            .iter()
            .find(|e| {
                e.id == workspace.id
                    || (!crate::remote::is_remote_id(&workspace.id)
                        && !crate::remote::is_remote_id(&e.id)
                        && e.folders == workspace.folders)
            })
            .and_then(|e| e.name.clone());
    }
    recent.retain(|e| {
        e.id != workspace.id
            && (crate::remote::is_remote_id(&workspace.id)
                || crate::remote::is_remote_id(&e.id)
                || workspace.folders.is_empty()
                || e.folders != workspace.folders)
    });
    if !workspace.folders.is_empty() {
        recent.insert(0, workspace);
    }
    storage::sessions::save_recent_at(&tx, &recent)?;
    tx.commit()?;
    Ok(recent)
}
pub fn remove_recent_workspace(id: &str) -> Result<Vec<RecentWorkspace>> {
    let mut db = storage::open()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut recent = storage::sessions::recent_at(&tx)?;
    recent.retain(|e| e.id != id);
    storage::sessions::save_recent_at(&tx, &recent)?;
    tx.commit()?;
    Ok(recent)
}
fn get(key: &str) -> Result<Option<String>> {
    setting(&storage::open()?, key)
}
fn set(key: &str, value: &str) -> Result<()> {
    set_setting(&storage::open()?, key, value)
}
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GitPreferences {
    pub worktrees: bool,
}
impl Default for GitPreferences {
    fn default() -> Self {
        Self { worktrees: true }
    }
}
pub fn git_preferences() -> GitPreferences {
    get("git-preferences")
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}
pub fn save_git_preferences(value: GitPreferences) -> Result<()> {
    set("git-preferences", &serde_json::to_string(&value)?)
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
pub fn workspace_sessions() -> Result<BTreeMap<String, glim_core::session::WorkspaceSession>> {
    Ok(storage::sessions::load(&storage::open()?, "workspace")?
        .into_iter()
        .filter_map(|(id, sessions)| sessions.into_iter().next().map(|s| (id, s)))
        .collect())
}
pub fn save_workspace_sessions(
    sessions: &BTreeMap<String, glim_core::session::WorkspaceSession>,
) -> Result<()> {
    save_groups(
        "workspace",
        &sessions
            .iter()
            .map(|(id, s)| (id.clone(), vec![s.clone()]))
            .collect(),
    )
}
pub fn window_sessions() -> Result<BTreeMap<String, Vec<glim_core::session::WorkspaceSession>>> {
    storage::sessions::load(&storage::open()?, "window")
}
pub fn save_window_sessions(
    windows: &BTreeMap<String, Vec<glim_core::session::WorkspaceSession>>,
) -> Result<()> {
    save_groups("window", windows)
}
fn save_groups(
    scope: &str,
    groups: &BTreeMap<String, Vec<glim_core::session::WorkspaceSession>>,
) -> Result<()> {
    let mut db = storage::open()?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    storage::sessions::save(&tx, scope, groups)?;
    tx.commit()?;
    Ok(())
}
