//! Streaming JSON index for the offline API reference. Values stay in a private
//! snapshot; only bounded labels and child windows cross into the WebView.
use crate::{limits, work::Cancellation};
use anyhow::{Result, ensure};
use rusqlite::{Connection, params};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};

pub struct Index {
    pub database: PathBuf,
    _directory: tempfile::TempDir,
}

struct Scanner<R> {
    input: R,
    offset: u64,
    cancel: Cancellation,
    complete_string: bool,
}
impl<R: BufRead> Scanner<R> {
    fn peek(&mut self) -> Result<Option<u8>> {
        if self.offset.is_multiple_of(limits::IO_BUFFER_BYTES as u64) {
            self.cancel.check()?;
        }
        Ok(self.input.fill_buf()?.first().copied())
    }
    fn next(&mut self) -> Result<Option<u8>> {
        let byte = self.peek()?;
        if byte.is_some() {
            self.input.consume(1);
            self.offset += 1;
        }
        Ok(byte)
    }
    fn whitespace(&mut self) -> Result<()> {
        while self.peek()?.is_some_and(|b| b.is_ascii_whitespace()) {
            self.next()?;
        }
        Ok(())
    }
    fn expect(&mut self, byte: u8) -> Result<()> {
        self.whitespace()?;
        ensure!(
            self.next()? == Some(byte),
            "Invalid JSON at byte {}",
            self.offset
        );
        Ok(())
    }
    fn string(&mut self) -> Result<String> {
        self.expect(b'"')?;
        let start = self.offset;
        let mut prefix = Vec::with_capacity(limits::OPENAPI_FIELD_BYTES + 1);
        prefix.push(b'"');
        let mut escaped = false;
        let mut continuation = 0_u8;
        let mut minimum = 0x80;
        let mut maximum = 0xbf;
        loop {
            let byte = self
                .next()?
                .ok_or_else(|| anyhow::anyhow!("Unterminated JSON string"))?;
            if !escaped && byte == b'"' {
                ensure!(continuation == 0, "Invalid UTF-8 in JSON string");
                break;
            }
            if continuation > 0 {
                ensure!(
                    (minimum..=maximum).contains(&byte),
                    "Invalid UTF-8 in JSON string"
                );
                continuation -= 1;
                minimum = 0x80;
                maximum = 0xbf;
            } else if byte >= 0x80 {
                match byte {
                    0xc2..=0xdf => continuation = 1,
                    0xe0..=0xef => {
                        continuation = 2;
                        if byte == 0xe0 {
                            minimum = 0xa0;
                        }
                        if byte == 0xed {
                            maximum = 0x9f;
                        }
                    }
                    0xf0..=0xf4 => {
                        continuation = 3;
                        if byte == 0xf0 {
                            minimum = 0x90;
                        }
                        if byte == 0xf4 {
                            maximum = 0x8f;
                        }
                    }
                    _ => anyhow::bail!("Invalid UTF-8 in JSON string"),
                }
            }
            if prefix.len() < limits::OPENAPI_FIELD_BYTES {
                prefix.push(byte);
            }
            escaped = !escaped && byte == b'\\';
        }
        self.complete_string = self.offset - start - 1 == (prefix.len() - 1) as u64;
        // A display prefix can end inside UTF-8, an escape or a surrogate pair.
        // Trim only that incomplete tail; the snapshot retains the entire value.
        loop {
            prefix.push(b'"');
            let decoded = serde_json::from_slice::<String>(&prefix);
            prefix.pop();
            if let Ok(decoded) = decoded {
                return Ok(decoded);
            }
            ensure!(prefix.len() > 1, "Invalid JSON string");
            prefix.pop();
        }
    }
    fn skip(&mut self) -> Result<()> {
        self.whitespace()?;
        let mut nesting = 0_u64;
        loop {
            match self.peek()? {
                Some(b'"') => {
                    self.string()?;
                }
                Some(b'{' | b'[') => {
                    nesting += 1;
                    self.next()?;
                }
                Some(b'}' | b']') if nesting > 0 => {
                    nesting -= 1;
                    self.next()?;
                    if nesting == 0 {
                        return Ok(());
                    }
                }
                Some(b',' | b'}' | b']') if nesting == 0 => return Ok(()),
                Some(_) => {
                    self.next()?;
                }
                None => return Ok(()),
            }
            if nesting == 0 {
                match self.peek()? {
                    None | Some(b',' | b'}' | b']') => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

pub fn supports(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
}

pub fn is_spec(path: &Path, cancel: &Cancellation) -> Result<bool> {
    let mut scan = Scanner {
        input: BufReader::with_capacity(limits::IO_BUFFER_BYTES, File::open(path)?),
        offset: 0,
        cancel: cancel.clone(),
        complete_string: true,
    };
    scan.whitespace()?;
    if scan.next()? != Some(b'{') {
        return Ok(false);
    }
    loop {
        scan.whitespace()?;
        if scan.peek()? != Some(b'"') {
            return Ok(false);
        }
        let key = scan.string()?;
        scan.expect(b':')?;
        scan.whitespace()?;
        if matches!(key.as_str(), "openapi" | "swagger") && scan.peek()? == Some(b'"') {
            let version = scan.string()?;
            if (key == "openapi" && version.starts_with("3."))
                || (key == "swagger" && version == "2.0")
            {
                return Ok(true);
            }
        } else {
            scan.skip()?;
        }
        scan.whitespace()?;
        if scan.next()? != Some(b',') {
            return Ok(false);
        }
    }
}

struct Builder<R> {
    scan: Scanner<R>,
    connection: Connection,
}
impl<R: BufRead> Builder<R> {
    fn value(&mut self, parent: i64, name: &str, name_complete: bool, depth: usize) -> Result<()> {
        ensure!(
            depth <= limits::OPENAPI_DEPTH,
            "JSON nesting exceeds the active parser budget; use Source"
        );
        self.scan.whitespace()?;
        let start = self.scan.offset;
        let byte = self
            .scan
            .peek()?
            .ok_or_else(|| anyhow::anyhow!("Missing JSON value"))?;
        let kind = match byte {
            b'{' => "object",
            b'[' => "array",
            b'"' => "string",
            _ => "scalar",
        };
        self.connection.execute(
            "INSERT INTO nodes(parent,name,name_complete,kind,start) VALUES (?1,?2,?3,?4,?5)",
            params![parent, name, name_complete, kind, start],
        )?;
        let id = self.connection.last_insert_rowid();
        let mut value = String::new();
        let mut value_complete = true;
        if matches!(byte, b'{' | b'[') {
            self.scan.next()?;
            let end = if byte == b'{' { b'}' } else { b']' };
            let mut index = 0_u64;
            self.scan.whitespace()?;
            if self.scan.peek()? != Some(end) {
                loop {
                    let (key, complete) = if byte == b'{' {
                        let key = self.scan.string()?;
                        let complete = self.scan.complete_string;
                        self.scan.expect(b':')?;
                        (key, complete)
                    } else {
                        (index.to_string(), true)
                    };
                    self.value(id, &key, complete, depth + 1)?;
                    index += 1;
                    self.scan.whitespace()?;
                    if self.scan.peek()? != Some(b',') {
                        break;
                    }
                    self.scan.next()?;
                }
            }
            self.scan.expect(end)?;
        } else if byte == b'"' {
            value = self.scan.string()?;
            value_complete = self.scan.complete_string;
        } else {
            while let Some(byte) = self.scan.peek()? {
                if byte.is_ascii_whitespace() || matches!(byte, b',' | b'}' | b']') {
                    break;
                }
                if value.len() < limits::OPENAPI_FIELD_BYTES {
                    value.push(char::from(byte));
                }
                self.scan.next()?;
            }
            ensure!(self.scan.offset > start, "Invalid JSON value");
            value_complete = self.scan.offset - start == value.len() as u64;
        }
        self.connection.execute(
            "UPDATE nodes SET value=?1,length=?2,value_complete=?3 WHERE id=?4",
            params![value, self.scan.offset - start, value_complete, id],
        )?;
        Ok(())
    }
}

pub fn prepare(path: &Path, text: Option<&str>, cancel: &Cancellation) -> Result<Index> {
    let bytes = (limits::OPENAPI_DEPTH * limits::OPENAPI_FIELD_BYTES * 2
        + limits::IO_BUFFER_BYTES * 2
        + limits::SQLITE_CACHE_KIB as usize * 1024) as u64;
    let _memory = crate::resources::pool().reserve_working(bytes, bytes)?;
    let directory = tempfile::tempdir()?;
    let snapshot = directory.path().join("source.json");
    let mut output = File::create(&snapshot)?;
    match text {
        Some(text) => {
            cancel.copy(text.as_bytes(), &mut output)?;
        }
        None => {
            let input = File::open(path)?;
            let before = input.metadata()?;
            let copied = cancel.copy((&input).take(before.len()), &mut output)?;
            let after = input.metadata()?;
            ensure!(
                copied == before.len()
                    && before.len() == after.len()
                    && before.modified()? == after.modified()?,
                "Document changed while preparing API reference; retry"
            );
        }
    }
    output.flush()?;
    ensure!(
        is_spec(&snapshot, cancel)?,
        "The document no longer declares OpenAPI 3.x or Swagger 2.0; use Source"
    );
    let database = directory.path().join("index.sqlite");
    let connection = Connection::open(&database)?;
    let token = cancel.clone();
    connection.progress_handler(1000, Some(move || token.check().is_err()));
    connection.execute_batch(&format!(
        "PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA cache_size=-{}; PRAGMA temp_store=FILE;
         CREATE TABLE nodes(id INTEGER PRIMARY KEY,parent INTEGER NOT NULL,name TEXT NOT NULL,name_complete INTEGER NOT NULL,kind TEXT NOT NULL,value TEXT NOT NULL DEFAULT '',value_complete INTEGER NOT NULL DEFAULT 1,start INTEGER NOT NULL,length INTEGER NOT NULL DEFAULT 0);
         CREATE INDEX children ON nodes(parent,name,id);
         BEGIN;", limits::SQLITE_CACHE_KIB))?;
    let mut builder = Builder {
        scan: Scanner {
            input: BufReader::with_capacity(limits::IO_BUFFER_BYTES, File::open(&snapshot)?),
            offset: 0,
            cancel: cancel.clone(),
            complete_string: true,
        },
        connection,
    };
    builder.value(0, "", true, 0)?;
    // The indexed nesting has already passed the shared stack budget, so
    // IgnoredAny's iterative syntax validator cannot grow an unbounded stack.
    let input = crate::work::Reader {
        inner: File::open(&snapshot)?,
        cancel: cancel.clone(),
    };
    let mut deserializer = serde_json::Deserializer::from_reader(BufReader::with_capacity(
        limits::IO_BUFFER_BYTES,
        input,
    ));
    serde::de::IgnoredAny::deserialize(&mut deserializer)?;
    deserializer.end()?;
    builder.connection.execute_batch(
        "CREATE TABLE endpoints AS
         SELECT operation.id AS id,path.name AS path,operation.name AS method,
         COALESCE((SELECT value FROM nodes WHERE parent=operation.id AND name='summary' LIMIT 1),'') AS summary,
         COALESCE((SELECT item.value FROM nodes tags JOIN nodes item ON item.parent=tags.id AND item.name='0' WHERE tags.parent=operation.id AND tags.name='tags' LIMIT 1),'') AS tag
         FROM nodes root JOIN nodes paths ON paths.parent=root.id AND paths.name='paths'
         JOIN nodes path ON path.parent=paths.id JOIN nodes operation ON operation.parent=path.id
         WHERE root.parent=0 AND operation.kind='object' AND operation.name IN ('get','put','post','delete','options','head','patch','trace');
         CREATE INDEX endpoint_order ON endpoints(tag,path,method,id); COMMIT;")?;
    cancel.check()?;
    Ok(Index {
        database,
        _directory: directory,
    })
}

use serde::Deserialize;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom};

    #[test]
    fn detection_uses_only_root_markers_and_skips_large_values() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("unrelated-name.json");
        for (text, expected) in [
            (r#"{"nested":{"openapi":"3.1.0"}}"#.to_owned(), false),
            (r#"{"openapi":"2.0","swagger":"2.0"}"#.to_owned(), true),
            (
                format!(
                    r#"{{"description":{},"openapi":"3.1.0","paths":{{}}}}"#,
                    serde_json::to_string(
                        &"escaped \\\" } [ 中文".repeat(limits::OPENAPI_FIELD_BYTES)
                    )?
                ),
                true,
            ),
        ] {
            std::fs::write(&path, text)?;
            assert_eq!(is_spec(&path, &Cancellation::default())?, expected);
        }
        Ok(())
    }

    #[test]
    fn index_keeps_complete_unicode_values_and_source_offsets() -> Result<()> {
        let long = "中文 \\\"".repeat(limits::OPENAPI_FIELD_BYTES);
        let text = serde_json::json!({
            "openapi": "3.1.0", "info": {"title": "接口", "version": "1"},
            "paths": {"/things/{id}": {"parameters": [{"name":"id","in":"path","required":true}],
                "get": {"tags":["事物"],"summary":"读取", "responses":{"200":{"description":long,"content":{"application/json":{"schema":{"$ref":"#/components/schemas/A~1B"}}}}}}}},
            "components":{"schemas":{"A/B":{"type":"object","properties":{"self":{"$ref":"#/components/schemas/A~1B"}}}}}
        }).to_string();
        let index = prepare(
            Path::new("/missing/source.json"),
            Some(&text),
            &Cancellation::default(),
        )?;
        let connection = Connection::open(&index.database)?;
        let (path, method, summary, tag): (String, String, String, String) =
            connection.query_row("SELECT path,method,summary,tag FROM endpoints", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?;
        assert_eq!(
            (
                path.as_str(),
                method.as_str(),
                summary.as_str(),
                tag.as_str()
            ),
            ("/things/{id}", "get", "读取", "事物")
        );
        let (start, length, prefix): (u64, u64, String) = connection.query_row(
            "SELECT start,length,value FROM nodes WHERE name='description'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        assert!(prefix.len() < long.len());
        let mut source = File::open(index.database.with_file_name("source.json"))?;
        source.seek(SeekFrom::Start(start))?;
        let mut raw = String::new();
        source.take(length).read_to_string(&mut raw)?;
        assert_eq!(serde_json::from_str::<String>(&raw)?, long);
        drop(connection);
        let directory = index.database.parent().unwrap().to_path_buf();
        drop(index);
        assert!(!directory.exists());
        Ok(())
    }

    #[test]
    fn endpoint_windows_cover_the_complete_catalog() -> Result<()> {
        let paths: serde_json::Map<String, serde_json::Value> = (0..limits::OPENAPI_PAGE_NODES + 1)
            .map(|id| {
                (
                    format!("/item/{id}"),
                    serde_json::json!({"get":{"responses":{}}}),
                )
            })
            .collect();
        let text =
            serde_json::json!({"swagger":"2.0","info":{"title":"API","version":"1"},"paths":paths})
                .to_string();
        let index = prepare(
            Path::new("/unused.json"),
            Some(&text),
            &Cancellation::default(),
        )?;
        let connection = Connection::open(&index.database)?;
        let count: usize =
            connection.query_row("SELECT count(*) FROM endpoints", [], |r| r.get(0))?;
        assert_eq!(count, limits::OPENAPI_PAGE_NODES + 1);
        let remaining: usize = connection.query_row("SELECT count(*) FROM (SELECT id FROM endpoints ORDER BY tag,path,method,id LIMIT ?1 OFFSET ?1)", [limits::OPENAPI_PAGE_NODES], |r| r.get(0))?;
        assert_eq!(remaining, 1);
        Ok(())
    }

    #[test]
    fn invalid_json_and_cancelled_preparation_never_publish_an_index() {
        assert!(
            prepare(
                Path::new("/unused.json"),
                Some(r#"{"openapi":"3.1.0","paths":{}} trailing"#),
                &Cancellation::default()
            )
            .is_err()
        );
        let (cancel, owner) = Cancellation::task();
        drop(owner);
        assert!(
            prepare(
                Path::new("/unused.json"),
                Some(r#"{"openapi":"3.1.0"}"#),
                &cancel
            )
            .is_err_and(|error| error.is::<crate::work::Cancelled>())
        );
    }
}
