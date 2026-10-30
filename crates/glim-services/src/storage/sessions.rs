use super::*;
use glim_core::update::{SessionTab, WorkspaceSession};
use std::collections::BTreeMap;

pub(crate) fn save(
    db: &Connection,
    scope: &str,
    groups: &BTreeMap<String, Vec<WorkspaceSession>>,
) -> Result<()> {
    db.execute("DELETE FROM session_groups WHERE scope=?1", [scope])?;
    for (owner, sessions) in groups {
        db.execute(
            "INSERT INTO session_groups(scope,id) VALUES(?1,?2)",
            (scope, owner),
        )?;
        for (position, session) in sessions.iter().enumerate() {
            ensure!(
                session.active.is_none_or(|i| i < session.tabs.len()),
                "Invalid active session tab"
            );
            db.execute("INSERT INTO sessions(scope,owner,position,workspace_id,name,window_group,active) VALUES(?1,?2,?3,?4,?5,?6,?7)", rusqlite::params![scope,owner,position as i64,session.workspace_id,session.name,session.window_group,session.active.map(|v| v as i64)])?;
            let id = db.last_insert_rowid();
            for (position, path) in session.roots.iter().enumerate() {
                db.execute(
                    "INSERT INTO session_roots VALUES(?1,?2,?3)",
                    (id, position as i64, path_text(path)?),
                )?;
            }
            for (position, tab) in session.tabs.iter().enumerate() {
                let (kind, path, flag) = match tab {
                    SessionTab::File { path, preview } => ("file", path, *preview),
                    SessionTab::Playlist { path, shuffle } => ("playlist", path, *shuffle),
                };
                db.execute(
                    "INSERT INTO session_tabs VALUES(?1,?2,?3,?4,?5)",
                    (id, position as i64, kind, path_text(path)?, flag),
                )?;
            }
        }
    }
    Ok(())
}
pub(crate) fn load(
    db: &Connection,
    scope: &str,
) -> Result<BTreeMap<String, Vec<WorkspaceSession>>> {
    let snapshot = db.unchecked_transaction()?;
    let db = &snapshot;
    let owners = db
        .prepare("SELECT id FROM session_groups WHERE scope=?1 ORDER BY id")?
        .query_map([scope], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut groups = BTreeMap::new();
    for owner in owners {
        let mut statement = db.prepare("SELECT id,workspace_id,name,window_group,active FROM sessions WHERE scope=?1 AND owner=?2 ORDER BY position")?;
        let mut rows = statement.query((scope, &owner))?;
        let mut sessions = Vec::new();
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let roots = db
                .prepare("SELECT path FROM session_roots WHERE session=?1 ORDER BY position")?
                .query_map([id], |r| Ok(PathBuf::from(r.get::<_, String>(0)?)))?
                .collect::<rusqlite::Result<_>>()?;
            let tabs = db
                .prepare(
                    "SELECT kind,path,flag FROM session_tabs WHERE session=?1 ORDER BY position",
                )?
                .query_map([id], |r| {
                    let path = PathBuf::from(r.get::<_, String>(1)?);
                    Ok(if r.get::<_, String>(0)? == "file" {
                        SessionTab::File {
                            path,
                            preview: r.get(2)?,
                        }
                    } else {
                        SessionTab::Playlist {
                            path,
                            shuffle: r.get(2)?,
                        }
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let active = row.get::<_, Option<i64>>(4)?.map(|v| v as usize);
            ensure!(
                active.is_none_or(|i| i < tabs.len()),
                "Invalid stored active session tab"
            );
            sessions.push(WorkspaceSession {
                roots,
                tabs,
                active,
                workspace_id: row.get(1)?,
                name: row.get(2)?,
                window_group: row.get(3)?,
            });
        }
        groups.insert(owner, sessions);
    }
    snapshot.commit()?;
    Ok(groups)
}
