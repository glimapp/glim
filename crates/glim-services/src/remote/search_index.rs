mod incremental;

use super::{Connection, Session, sftp::Sftp};
use crate::{limits, work::Cancellation, workspace::SearchPage};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection as Database, OpenFlags, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{OpenOptions, TryLockError},
    io::Read,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{DirBuilderExt, OpenOptionsExt},
    },
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub struct IndexedPage {
    pub page: SearchPage,
    pub revision: i64,
    pub complete: bool,
    pub warning: Option<String>,
}

pub enum SearchIndexUpdate {
    Busy,
    Ready(Option<String>),
}

#[derive(Default)]
pub struct SearchIndexProgress {
    started: AtomicBool,
    cached_ready: AtomicBool,
    files: AtomicU64,
}
impl SearchIndexProgress {
    pub fn cache_ready(&self) -> bool {
        self.cached_ready.load(Ordering::Acquire)
    }
    pub fn files(&self) -> Option<u64> {
        self.started
            .load(Ordering::Acquire)
            .then(|| self.files.load(Ordering::Acquire))
    }
}

impl Session {
    pub fn search_scope(&self, roots: &[PathBuf]) -> String {
        scope(&self.connection, roots)
    }

    pub fn search(
        &self,
        roots: &[PathBuf],
        query: &str,
        offset: usize,
        revision: Option<i64>,
        cancel: &Cancellation,
    ) -> Result<Option<IndexedPage>> {
        cancel.check()?;
        let path = index_path(&self.connection, roots)?;
        let empty = || {
            Some(IndexedPage {
                page: SearchPage {
                    paths: Vec::new(),
                    more: false,
                },
                revision: -1,
                complete: false,
                warning: None,
            })
        };
        if !path.exists() {
            return Ok(if revision == Some(-1) { None } else { empty() });
        }
        let db = Database::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        configure(&db, cancel)?;
        if !db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='state')",
            [],
            |r| r.get::<_, bool>(0),
        )? {
            return Ok(if revision == Some(-1) { None } else { empty() });
        }
        let tx = db.unchecked_transaction()?;
        let (current, complete, warning) = tx.query_row(
            "SELECT revision,complete,warning FROM state WHERE id=1",
            [],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )?;
        if revision == Some(current) {
            return Ok(None);
        }
        let mut statement = tx.prepare(
            "SELECT path FROM files WHERE instr(folded,?1)>0 ORDER BY path LIMIT ?2 OFFSET ?3",
        )?;
        let mut paths = statement
            .query_map(
                (
                    query.trim().to_lowercase(),
                    limits::WORKSPACE_SEARCH_RESULTS as i64 + 1,
                    i64::try_from(offset)?,
                ),
                |r| Ok(PathBuf::from(OsString::from_vec(r.get(0)?))),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        cancel.check()?;
        let more = paths.len() > limits::WORKSPACE_SEARCH_RESULTS;
        paths.truncate(limits::WORKSPACE_SEARCH_RESULTS);
        Ok(Some(IndexedPage {
            page: SearchPage { paths, more },
            revision: current,
            complete,
            warning,
        }))
    }

    pub fn refresh_search_index(
        &self,
        roots: &[PathBuf],
        force: bool,
        cancel: &Cancellation,
        progress: &SearchIndexProgress,
    ) -> Result<SearchIndexUpdate> {
        cancel.check()?;
        let roots = normalized_roots(roots);
        ensure!(
            roots.iter().all(|p| valid_path(p)),
            "Invalid remote search root"
        );
        let path = index_path(&self.connection, &roots)?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path.parent().context("Missing search cache directory")?)?;
        // Serialize publishers across windows and processes without holding a SQLite
        // transaction over the whole scan. Readers can use every committed batch.
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(path.with_extension("lock"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Ok(SearchIndexUpdate::Busy),
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(&path)?;
        let mut db = Database::open(path)?;
        configure(&db, cancel)?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; BEGIN IMMEDIATE;
             CREATE TABLE IF NOT EXISTS files(path BLOB PRIMARY KEY, folded TEXT NOT NULL, generation INTEGER NOT NULL) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS pending(path BLOB PRIMARY KEY, done INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID;
             CREATE INDEX IF NOT EXISTS pending_done ON pending(done,path);
             CREATE TABLE IF NOT EXISTS state(id INTEGER PRIMARY KEY CHECK(id=1), generation INTEGER NOT NULL, revision INTEGER NOT NULL, completed_ms INTEGER NOT NULL, complete INTEGER NOT NULL, warning TEXT);
             INSERT OR IGNORE INTO state VALUES(1,0,0,0,0,NULL); COMMIT;",
        )?;
        let complete: bool =
            db.query_row("SELECT complete FROM state WHERE id=1", [], |r| r.get(0))?;
        if !force && complete {
            progress.cached_ready.store(true, Ordering::Release);
            return incremental::refresh(self, &mut db, &roots, cancel);
        }
        db.execute_batch("BEGIN IMMEDIATE; DELETE FROM pending; UPDATE state SET generation=generation+1,revision=revision+1,complete=0,warning=NULL; COMMIT;")?;
        let generation = db.query_row("SELECT generation FROM state", [], |r| r.get(0))?;
        progress.started.store(true, Ordering::Release);
        let mut writer = Writer {
            db,
            generation,
            changes: 0,
            first_file: true,
            published: Instant::now(),
            files: 0,
            progress,
        };
        let result = scan_shell(&self.connection, &roots, &mut writer, cancel);
        let result = match result {
            Ok(()) => Ok(()),
            Err(_) if cancel.check().is_ok() => {
                scan_sftp(&self.connection, &roots, &mut writer, cancel)
            }
            Err(error) => Err(error),
        };
        cancel.check()?;
        writer.publish()?;
        match result {
            Ok(()) => {
                let tx = writer.db.transaction()?;
                tx.execute("DELETE FROM files WHERE generation<>?1", [generation])?;
                tx.execute(
                    "UPDATE state SET revision=revision+1,complete=1,completed_ms=?1,warning=NULL",
                    [now_ms()],
                )?;
                tx.execute("DELETE FROM pending", [])?;
                tx.execute_batch("DROP TABLE IF EXISTS directories")?;
                tx.commit()?;
                Ok(SearchIndexUpdate::Ready(None))
            }
            Err(error) => {
                writer.db.execute(
                    "UPDATE state SET revision=revision+1,warning=?1",
                    [format!("{error:#}")],
                )?;
                Err(error)
            }
        }
    }
}

fn normalized_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = roots.to_vec();
    roots.sort();
    roots.dedup();
    roots
        .iter()
        .filter(|path| {
            !roots
                .iter()
                .any(|other| other != *path && path.starts_with(other))
        })
        .cloned()
        .collect()
}
fn scope(connection: &Connection, roots: &[PathBuf]) -> String {
    let mut hash = Sha256::new();
    for part in [
        connection.id.as_bytes(),
        connection.host.as_bytes(),
        connection.user.as_bytes(),
        &connection.port.unwrap_or(0).to_le_bytes(),
        connection
            .identity
            .as_deref()
            .unwrap_or(Path::new(""))
            .as_os_str()
            .as_bytes(),
    ] {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    for root in normalized_roots(roots) {
        let bytes = root.as_os_str().as_bytes();
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    format!("{:x}", hash.finalize())
}
fn index_path(connection: &Connection, roots: &[PathBuf]) -> Result<PathBuf> {
    Ok(crate::preferences::directory()?
        .join("RemoteSearch/v1")
        .join(format!("{}.sqlite3", scope(connection, roots))))
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn valid_path(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
}
fn configure(db: &Database, cancel: &Cancellation) -> Result<()> {
    db.busy_timeout(Duration::from_millis(
        limits::SQLITE_BUSY_MILLISECONDS as u64,
    ))?;
    db.pragma_update(None, "cache_size", -limits::SQLITE_CACHE_KIB)?;
    db.pragma_update(None, "temp_store", "FILE")?;
    let cancel = cancel.clone();
    db.progress_handler(1000, Some(move || cancel.check().is_err()));
    Ok(())
}

struct Writer<'a> {
    db: Database,
    generation: i64,
    changes: usize,
    first_file: bool,
    published: Instant,
    files: u64,
    progress: &'a SearchIndexProgress,
}
impl Writer<'_> {
    fn add(&mut self, path: &Path, directory: bool) -> Result<()> {
        if self.changes == 0 {
            self.db.execute_batch("BEGIN IMMEDIATE")?;
        }
        let bytes = path.as_os_str().as_bytes();
        if directory {
            self.db
                .execute("INSERT OR IGNORE INTO pending(path) VALUES(?1)", [bytes])?;
        } else {
            let added = self.db.execute("INSERT INTO files VALUES(?1,?2,?3) ON CONFLICT(path) DO UPDATE SET generation=excluded.generation,folded=excluded.folded WHERE files.generation<>excluded.generation", (bytes, path.to_string_lossy().to_lowercase(), self.generation))?;
            self.files += added as u64;
        }
        self.changes += 1;
        if (!directory && self.first_file)
            || self.changes >= limits::REMOTE_SEARCH_BATCH_PATHS
            || self.published.elapsed() >= Duration::from_millis(limits::REMOTE_SEARCH_POLL_MS)
        {
            self.publish()?;
            if !directory {
                self.first_file = false;
            }
        }
        Ok(())
    }
    fn publish(&mut self) -> Result<()> {
        if self.changes > 0 {
            self.db
                .execute_batch("UPDATE state SET revision=revision+1; COMMIT;")?;
            self.changes = 0;
            self.progress.files.store(self.files, Ordering::Release);
            self.published = Instant::now();
        }
        Ok(())
    }
}

fn scan_sftp(
    connection: &Connection,
    roots: &[PathBuf],
    writer: &mut Writer<'_>,
    cancel: &Cancellation,
) -> Result<()> {
    let mut transport = Sftp::connect(connection, cancel)?;
    for root in roots {
        writer.add(root, true)?;
    }
    writer.publish()?;
    let mut failure = None;
    loop {
        cancel.check()?;
        let directory = writer
            .db
            .query_row(
                "SELECT path FROM pending WHERE done=0 ORDER BY path LIMIT 1",
                [],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?;
        let Some(directory) = directory else { break };
        let path = PathBuf::from(OsString::from_vec(directory.clone()));
        let result = transport.visit_directory(&path, cancel, |entry| {
            if entry.path.file_name().is_some_and(|name| name == ".git")
                || (entry.is_dir && entry.is_symlink)
            {
                return Ok(());
            }
            writer.add(&entry.path, entry.is_dir)
        });
        writer.publish()?;
        writer
            .db
            .execute("UPDATE pending SET done=1 WHERE path=?1", [&directory])?;
        if let Err(error) = result {
            cancel.check()?;
            if !transport.healthy() {
                return Err(error);
            }
            if failure.is_none() {
                failure = Some(error.context(format!("Cannot scan {}", path.display())));
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

fn scan_shell(
    connection: &Connection,
    roots: &[PathBuf],
    writer: &mut Writer<'_>,
    cancel: &Cancellation,
) -> Result<()> {
    for root in roots {
        cancel.check()?;
        let mut script = b"printf 'GLIM_SEARCH_V1\\000'; find '".to_vec();
        for byte in root.as_os_str().as_bytes() {
            if *byte == b'\'' {
                script.extend_from_slice(b"'\"'\"'");
            } else {
                script.push(*byte);
            }
        }
        script.extend_from_slice(b"' -name .git -prune -o \\( -type f -o \\( -type l -exec test ! -d {} \\; \\) \\) -print0");
        let mut command = connection.ssh_command()?;
        command
            .args(["-o", "BatchMode=yes"])
            .arg(&connection.host)
            .arg(OsString::from_vec(script))
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command
            .spawn()
            .context("Cannot start remote filename scan")?;
        let mut output = child.stdout.take().context("Missing SSH search output")?;
        let process = ScanProcess::new(child, cancel.clone());
        let mut buffer = vec![0; limits::REMOTE_TRANSFER_BYTES as usize];
        let mut record = Vec::new();
        let mut header = false;
        loop {
            cancel.check()?;
            let count = output.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            *process.activity.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
            for byte in &buffer[..count] {
                if *byte != 0 {
                    ensure!(
                        record.len() < limits::REMOTE_SEARCH_RECORD_BYTES,
                        "Invalid remote filename stream"
                    );
                    record.push(*byte);
                    continue;
                }
                if !header {
                    ensure!(
                        record == b"GLIM_SEARCH_V1",
                        "Remote shell does not provide filename search"
                    );
                    header = true;
                } else {
                    let path = PathBuf::from(OsString::from_vec(std::mem::take(&mut record)));
                    ensure!(
                        valid_path(&path) && path.starts_with(root),
                        "Remote search returned an invalid path"
                    );
                    writer.add(&path, false)?;
                }
                record.clear();
            }
            writer.publish()?;
        }
        cancel.check()?;
        ensure!(
            header && record.is_empty(),
            "Incomplete remote filename stream"
        );
        let status = loop {
            cancel.check()?;
            if let Some(status) = process
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .try_wait()?
            {
                break status;
            }
            std::thread::sleep(Duration::from_millis(limits::REMOTE_SEARCH_POLL_MS));
        };
        ensure!(status.success(), "Remote filename scan was incomplete");
    }
    Ok(())
}

struct ScanProcess {
    child: Arc<Mutex<Child>>,
    activity: Arc<Mutex<Instant>>,
    done: Arc<AtomicBool>,
    watch: Option<std::thread::JoinHandle<()>>,
}
impl ScanProcess {
    fn new(child: Child, cancel: Cancellation) -> Self {
        let child = Arc::new(Mutex::new(child));
        let activity = Arc::new(Mutex::new(Instant::now()));
        let done = Arc::new(AtomicBool::new(false));
        let (worker, progress, stop) = (child.clone(), activity.clone(), done.clone());
        let watch = std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if cancel.check().is_err()
                    || progress.lock().unwrap_or_else(|e| e.into_inner()).elapsed()
                        >= Duration::from_millis(limits::REMOTE_IDLE_TIMEOUT_MS)
                {
                    if let Ok(mut child) = worker.lock() {
                        terminate(&mut child);
                    }
                    break;
                }
                std::thread::park_timeout(Duration::from_millis(limits::REMOTE_SEARCH_POLL_MS));
            }
        });
        Self {
            child,
            activity,
            done,
            watch: Some(watch),
        }
    }
}
impl Drop for ScanProcess {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(watch) = self.watch.take() {
            watch.thread().unpark();
            let _ = watch.join();
        }
        if let Ok(mut child) = self.child.lock() {
            if !matches!(child.try_wait(), Ok(Some(_))) {
                terminate(&mut child);
            }
            let _ = child.wait();
        }
    }
}
fn terminate(child: &mut Child) {
    let _ = Command::new("/bin/kill")
        .args(["-TERM", "--", &format!("-{}", child.id())])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}
