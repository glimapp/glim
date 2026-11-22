//! Read-only resource routes consumed by the bundled offline API reference.
use crate::{limits, work::Cancellation};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, Params, params};
use serde_json::{Map, Value, json};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const NODE: &str = "SELECT n.id,n.parent,n.name,n.name_complete,n.kind,n.value,n.value_complete,n.length,
    COALESCE((SELECT value FROM nodes WHERE parent=n.id AND name='name' LIMIT 1),'') AS title,
    COALESCE((SELECT value FROM nodes WHERE parent=n.id AND name='type' LIMIT 1),'') AS type,
    COALESCE((SELECT value FROM nodes WHERE parent=n.id AND name='in' LIMIT 1),'') AS location,
    COALESCE((SELECT value FROM nodes WHERE parent=n.id AND name='required' AND kind='scalar' LIMIT 1),'') AS required
    FROM nodes n";

fn rows(db: &Connection, sql: &str, bindings: impl Params) -> Result<Vec<Value>> {
    let mut statement = db.prepare(sql)?;
    let result = statement.query_map(bindings, |row| {
        let mut object = Map::new();
        for (index, name) in row.as_ref().column_names().iter().enumerate() {
            let value = match row.get_ref(index)? {
                rusqlite::types::ValueRef::Integer(number) => Value::from(number),
                _ => Value::String(row.get(index)?),
            };
            object.insert((*name).to_owned(), value);
        }
        Ok(Value::Object(object))
    })?;
    Ok(result.collect::<rusqlite::Result<_>>()?)
}

fn page(mut items: Vec<Value>) -> Value {
    let more = items.len() > limits::OPENAPI_PAGE_NODES;
    items.truncate(limits::OPENAPI_PAGE_NODES);
    json!({ "items": items, "more": more })
}

fn raw(db: &Connection, database: &Path, id: i64, offset: i64) -> Result<Value> {
    let (start, length): (u64, u64) =
        db.query_row("SELECT start,length FROM nodes WHERE id=?1", [id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
    let mut offset = (offset as u64).min(length);
    let mut file = File::open(database.with_file_name("source.json"))?;
    file.seek(SeekFrom::Start(
        start.checked_add(offset).context("Invalid source offset")?,
    ))?;
    let mut data = vec![0; (length - offset).min(limits::OPENAPI_FIELD_BYTES as u64) as usize];
    file.read_exact(&mut data)?;
    let leading = data.iter().take_while(|byte| **byte & 0xc0 == 0x80).count();
    offset += leading as u64;
    let data = &data[leading..];
    let text = match std::str::from_utf8(data) {
        Ok(text) => text,
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&data[..e.valid_up_to()])?,
        Err(e) => return Err(e.into()),
    };
    let next = offset + text.len() as u64;
    Ok(json!({ "text": text, "offset": offset, "next": next, "more": next < length }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaped_pointer_and_raw_pages_preserve_unicode() -> Result<()> {
        let description = "潮位🌊".repeat(limits::OPENAPI_FIELD_BYTES);
        let spec = json!({ "openapi": "3.1.0", "paths": {}, "a/b~c": description }).to_string();
        let cancel = Cancellation::default();
        let index = crate::openapi::prepare(Path::new("stations.json"), Some(&spec), &cancel)?;
        let mut url = url::Url::parse("glim-api://document/pointer")?;
        url.query_pairs_mut().append_pair("value", "/a~1b~0c");
        let node = resource(&index.database, &url, &cancel)?;
        let id = node["id"].as_i64().context("Expected a field")?.to_string();
        url.set_path("/raw");
        let mut offset = 0;
        let mut raw = String::new();
        loop {
            url.query_pairs_mut()
                .clear()
                .append_pair("id", &id)
                .append_pair("offset", &offset.to_string());
            let page = resource(&index.database, &url, &cancel)?;
            raw.push_str(page["text"].as_str().context("Missing raw text")?);
            let next = page["next"].as_u64().context("Missing page offset")?;
            assert!(next > offset);
            if page["more"] == false {
                break;
            }
            offset = next;
        }
        assert_eq!(serde_json::from_str::<String>(&raw)?, description);
        Ok(())
    }
}

pub fn resource(database: &Path, url: &url::Url, cancel: &Cancellation) -> Result<Value> {
    cancel.check()?;
    let bytes = (limits::OPENAPI_FIELD_BYTES * limits::OPENAPI_PAGE_NODES * 16
        + limits::SQLITE_CACHE_KIB as usize * 1024) as u64;
    let _memory = crate::resources::pool().reserve_working(bytes, bytes)?;
    let db = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_PRIVATE_CACHE,
    )?;
    let token = cancel.clone();
    db.progress_handler(1000, Some(move || token.check().is_err()));
    db.pragma_update(None, "cache_size", -limits::SQLITE_CACHE_KIB)?;
    db.pragma_update(None, "temp_store", "FILE")?;
    let arg = |key: &str| {
        url.query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    let number = |key| arg(key).parse::<i64>().unwrap_or_default().max(0);
    let id = number("id");
    let offset = number("offset");
    let take = limits::OPENAPI_PAGE_NODES as i64 + 1;
    let one = |items: Vec<Value>| items.into_iter().next().unwrap_or_else(|| json!({}));
    let node = |id: i64| Ok(one(rows(&db, &format!("{NODE} WHERE n.id=?1"), [id])?));
    match url.path() {
        "/endpoints" => Ok(page(rows(
            &db,
            "SELECT id,path,method,summary,tag FROM endpoints ORDER BY tag,path,method,id LIMIT ?1 OFFSET ?2",
            [take, offset],
        )?)),
        "/children" => Ok(page(rows(
            &db,
            &format!("{NODE} WHERE n.parent=?1 ORDER BY n.id LIMIT ?2 OFFSET ?3"),
            [id, take, offset],
        )?)),
        "/node" => node(id),
        "/child" => Ok(one(rows(
            &db,
            &format!("{NODE} WHERE n.parent=?1 AND n.name=?2 ORDER BY n.id DESC LIMIT 1"),
            params![id, arg("name")],
        )?)),
        "/pointer" => {
            let pointer = arg("value");
            let mut parent: i64 = 1;
            if !pointer.is_empty() {
                ensure!(pointer.starts_with('/'), "Invalid JSON pointer");
                let mut lookup = db.prepare("SELECT id FROM nodes WHERE parent=?1 AND name=?2 AND name_complete=1 ORDER BY id DESC LIMIT 1")?;
                for part in pointer[1..].split('/') {
                    cancel.check()?;
                    let name = part.replace("~1", "/").replace("~0", "~");
                    parent = lookup.query_row(params![parent, name], |row| row.get(0))?;
                }
            }
            node(parent)
        }
        "/raw" => raw(&db, database, id, offset),
        _ => anyhow::bail!("Unknown API reference resource"),
    }
}
