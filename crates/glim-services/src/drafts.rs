//! Private draft checkpoints. One retained background worker coalesces pending
//! snapshots; an OS lock prevents one Glim process recovering another's notes.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::BufReader,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
};

#[derive(Serialize, Deserialize)]
pub struct Draft {
    pub id: String,
    pub title: String,
    pub directory: PathBuf,
    pub text: String,
}

/// Opaque database record reference; no per-note checkpoint files are created.
pub struct Checkpoint {
    store: PathBuf,
    session: String,
    id: String,
}

enum Change {
    Save(Draft),
    Delete,
}
#[derive(Default)]
struct State {
    pending: BTreeMap<String, Change>,
    recovered: Vec<Checkpoint>,
    ready: bool,
    stopped: bool,
    error: Option<String>,
}

#[derive(Clone)]
pub struct Client {
    state: Arc<Mutex<State>>,
    wake: mpsc::SyncSender<()>,
}
pub struct Worker {
    state: Arc<Mutex<State>>,
    wake: mpsc::Receiver<()>,
}

pub fn channel() -> (Client, Worker) {
    let state = Arc::new(Mutex::new(State::default()));
    let (tx, rx) = mpsc::sync_channel(1);
    (
        Client {
            state: state.clone(),
            wake: tx,
        },
        Worker { state, wake: rx },
    )
}
impl Client {
    pub fn checkpoint(&self, draft: Draft) {
        let id = draft.id.clone();
        let change = if draft.text.is_empty() {
            Change::Delete
        } else {
            Change::Save(draft)
        };
        self.enqueue(id, change);
    }
    pub fn remove(&self, id: &str) {
        self.enqueue(id.to_owned(), Change::Delete);
    }
    fn enqueue(&self, id: String, change: Change) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.stopped {
            state.pending.insert(id, change);
            let _ = self.wake.try_send(());
        }
    }
    pub fn recovered(&self) -> Option<Vec<Checkpoint>> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.ready.then(|| std::mem::take(&mut state.recovered))
    }
    pub fn take_error(&self) -> Option<String> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .error
            .take()
    }
    pub fn shutdown(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).stopped = true;
        let _ = self.wake.try_send(());
    }
}
impl Worker {
    pub fn run(self) -> Result<()> {
        let result = self.run_inner();
        if let Err(error) = &result {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.error = Some(format!("{error:#}"));
            state.ready = true;
        }
        result
    }
    fn run_inner(&self) -> Result<()> {
        let root = crate::preferences::directory()?;
        self.run_at(&root)
    }
    fn run_at(&self, root: &Path) -> Result<()> {
        let locks = root.join("DraftLocks");
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&locks)?;
        std::fs::set_permissions(&locks, std::fs::Permissions::from_mode(0o700))?;
        let session = locks.join(format!(
            "session-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::DirBuilder::new().mode(0o700).create(&session)?;
        let lock_path = session.join(".lock.pending");
        let lock = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)?;
        lock.lock()?;
        std::fs::rename(lock_path, session.join(".lock"))?;
        let mut db = crate::storage::open_at(root)?;
        let owner = session
            .file_name()
            .and_then(|v| v.to_str())
            .context("Invalid draft session")?;
        let mut recovered = Vec::new();
        for source in [locks, root.join("Drafts")] {
            if !source.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&source)? {
                if self.state.lock().unwrap_or_else(|e| e.into_inner()).stopped {
                    break;
                }
                let entry = entry?;
                if !entry.file_type()?.is_dir() || entry.path() == session {
                    continue;
                }
                let Ok(other_lock) = OpenOptions::new()
                    .write(true)
                    .open(entry.path().join(".lock"))
                else {
                    continue;
                };
                if other_lock.try_lock().is_err() {
                    continue;
                }
                let old_owner = entry.file_name().to_string_lossy().into_owned();
                let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let mut imported = Vec::new();
                for record in std::fs::read_dir(entry.path())? {
                    let record = record?;
                    if !record.file_type()?.is_file()
                        || record.path().extension().is_none_or(|v| v != "json")
                    {
                        continue;
                    }
                    let relative = record
                        .path()
                        .strip_prefix(root)?
                        .to_string_lossy()
                        .into_owned();
                    let exists: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM imported_files WHERE path=?1)",
                        [&relative],
                        |r| r.get(0),
                    )?;
                    if !exists {
                        let draft =
                            read_legacy(&record.path(), &crate::work::Cancellation::default())?;
                        tx.execute("INSERT INTO drafts(session,id,title,directory,text) VALUES(?1,?2,?3,?4,?5)",rusqlite::params![owner,draft.id,draft.title,crate::storage::path_text(&draft.directory)?,draft.text])?;
                        tx.execute("INSERT INTO imported_files(path) VALUES(?1)", [&relative])?;
                    }
                    imported.push(relative);
                }
                tx.execute(
                    "UPDATE drafts SET session=?1 WHERE session=?2",
                    (owner, &old_owner),
                )?;
                tx.commit()?;
                for relative in imported {
                    let target = root.join("LegacyMigration").join(&relative);
                    if !target.exists() {
                        std::fs::create_dir_all(target.parent().context("Invalid draft archive")?)?;
                        std::fs::rename(root.join(relative), target)?;
                    }
                }
                let _ = std::fs::remove_file(entry.path().join(".lock"));
                let _ = std::fs::remove_dir(entry.path());
            }
        }
        let ids = db
            .prepare("SELECT id FROM drafts WHERE session=?1 ORDER BY id")?
            .query_map([owner], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for id in ids {
            recovered.push(Checkpoint {
                store: root.to_owned(),
                session: owner.to_owned(),
                id,
            });
        }
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.recovered = recovered;
            state.ready = true;
        }
        loop {
            let disconnected = self.wake.recv().is_err();
            let pending = {
                let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                std::mem::take(&mut state.pending)
            };
            for (id, change) in pending {
                let result = apply(&db, owner, &id, change);
                if let Err(error) = result {
                    log::error!("draft checkpoint failed: {error:#}");
                    self.state.lock().unwrap_or_else(|e| e.into_inner()).error =
                        Some(format!("{error:#}"));
                }
            }
            let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if disconnected || state.stopped && state.pending.is_empty() {
                break;
            }
        }
        // Leave the marker in place; the next process can claim any remaining
        // records or remove the empty session while holding its OS lock.
        drop(lock);
        Ok(())
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}
fn apply(db: &rusqlite::Connection, session: &str, id: &str, change: Change) -> Result<()> {
    ensure!(valid_id(id), "Invalid draft identity");
    match change {
        Change::Delete => {
            db.execute(
                "DELETE FROM drafts WHERE session=?1 AND id=?2",
                (session, id),
            )?;
        }
        Change::Save(draft) => {
            db.execute("INSERT INTO drafts(session,id,title,directory,text) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(session,id) DO UPDATE SET title=excluded.title,directory=excluded.directory,text=excluded.text",rusqlite::params![session,id,draft.title,crate::storage::path_text(&draft.directory)?,draft.text])?;
        }
    }
    Ok(())
}

pub fn read(checkpoint: &Checkpoint) -> Result<Draft> {
    read_cancellable(checkpoint, &crate::work::Cancellation::default())
}
pub fn read_cancellable(
    checkpoint: &Checkpoint,
    cancel: &crate::work::Cancellation,
) -> Result<Draft> {
    cancel.check()?;
    ensure!(valid_id(&checkpoint.id), "Invalid recovered note identity");
    let db = crate::storage::open_at(&checkpoint.store)?;
    let owner = &checkpoint.session;
    let id = &checkpoint.id;
    let draft = db.query_row(
        "SELECT title,directory,text FROM drafts WHERE session=?1 AND id=?2",
        (owner, id),
        |r| {
            Ok(Draft {
                id: id.clone(),
                title: r.get(0)?,
                directory: PathBuf::from(r.get::<_, String>(1)?),
                text: r.get(2)?,
            })
        },
    )?;
    cancel.check()?;
    Ok(draft)
}
fn read_legacy(path: &Path, cancel: &crate::work::Cancellation) -> Result<Draft> {
    let input = crate::work::Reader {
        inner: File::open(path)?,
        cancel: cancel.clone(),
    };
    let draft: Draft =
        serde_json::from_reader(BufReader::new(input)).context("Cannot read recovered note")?;
    ensure!(valid_id(&draft.id), "Invalid recovered note identity");
    ensure!(
        path.file_stem()
            .is_some_and(|name| name == draft.id.as_str()),
        "Draft identity does not match its recovery file"
    );
    Ok(draft)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(text: &str) -> Draft {
        Draft {
            id: "note-1".into(),
            title: "Untitled-1".into(),
            directory: PathBuf::from("/tmp"),
            text: text.into(),
        }
    }
    fn start(root: &Path) -> (Client, std::thread::JoinHandle<Result<()>>) {
        let (client, worker) = channel();
        let root = root.to_path_buf();
        let task = std::thread::spawn(move || worker.run_at(&root));
        (client, task)
    }
    fn ready(client: &Client) -> Vec<Checkpoint> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(paths) = client.recovered() {
                return paths;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "recovery worker did not initialize"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    #[test]
    fn checkpoints_survive_restart_with_latest_unicode_content_and_explicit_discard() -> Result<()>
    {
        let directory = tempfile::tempdir()?;
        let (first, task) = start(directory.path());
        ready(&first);
        first.checkpoint(note("old"));
        first.checkpoint(note("恢复后的正文\r\nsecond line"));
        first.shutdown();
        task.join().unwrap()?;
        let (second, task) = start(directory.path());
        let paths = ready(&second);
        assert_eq!(paths.len(), 1);
        assert_eq!(read(&paths[0])?.text, "恢复后的正文\r\nsecond line");
        second.remove("note-1");
        second.shutdown();
        task.join().unwrap()?;
        let (third, task) = start(directory.path());
        assert!(ready(&third).is_empty());
        third.shutdown();
        task.join().unwrap()?;
        Ok(())
    }
    #[test]
    fn a_running_session_is_not_claimed_by_another_worker() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (first, first_task) = start(directory.path());
        ready(&first);
        first.checkpoint(note("still being edited"));
        let (second, second_task) = start(directory.path());
        assert!(ready(&second).is_empty());
        second.shutdown();
        second_task.join().unwrap()?;
        first.shutdown();
        first_task.join().unwrap()?;
        let (third, task) = start(directory.path());
        let paths = ready(&third);
        assert_eq!(read(&paths[0])?.text, "still being edited");
        third.shutdown();
        task.join().unwrap()?;
        Ok(())
    }
}
