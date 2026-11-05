//! Change one integration entry without reformatting another application's JSON/JSONC.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::ops::Range;

struct Member {
    key: String,
    start: usize,
    value: Range<usize>,
    comma: Option<usize>,
}

fn trivia(bytes: &[u8], at: &mut usize) {
    loop {
        while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
            *at += 1;
        }
        if bytes.get(*at..*at + 2) == Some(b"//") {
            while bytes.get(*at).is_some_and(|b| *b != b'\n') {
                *at += 1;
            }
        } else if bytes.get(*at..*at + 2) == Some(b"/*") {
            *at += 2;
            while *at < bytes.len() && bytes.get(*at..*at + 2) != Some(b"*/") {
                *at += 1;
            }
            *at = (*at + 2).min(bytes.len());
        } else {
            break;
        }
    }
}

fn quoted(bytes: &[u8], at: &mut usize) {
    let quote = bytes[*at];
    *at += 1;
    while *at < bytes.len() {
        let b = bytes[*at];
        *at += 1;
        if b == quote {
            break;
        }
        if b == b'\\' {
            *at = (*at + 1).min(bytes.len());
        }
    }
}

fn value_end(bytes: &[u8], at: &mut usize) {
    let mut depth = 0usize;
    while *at < bytes.len() {
        match bytes[*at] {
            b'"' | b'\'' => quoted(bytes, at),
            b'/' if matches!(bytes.get(*at + 1), Some(b'/' | b'*')) => {
                if depth == 0 {
                    break;
                }
                trivia(bytes, at);
            }
            b'{' | b'[' => {
                depth += 1;
                *at += 1;
            }
            b'}' | b']' if depth > 0 => {
                depth -= 1;
                *at += 1;
                if depth == 0 {
                    break;
                }
            }
            b',' | b'}' | b']' if depth == 0 => break,
            _ => *at += 1,
        }
    }
}

fn members(source: &str) -> Result<(Vec<Member>, usize)> {
    let bytes = source.as_bytes();
    let mut at = 0;
    trivia(bytes, &mut at);
    ensure!(
        bytes.get(at) == Some(&b'{'),
        "Expected a configuration object"
    );
    at += 1;
    let mut members: Vec<Member> = Vec::new();
    loop {
        trivia(bytes, &mut at);
        if bytes.get(at) == Some(&b'}') {
            return Ok((members, at));
        }
        ensure!(at < bytes.len(), "Incomplete configuration object");
        let start = at;
        if matches!(bytes[at], b'"' | b'\'') {
            quoted(bytes, &mut at);
        } else {
            while bytes
                .get(at)
                .is_some_and(|b| *b != b':' && !b.is_ascii_whitespace())
            {
                at += 1;
            }
        }
        let key_object: Value = json5::from_str(&format!("{{{}:null}}", &source[start..at]))?;
        let key = key_object
            .as_object()
            .and_then(|v| v.keys().next())
            .context("Missing key")?
            .clone();
        ensure!(
            !members.iter().any(|m| m.key == key),
            "Duplicate configuration key"
        );
        trivia(bytes, &mut at);
        ensure!(bytes.get(at) == Some(&b':'), "Missing property separator");
        at += 1;
        trivia(bytes, &mut at);
        let value_start = at;
        value_end(bytes, &mut at);
        let value = value_start..source[..at].trim_end().len();
        trivia(bytes, &mut at);
        let comma = (bytes.get(at) == Some(&b',')).then_some(at);
        if comma.is_some() {
            at += 1;
        } else {
            ensure!(bytes.get(at) == Some(&b'}'), "Missing property separator");
        }
        members.push(Member {
            key,
            start,
            value,
            comma,
        });
    }
}

pub(super) fn edit(source: &str, keys: &[&str], value: Option<&Value>) -> Result<String> {
    let mut expected: Value = json5::from_str(source)?;
    let object = expected
        .as_object_mut()
        .context("Expected a configuration object")?;
    if keys.len() == 1 {
        if let Some(value) = value {
            object.insert(keys[0].to_owned(), value.clone());
        } else {
            object.remove(keys[0]);
        }
    }
    let (members, end) = members(source)?;
    let mut result = source.to_owned();
    if let Some((index, member)) = members.iter().enumerate().find(|(_, m)| m.key == keys[0]) {
        if keys.len() > 1 {
            let replacement = edit(&source[member.value.clone()], &keys[1..], value)?;
            result.replace_range(member.value.clone(), &replacement);
        } else if let Some(value) = value {
            result.replace_range(member.value.clone(), &serde_json::to_string_pretty(value)?);
        } else if let Some(comma) = member.comma {
            result.replace_range(member.start..comma + 1, "");
        } else {
            result.replace_range(member.start..member.value.end, "");
            if let Some(previous) = index.checked_sub(1).and_then(|i| members[i].comma) {
                result.replace_range(previous..previous + 1, "");
            }
        }
    } else if let Some(value) = value {
        let mut nested = value.clone();
        for key in keys[1..].iter().rev() {
            nested = serde_json::json!({*key: nested});
        }
        let entry = format!(
            "\n  {}: {}\n",
            serde_json::to_string(keys[0])?,
            serde_json::to_string_pretty(&nested)?
        );
        result.insert_str(end, &entry);
        if let Some(last) = members.last().filter(|m| m.comma.is_none()) {
            result.insert(last.value.end, ',');
        }
    }
    let actual: Value = json5::from_str(&result)?;
    if keys.len() == 1 {
        ensure!(
            actual == expected,
            "Configuration edit changed unrelated values"
        );
    } else {
        // Each recursive call validates its own object; this level must preserve its siblings.
        let mut actual = actual;
        actual
            .as_object_mut()
            .context("Expected an object")?
            .remove(keys[0]);
        expected
            .as_object_mut()
            .context("Expected an object")?
            .remove(keys[0]);
        ensure!(
            actual == expected,
            "Configuration edit changed unrelated values"
        );
    }
    Ok(result)
}
