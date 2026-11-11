use super::*;

pub(super) fn refresh(
    session: &Session,
    db: &mut Database,
    roots: &[PathBuf],
    cancel: &Cancellation,
) -> Result<SearchIndexUpdate> {
    seed(db, roots, cancel)?;
    let now = now_ms();
    let directories = db
        .prepare("SELECT path,modified,verified_ms FROM directories WHERE checked_ms<=?1 AND (error IS NULL OR checked_ms<=?3) ORDER BY checked_ms,path LIMIT ?2")?
        .query_map(
            (now.saturating_sub(limits::REMOTE_SEARCH_DIRECTORY_POLL_MS), limits::REMOTE_SEARCH_DIRECTORY_BATCH as i64, now.saturating_sub(limits::REMOTE_SEARCH_REFRESH_MS)),
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Option<u32>>(1)?, row.get::<_, u64>(2)?)),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if directories.is_empty() {
        let warning = db.query_row("SELECT warning FROM state", [], |r| r.get(0))?;
        return Ok(SearchIndexUpdate::Ready(warning));
    }
    let mut retained = match session.search_transport.try_lock() {
        Ok(transport) => transport,
        Err(std::sync::TryLockError::WouldBlock) => return Ok(SearchIndexUpdate::Busy),
        Err(_) => anyhow::bail!("Search connection unavailable; reconnect to continue"),
    };
    if retained
        .as_ref()
        .is_none_or(|transport| !transport.healthy())
    {
        *retained = Some(Sftp::connect(&session.connection, cancel)?);
    }
    let transport = retained.as_mut().context("Missing search connection")?;
    db.execute_batch("CREATE TEMP TABLE listing(path BLOB PRIMARY KEY, directory INTEGER NOT NULL, folded TEXT NOT NULL) WITHOUT ROWID;")?;
    for (bytes, modified, verified) in directories {
        cancel.check()?;
        let path = PathBuf::from(OsString::from_vec(bytes.clone()));
        // A parent processed earlier in this batch may have removed this subtree.
        if !db.query_row(
            "SELECT EXISTS(SELECT 1 FROM directories WHERE path=?1)",
            [&bytes],
            |r| r.get::<_, bool>(0),
        )? {
            continue;
        }
        let result = update_directory(db, transport, &path, modified, verified, roots, cancel);
        cancel.check()?;
        let error = result
            .err()
            .map(|error| format!("{}: {error:#}", path.display()));
        db.execute(
            "UPDATE directories SET checked_ms=?2,error=?3 WHERE path=?1",
            (&bytes, now_ms(), &error),
        )?;
        if !transport.healthy() {
            break;
        }
    }
    let warning: Option<String> = db
        .query_row(
            "SELECT error FROM directories WHERE error IS NOT NULL ORDER BY path LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    db.execute(
        "UPDATE state SET warning=?1,revision=revision+1 WHERE warning IS NOT ?1",
        [&warning],
    )?;
    Ok(SearchIndexUpdate::Ready(warning))
}

fn seed(db: &mut Database, roots: &[PathBuf], cancel: &Cancellation) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS directories(
        path BLOB PRIMARY KEY, parent BLOB, modified INTEGER,
        checked_ms INTEGER NOT NULL DEFAULT 0, verified_ms INTEGER NOT NULL DEFAULT 0, error TEXT
    ) WITHOUT ROWID;
    CREATE INDEX IF NOT EXISTS directories_checked ON directories(checked_ms,path);
    CREATE INDEX IF NOT EXISTS directories_parent ON directories(parent,path);",
    )?;
    if db.query_row("SELECT EXISTS(SELECT 1 FROM directories)", [], |r| {
        r.get::<_, bool>(0)
    })? {
        return Ok(());
    }
    let tx = db.transaction()?;
    for root in roots {
        insert_directory(&tx, root, roots)?;
    }
    {
        let mut statement = tx.prepare("SELECT path FROM files")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            cancel.check()?;
            let path = PathBuf::from(OsString::from_vec(row.get(0)?));
            for parent in path.ancestors().skip(1) {
                if !roots.iter().any(|root| parent.starts_with(root)) {
                    break;
                }
                if insert_directory(&tx, parent, roots)? == 0 {
                    break;
                }
            }
        }
    }
    tx.commit()?;
    Ok(())
}

fn insert_directory(db: &Database, path: &Path, roots: &[PathBuf]) -> Result<usize> {
    let parent = (!roots.iter().any(|root| root == path))
        .then(|| path.parent().map(|p| p.as_os_str().as_bytes()))
        .flatten();
    Ok(db.execute(
        "INSERT OR IGNORE INTO directories(path,parent) VALUES(?1,?2)",
        (path.as_os_str().as_bytes(), parent),
    )?)
}

fn update_directory(
    db: &mut Database,
    transport: &mut Sftp,
    path: &Path,
    modified: Option<u32>,
    verified: u64,
    roots: &[PathBuf],
    cancel: &Cancellation,
) -> Result<()> {
    let stamp = transport.lstat_optional(path, cancel)?;
    let Some(stamp) = stamp.filter(|stamp| stamp.mode & 0o170000 == 0o040000) else {
        ensure!(
            !roots.iter().any(|root| root == path),
            "Workspace root is unavailable"
        );
        let tx = db.transaction()?;
        remove_tree(&tx, path)?;
        tx.execute("UPDATE state SET revision=revision+1", [])?;
        tx.commit()?;
        return Ok(());
    };
    if stamp.modified.is_some()
        && stamp.modified == modified
        && now_ms().saturating_sub(verified) < limits::REMOTE_SEARCH_REFRESH_MS
    {
        return Ok(());
    }
    let staging = db.transaction()?;
    staging.execute("DELETE FROM listing", [])?;
    transport.visit_directory(path, cancel, |entry| {
        if entry.path.file_name().is_some_and(|name| name == ".git")
            || (entry.is_dir && entry.is_symlink)
        {
            return Ok(());
        }
        staging.execute(
            "INSERT OR REPLACE INTO listing VALUES(?1,?2,?3)",
            (
                entry.path.as_os_str().as_bytes(),
                entry.is_dir,
                entry.path.to_string_lossy().to_lowercase(),
            ),
        )?;
        Ok(())
    })?;
    staging.commit()?;
    cancel.check()?;
    let tx = db.transaction()?;
    let mut changes = 0;
    // Only a complete listing authorizes removals. Failed/partial reads keep the old cache.
    loop {
        let removed: Option<Vec<u8>> = tx.query_row(
            "SELECT path FROM directories WHERE parent=?1 AND path NOT IN (SELECT path FROM listing WHERE directory=1) LIMIT 1",
            [path.as_os_str().as_bytes()], |r| r.get(0),
        ).optional()?;
        let Some(removed) = removed else { break };
        cancel.check()?;
        remove_tree(&tx, &PathBuf::from(OsString::from_vec(removed)))?;
        changes += 1;
    }
    let (prefix, end) = descendants(path);
    changes += tx.execute(
        "DELETE FROM files WHERE path>=?1 AND path<?2 AND instr(substr(path,length(?1)+1),x'2f')=0
         AND path NOT IN (SELECT path FROM listing WHERE directory=0)",
        (&prefix, &end),
    )?;
    changes += tx.execute(
        "INSERT INTO files SELECT path,folded,(SELECT generation FROM state) FROM listing WHERE directory=0
         ON CONFLICT(path) DO NOTHING", [],
    )?;
    changes += tx.execute("INSERT OR IGNORE INTO directories(path,parent) SELECT path,?1 FROM listing WHERE directory=1", [path.as_os_str().as_bytes()])?;
    tx.execute(
        "UPDATE directories SET modified=?2,verified_ms=?3 WHERE path=?1",
        (path.as_os_str().as_bytes(), stamp.modified, now_ms()),
    )?;
    if changes > 0 {
        tx.execute("UPDATE state SET revision=revision+1", [])?;
    }
    tx.commit()?;
    Ok(())
}

fn descendants(path: &Path) -> (Vec<u8>, Vec<u8>) {
    let mut prefix = path.as_os_str().as_bytes().to_vec();
    if !prefix.ends_with(b"/") {
        prefix.push(b'/');
    }
    let mut end = prefix.clone();
    *end.last_mut().unwrap() += 1;
    (prefix, end)
}

fn remove_tree(db: &Database, path: &Path) -> Result<()> {
    let (prefix, end) = descendants(path);
    db.execute(
        "DELETE FROM files WHERE path>=?1 AND path<?2",
        (&prefix, &end),
    )?;
    db.execute(
        "DELETE FROM directories WHERE (path>=?1 AND path<?2) OR path=?3",
        (&prefix, &end, path.as_os_str().as_bytes()),
    )?;
    Ok(())
}
