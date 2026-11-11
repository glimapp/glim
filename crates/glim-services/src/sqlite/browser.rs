//! Read-only, bounded SQLite queries against a private WAL-aware snapshot.
use super::Snapshot;
use crate::{limits, work::Cancellation};
use anyhow::{Context, Result, anyhow, ensure};
use rusqlite::{Connection, params_from_iter};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

pub struct Database {
    snapshot: Snapshot,
    pub tables: Vec<String>,
}

#[derive(Debug)]
enum OpenStage {
    Snapshot,
    Recovery,
    Schema,
}
impl std::fmt::Display for OpenStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Snapshot => "Could not prepare the database snapshot",
            Self::Recovery => "Could not open the database snapshot",
            Self::Schema => "Could not read the database schema",
        })
    }
}
impl std::error::Error for OpenStage {}

pub fn failure_stage(error: &anyhow::Error) -> &'static str {
    match error.downcast_ref::<OpenStage>() {
        Some(OpenStage::Snapshot) => "snapshot",
        Some(OpenStage::Recovery) => "recovery",
        Some(OpenStage::Schema) => "schema",
        None => "unknown",
    }
}
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub table: String,
    pub columns: Vec<String>,
    pub order: Vec<String>,
    pub search: String,
    pub sort: Option<(usize, bool)>,
    pub page: usize,
    pub start_column: usize,
    pub seconds: u64,
}
pub struct Columns {
    pub names: Vec<String>,
    pub order: Vec<String>,
}
pub use glim_core::table::SqlitePage as Page;
fn preview_text(value: &[u8]) -> std::borrow::Cow<'_, str> {
    if let Ok(text) = std::str::from_utf8(value) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut text = String::with_capacity(value.len() * 3);
    for chunk in value.utf8_chunks() {
        text.push_str(chunk.valid());
        if !chunk.invalid().is_empty() {
            text.push('\u{fffd}');
        }
    }
    std::borrow::Cow::Owned(text)
}
pub fn open(path: &Path, cancel: &Cancellation) -> Result<Arc<Database>> {
    let snapshot = super::prepare_cancellable(path, cancel).context(OpenStage::Snapshot)?;
    let path = snapshot
        .database
        .as_ref()
        .ok_or_else(|| super::SnapshotFailure::MissingMainDatabase(snapshot.information.clone()))
        .context(OpenStage::Snapshot)?;
    // Recovery is allowed only on the private copy, then all browser connections are read-only.
    let connection = Connection::open(path).context(OpenStage::Recovery)?;
    connection
        .busy_timeout(Duration::from_millis(
            limits::SQLITE_BUSY_MILLISECONDS as u64,
        ))
        .context(OpenStage::Recovery)?;
    let token = cancel.clone();
    let started = Instant::now();
    connection.progress_handler(
        1000,
        Some(move || {
            token.check().is_err()
                || started.elapsed() > Duration::from_secs(limits::SQLITE_QUERY_SECONDS)
        }),
    );
    connection
        .execute_batch("PRAGMA query_only=ON;")
        .context(OpenStage::Recovery)?;
    let tables = (|| -> rusqlite::Result<Vec<String>> {
        connection
            .prepare("SELECT name FROM sqlite_schema WHERE type IN ('table','view') ORDER BY name")?
            .query_map([], |row| row.get(0))?
            .collect()
    })()
    .context(OpenStage::Schema)?;
    Ok(Arc::new(Database { snapshot, tables }))
}
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
impl Database {
    pub fn source_stamp(&self) -> Option<super::SourceStamp> {
        self.snapshot.source_stamp.clone()
    }
    fn connection(&self, cancel: &Cancellation, seconds: u64) -> Result<Connection> {
        let connection = Connection::open_with_flags(
            self.snapshot.database.as_ref().unwrap(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        connection.busy_timeout(Duration::from_millis(
            limits::SQLITE_BUSY_MILLISECONDS as u64,
        ))?;
        connection.execute_batch(&format!(
            "PRAGMA query_only=ON; PRAGMA cache_size=-{};",
            limits::SQLITE_CACHE_KIB
        ))?;
        let cancel = cancel.clone();
        let started = Instant::now();
        let allowance = Duration::from_secs(seconds.max(limits::SQLITE_QUERY_SECONDS));
        connection.progress_handler(
            1000,
            Some(move || cancel.check().is_err() || started.elapsed() > allowance),
        );
        Ok(connection)
    }
    pub fn columns(&self, table: &str, cancel: &Cancellation) -> Result<Columns> {
        let connection = self.connection(cancel, limits::SQLITE_QUERY_SECONDS)?;
        let entries = connection
            .prepare("SELECT name, pk FROM pragma_table_xinfo(?) WHERE hidden != 1 ORDER BY cid")?
            .query_map([table], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let names = entries
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        let mut keys = entries
            .into_iter()
            .filter(|(_, pk)| *pk > 0)
            .collect::<Vec<_>>();
        keys.sort_by_key(|(_, pk)| *pk);
        let (kind, definition): (String, String) = connection.query_row(
            "SELECT type, coalesce(sql,'') FROM sqlite_schema WHERE name=?",
            [table],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let rowid = ["_rowid_", "rowid", "oid"]
            .into_iter()
            .find(|alias| !names.iter().any(|name| name.eq_ignore_ascii_case(alias)));
        let order = if kind == "table"
            && !definition.to_ascii_uppercase().contains("WITHOUT ROWID")
            && let Some(rowid) = rowid
        {
            vec![rowid.to_string()]
        } else if !keys.is_empty() {
            keys.into_iter().map(|(name, _)| name).collect()
        } else {
            names.clone()
        };
        Ok(Columns { names, order })
    }
    fn predicate(query: &Query) -> (String, Vec<String>) {
        let mut conditions = Vec::new();
        let mut values = Vec::new();
        if !query.search.is_empty() {
            // One parameter reused across all columns avoids SQLite's variable limit.
            values.push(query.search.clone());
            fn disjunction(parts: &[String]) -> String {
                match parts.len() {
                    0 => "0".into(),
                    1 => parts[0].clone(),
                    len => format!(
                        "({} OR {})",
                        disjunction(&parts[..len / 2]),
                        disjunction(&parts[len / 2..])
                    ),
                }
            }
            conditions.push(disjunction(
                &query
                    .columns
                    .iter()
                    .map(|column| {
                        format!(
                            "instr(lower(CAST({} AS TEXT)), lower(?1)) > 0",
                            quote(column)
                        )
                    })
                    .collect::<Vec<_>>(),
            ));
        }
        (
            if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            },
            values,
        )
    }
    pub fn count(&self, query: &Query, cancel: &Cancellation) -> Result<u64> {
        let connection = self.connection(cancel, query.seconds)?;
        let (predicate, values) = Self::predicate(query);
        Ok(connection.query_row(
            &format!("SELECT count(*) FROM {}{}", quote(&query.table), predicate),
            params_from_iter(values.iter()),
            |row| row.get(0),
        )?)
    }
    fn tail(query: &Query) -> (String, Vec<String>) {
        let (mut tail, values) = Self::predicate(query);
        let mut order = Vec::new();
        if let Some((index, descending)) = query.sort
            && let Some(column) = query.columns.get(index)
        {
            order.push(format!(
                "{} {}",
                quote(column),
                if descending { "DESC" } else { "ASC" }
            ));
        }
        order.extend(query.order.iter().map(|column| quote(column)));
        if !order.is_empty() {
            tail.push_str(&format!(" ORDER BY {}", order.join(",")));
        }
        (tail, values)
    }
    pub fn page(&self, query: &Query, cancel: &Cancellation) -> Result<Page> {
        self.page_with_pool(query, cancel, crate::resources::pool())
    }
    fn page_with_pool(
        &self,
        query: &Query,
        cancel: &Cancellation,
        pool: &crate::resources::ResourcePool,
    ) -> Result<Page> {
        let connection = self.connection(cancel, query.seconds)?;
        ensure!(!query.columns.is_empty(), "Table has no readable columns");
        let start = query.start_column.min(query.columns.len() - 1);
        let projection = query.columns.iter().skip(start).take(limits::SQLITE_COLUMN_WINDOW).map(|column| {
            let name = quote(column);
            format!("CASE WHEN typeof({name})='null' THEN 'NULL' WHEN typeof({name})='blob' THEN 'BLOB ' || hex(substr({name},1,{})) ELSE CAST(substr(CAST({name} AS BLOB),1,{}) AS TEXT) END", limits::SQLITE_BLOB_BYTES, limits::SQLITE_TEXT_BYTES)
        }).collect::<Vec<_>>().join(",");
        let (tail, values) = Self::tail(query);
        let sql = format!(
            "SELECT {projection} FROM {}{tail} LIMIT {} OFFSET {}",
            quote(&query.table),
            limits::SQLITE_PAGE_ROWS + 1,
            query.page.saturating_mul(limits::SQLITE_PAGE_ROWS)
        );
        let mut statement = connection.prepare(&sql)?;
        let count = statement.column_count();
        // Lossy UTF-8 conversion can expand each input byte into three bytes.
        // This covers the temporary row; retained output has its own admission.
        let scratch = count
            * (std::mem::size_of::<std::borrow::Cow<'_, str>>()
                + 3 * limits::SQLITE_TEXT_BYTES.max(5 + 2 * limits::SQLITE_BLOB_BYTES));
        let _conversion_memory = pool.reserve_working(scratch as u64, scratch as u64)?;
        let mut cursor = statement.query(params_from_iter(values.iter()))?;
        let mut rows = crate::resources::TextRowsBuilder::new(pool);
        let mut more = false;
        while let Some(row) = cursor.next()? {
            cancel.check()?;
            if rows.len() == limits::SQLITE_PAGE_ROWS {
                more = true;
                break;
            }
            let values = (0..count)
                .map(|index| match row.get_ref(index) {
                    Ok(rusqlite::types::ValueRef::Text(value)) => preview_text(value),
                    _ => std::borrow::Cow::Borrowed(""),
                })
                .collect::<Vec<_>>();
            rows.try_push(values.iter().map(|value| value.as_ref()))?;
        }
        Ok(Page {
            rows: rows.finish(),
            more,
            start_column: start,
        })
    }
    pub fn value(
        &self,
        query: &Query,
        row: usize,
        column: usize,
        offset: usize,
        cancel: &Cancellation,
    ) -> Result<(String, bool)> {
        let connection = self.connection(cancel, query.seconds)?;
        let name = quote(
            query
                .columns
                .get(column)
                .ok_or_else(|| anyhow!("Unknown column"))?,
        );
        let (tail, values) = Self::tail(query);
        let size = limits::TEXT_PAGE_BYTES as usize;
        let sql = format!(
            "SELECT typeof({name}), substr(CAST({name} AS BLOB),{}, {}) FROM {}{tail} LIMIT 1 OFFSET {}",
            offset + 1,
            size + 1,
            quote(&query.table),
            query
                .page
                .saturating_mul(limits::SQLITE_PAGE_ROWS)
                .saturating_add(row)
        );
        let (kind, mut bytes): (String, Vec<u8>) =
            connection.query_row(&sql, params_from_iter(values.iter()), |row| {
                Ok((
                    row.get(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?.unwrap_or_default(),
                ))
            })?;
        let more = bytes.len() > size;
        bytes.truncate(size);
        Ok((
            if kind == "blob" {
                bytes.iter().map(|byte| format!("{byte:02X} ")).collect()
            } else if kind == "null" {
                "NULL".into()
            } else {
                String::from_utf8_lossy(&bytes).into_owned()
            },
            more,
        ))
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    use crate::resources::ResourcePool;

    #[test]
    fn preview_conversion_matches_lossy_utf8_with_bounded_storage() {
        for value in [
            b"plain".as_slice(),
            "中文".as_bytes(),
            &[0xff, 0xfe, b'a'],
            &[0xf0, 0x9f, 0x92],
            &[0xed, 0xa0, 0x80],
            &[0xe2, b'a', 0x80],
        ] {
            assert_eq!(preview_text(value), String::from_utf8_lossy(value));
        }
    }

    #[test]
    fn a_fixed_query_page_never_publishes_a_partial_page_on_pressure() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let connection = Connection::open(file.path()).unwrap();
        connection.execute_batch("CREATE TABLE items(value TEXT); WITH RECURSIVE rows(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM rows WHERE n<20) INSERT INTO items SELECT 'value ' || n FROM rows;").unwrap();
        drop(connection);
        let cancel = Cancellation::default();
        let database = open(file.path(), &cancel).unwrap();
        let columns = database.columns("items", &cancel).unwrap();
        let query = Query {
            table: "items".into(),
            columns: columns.names,
            order: columns.order,
            ..Query::default()
        };
        let pool = ResourcePool::with_budgets(0, 2048);
        let failed = database.page_with_pool(&query, &cancel, &pool);
        assert!(matches!(failed, Err(error) if error.is::<crate::resources::WorkingPressure>()));
        assert_eq!(pool.snapshot().working_bytes, 0);
        let pool = ResourcePool::with_budgets(0, 64 * 1024);
        let page = database.page_with_pool(&query, &cancel, &pool).unwrap();
        assert_eq!(page.rows.len(), 20);
        assert!(!page.more);
        assert!(pool.snapshot().working_bytes > 0);
        drop(page);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
}
