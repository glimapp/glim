use super::*;
use crate::preferences::{ExplorerExpansion, RecentWorkspace};
use glim_core::update::WorkspaceSession;
use std::collections::BTreeMap;

fn source(db: &Connection, directory: &Path, name: &str) -> Result<Option<Vec<u8>>> {
    match std::fs::read(directory.join(name)) {
        Ok(bytes) => {
            db.execute("INSERT INTO imported_files(path) VALUES(?1)", [name])?;
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("Cannot read legacy {name}")),
    }
}
fn json<T: serde::de::DeserializeOwned>(
    db: &Connection,
    directory: &Path,
    name: &str,
) -> Result<Option<T>> {
    source(db, directory, name)?
        .map(|bytes| {
            serde_json::from_slice(&bytes).with_context(|| format!("Invalid legacy {name}"))
        })
        .transpose()
}
pub(super) fn import(db: &Connection, directory: &Path) -> Result<()> {
    for key in [
        "word-wrap",
        "appearance",
        "html-view",
        "markdown-view",
        "csv-view",
        "language",
        "telemetry",
        "telemetry-installation-id",
    ] {
        if let Some(bytes) = source(db, directory, key)? {
            set_setting(db, key, std::str::from_utf8(&bytes)?.trim())?;
        } else if key == "word-wrap" {
            let legacy = directory
                .parent()
                .context("Invalid data directory")?
                .join("Glimpse/word-wrap");
            if let Ok(value) = std::fs::read_to_string(legacy) {
                set_setting(db, key, value.trim())?;
            }
        }
    }
    let roots: Vec<PathBuf> = json(db, directory, "workspace.json")?.unwrap_or_default();
    save_paths(db, "workspace", "", &roots)?;
    let recent: Vec<RecentWorkspace> = json(db, directory, "recent-workspaces.json")?
        .unwrap_or_else(|| {
            if roots.is_empty() {
                vec![]
            } else {
                vec![RecentWorkspace {
                    id: "legacy".into(),
                    folders: roots,
                    name: None,
                }]
            }
        });
    crate::preferences::save_recent_at(db, &recent)?;
    let expanded: ExplorerExpansion = json(db, directory, "explorer.json")?.unwrap_or_default();
    for (root, paths) in expanded {
        save_paths(db, "explorer", path_text(&root)?, &paths)?;
    }
    let workspaces: BTreeMap<String, WorkspaceSession> =
        json(db, directory, "workspace-sessions.json")?.unwrap_or_default();
    sessions::save(
        db,
        "workspace",
        &workspaces
            .into_iter()
            .map(|(id, s)| (id, vec![s]))
            .collect(),
    )?;
    let windows: BTreeMap<String, Vec<WorkspaceSession>> =
        json(db, directory, "window-sessions.json")?.unwrap_or_default();
    sessions::save(db, "window", &windows)?;
    for (name, id) in [
        ("update-session.json", "pending"),
        ("last-update-session.json", "last"),
        ("rejected-update-session.json", "rejected"),
    ] {
        if let Some(bytes) = source(db, directory, name)? {
            // Preserve rejected or obsolete checkpoints verbatim; validation happens on consumption.
            set_record(db, "update", id, std::str::from_utf8(&bytes)?)?;
        }
    }
    let books = directory.join("Books");
    if books.is_dir() {
        for entry in std::fs::read_dir(books)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(id) = name.to_str().and_then(|s| s.strip_suffix(".json")) else {
                continue;
            };
            if id.len() != 64
                || !id.bytes().all(|b| b.is_ascii_hexdigit())
                || !entry.file_type()?.is_file()
            {
                continue;
            }
            let relative = format!("Books/{id}.json");
            let state: crate::books::preferences::ReadingState =
                json(db, directory, &relative)?.context("Missing book state")?;
            crate::books::preferences::validate(&state)?;
            set_record(
                db,
                "book",
                &format!("ebook:{id}"),
                &serde_json::to_string(&state)?,
            )?;
        }
    }
    Ok(())
}
pub(super) fn archive_sources(db: &Connection, directory: &Path) -> Result<()> {
    let names = db
        .prepare("SELECT path FROM imported_files")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for name in names {
        if name.starts_with("Drafts/") {
            continue;
        }
        let source = directory.join(&name);
        let target = directory.join("LegacyMigration").join(&name);
        if source.is_file() && !target.exists() {
            std::fs::create_dir_all(target.parent().context("Invalid migration archive")?)?;
            std::fs::rename(source, target)?;
        }
    }
    Ok(())
}
