use super::{Session, search_index::ScanProcess, sftp::Sftp};
use crate::{
    content_search::{self, Collector, Match, Page, Scanner},
    limits,
    work::Cancellation,
};
use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsString,
    io::{BufRead, BufReader},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    process::Stdio,
};

impl Session {
    pub fn search_content(
        &self,
        roots: &[PathBuf],
        query: &str,
        offset: usize,
        cancel: &Cancellation,
    ) -> Result<Page> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Page::default());
        }
        let roots = content_search::normalized_roots(roots);
        if let Some(page) = shell_search(self, &roots, query, offset, cancel)? {
            return Ok(page);
        }
        // A restricted or password-only SSH shell can still use the existing SFTP protocol.
        let mut transport = Sftp::connect(&self.connection, cancel)?;
        let mut output = Collector::new(offset);
        let mut file_offset = 0;
        let mut partial = false;
        loop {
            cancel.check()?;
            let Some(snapshot) = self.search(&roots, "", file_offset, None, cancel)? else {
                partial = true;
                break;
            };
            partial |= !snapshot.complete || snapshot.warning.is_some();
            for path in &snapshot.page.paths {
                if !content_search::text_candidate(path) {
                    continue;
                }
                let mut scanner = Scanner::new(path, query);
                let result = transport
                    .read_chunks(path, cancel, |bytes| Ok(scanner.feed(bytes, &mut output)));
                match result {
                    Ok(true) => {
                        scanner.finish(&mut output);
                    }
                    Ok(false) => {}
                    Err(_) => {
                        cancel.check()?;
                        output.failed();
                    }
                }
                if output.page.more {
                    break;
                }
                if !transport.healthy() {
                    partial = true;
                    break;
                }
            }
            if output.page.more || !snapshot.page.more || !transport.healthy() {
                break;
            }
            file_offset += snapshot.page.paths.len();
        }
        let mut page = output.finish();
        if partial {
            page.warning = Some(crate::locale::tr(
                "Content search used an incomplete file index. Retry after indexing finishes.",
            ));
        }
        Ok(page)
    }
}

fn quote(value: &[u8], output: &mut Vec<u8>) {
    output.push(b'\'');
    for &byte in value {
        if byte == b'\'' {
            output.extend_from_slice(b"'\"'\"'");
        } else {
            output.push(byte);
        }
    }
    output.push(b'\'');
}

fn shell_search(
    session: &Session,
    roots: &[PathBuf],
    query: &str,
    offset: usize,
    cancel: &Cancellation,
) -> Result<Option<Page>> {
    let mut script = b"command -v rg >/dev/null 2>&1 || exit 127; printf 'GLIM_CONTENT_V1\\000'; exec rg --no-config --fixed-strings --case-sensitive --hidden --no-ignore --no-follow --no-messages --line-number --byte-offset --with-filename --null --no-heading --color never --line-buffered --sort path --encoding none --glob '!.git' --glob '!**/.git/**' --max-columns ".to_vec();
    script.extend_from_slice(
        limits::WORKSPACE_CONTENT_SNIPPET_BYTES
            .to_string()
            .as_bytes(),
    );
    script.extend_from_slice(b" --max-columns-preview -e ");
    quote(query.as_bytes(), &mut script);
    script.extend_from_slice(b" --");
    for root in roots {
        ensure!(root.is_absolute(), "Invalid remote content search root");
        script.push(b' ');
        quote(root.as_os_str().as_bytes(), &mut script);
    }
    if roots.is_empty() {
        return Ok(Some(Page::default()));
    }
    let mut command = session.connection.ssh_command()?;
    let mut child = command
        .args(["-o", "BatchMode=yes"])
        .arg(&session.connection.host)
        .arg(OsString::from_vec(script))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Cannot start remote content search")?;
    let stdout = child
        .stdout
        .take()
        .context("Missing content search output")?;
    let process = ScanProcess::new_with_timeout(child, cancel.clone(), false);
    let mut reader = BufReader::with_capacity(limits::WORKSPACE_CONTENT_BUFFER_BYTES, stdout);
    let header = field(&mut reader, 0, limits::REMOTE_SEARCH_RECORD_BYTES, false)?;
    if header.is_none() {
        let status = process.wait(cancel)?;
        if matches!(status.code(), Some(127 | 255)) {
            return Ok(None);
        }
        anyhow::bail!("Remote content search could not start ({status})");
    }
    ensure!(
        header.as_deref() == Some(b"GLIM_CONTENT_V1"),
        "Invalid remote content search output"
    );
    let mut output = Collector::new(offset);
    loop {
        cancel.check()?;
        let Some(path) = field(&mut reader, 0, limits::REMOTE_SEARCH_RECORD_BYTES, false)? else {
            break;
        };
        let path = PathBuf::from(OsString::from_vec(path));
        ensure!(
            roots.iter().any(|root| path.starts_with(root))
                && !path
                    .components()
                    .any(|p| matches!(p, std::path::Component::ParentDir)),
            "Invalid content match path"
        );
        let line = number(&mut reader)?;
        let byte_offset = number(&mut reader)?;
        let preview = field(
            &mut reader,
            b'\n',
            limits::WORKSPACE_CONTENT_SNIPPET_BYTES,
            true,
        )?
        .context("Missing match text")?;
        if content_search::text_candidate(&path)
            && !output.push(Match {
                path,
                line,
                offset: byte_offset,
                snippet: content_search::preview(&preview),
            })
        {
            return Ok(Some(output.finish()));
        }
    }
    let status = process.wait(cancel)?;
    let mut page = output.finish();
    if !matches!(status.code(), Some(0 | 1)) {
        page.warning = Some(crate::locale::tr(
            "Some remote files could not be searched. Check permissions and retry.",
        ));
    }
    Ok(Some(page))
}

fn number(reader: &mut impl BufRead) -> Result<u64> {
    let bytes = field(reader, b':', limits::REMOTE_SEARCH_RECORD_BYTES, false)?
        .context("Missing match position")?;
    Ok(std::str::from_utf8(&bytes)?.parse()?)
}

fn field(
    reader: &mut impl BufRead,
    delimiter: u8,
    limit: usize,
    truncate: bool,
) -> Result<Option<Vec<u8>>> {
    let mut result = Vec::new();
    let mut seen = false;
    loop {
        let bytes = reader.fill_buf()?;
        if bytes.is_empty() {
            ensure!(!seen, "Incomplete content search output");
            return Ok(None);
        }
        seen = true;
        let end = bytes.iter().position(|byte| *byte == delimiter);
        let count = end.unwrap_or(bytes.len());
        let retained = count.min(limit.saturating_sub(result.len()));
        ensure!(
            truncate || retained == count,
            "Invalid content search record"
        );
        result.extend_from_slice(&bytes[..retained]);
        reader.consume(count + usize::from(end.is_some()));
        if end.is_some() {
            return Ok(Some(result));
        }
    }
}
