use crate::{limits, work::Cancellation};
use anyhow::Result;
use std::{
    collections::VecDeque,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Match {
    pub path: PathBuf,
    pub line: u64,
    pub offset: u64,
    pub snippet: String,
}

#[derive(Default)]
pub struct Page {
    pub matches: Vec<Match>,
    pub more: bool,
    pub warning: Option<String>,
}

pub(crate) struct Collector {
    pub page: Page,
    skip: usize,
    failures: usize,
}
impl Collector {
    pub fn new(offset: usize) -> Self {
        Self {
            page: Page::default(),
            skip: offset,
            failures: 0,
        }
    }
    pub fn push(&mut self, found: Match) -> bool {
        if self.skip > 0 {
            self.skip -= 1;
            return true;
        }
        if self.page.matches.len() == limits::WORKSPACE_SEARCH_RESULTS {
            self.page.more = true;
            return false;
        }
        self.page.matches.push(found);
        true
    }
    pub fn failed(&mut self) {
        self.failures += 1;
    }
    pub fn finish(mut self) -> Page {
        if self.failures > 0 {
            self.page.warning = Some(
                crate::locale::tr("Some files could not be searched ({count}).")
                    .replace("{count}", &self.failures.to_string()),
            );
        }
        self.page
    }
}

pub(crate) fn text_candidate(path: &Path) -> bool {
    use glim_core::formats::FormatSet;
    ![
        FormatSet::Archive,
        FormatSet::ContainerReport,
        FormatSet::Sqlite,
        FormatSet::NativeMedia,
        FormatSet::NativeFont,
        FormatSet::NativeOffice,
        FormatSet::Iwork,
        FormatSet::Pdf,
        FormatSet::Safetensors,
        FormatSet::Image,
        FormatSet::Xmind,
    ]
    .into_iter()
    .any(|format| format.matches(path))
}

pub fn search(
    roots: &[PathBuf],
    query: &str,
    offset: usize,
    cancel: &Cancellation,
) -> Result<Page> {
    let mut output = Collector::new(offset);
    if query.trim().is_empty() {
        return Ok(output.finish());
    }
    let roots = normalized_roots(roots);
    let mut buffer = vec![0; limits::WORKSPACE_CONTENT_BUFFER_BYTES];
    for root in roots {
        for entry in ignore::WalkBuilder::new(root)
            .standard_filters(false)
            .follow_links(false)
            .filter_entry(|entry| entry.file_name() != ".git")
            .sort_by_file_name(|a, b| a.cmp(b))
            .build()
        {
            cancel.check()?;
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    output.failed();
                    continue;
                }
            };
            if !entry.file_type().is_some_and(|kind| kind.is_file())
                || !text_candidate(entry.path())
            {
                continue;
            }
            let result = (|| -> Result<()> {
                let mut file = File::open(entry.path())?;
                let mut scanner = Scanner::new(entry.path(), query.trim());
                loop {
                    cancel.check()?;
                    let count = file.read(&mut buffer)?;
                    if count == 0 {
                        scanner.finish(&mut output);
                        break;
                    }
                    if !scanner.feed(&buffer[..count], &mut output) {
                        break;
                    }
                }
                Ok(())
            })();
            if result.is_err() {
                cancel.check()?;
                output.failed();
            }
            if output.page.more {
                return Ok(output.finish());
            }
        }
    }
    Ok(output.finish())
}

pub(crate) fn normalized_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
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

// KMP keeps matching across read boundaries without buffering an entire line.
pub(crate) struct Scanner {
    path: PathBuf,
    pattern: Vec<u8>,
    failure: Vec<usize>,
    matched: usize,
    position: u64,
    line: u64,
    line_matched: bool,
    first: bool,
    context: VecDeque<u8>,
    pending: Option<(u64, Vec<u8>)>,
}
impl Scanner {
    pub fn new(path: &Path, query: &str) -> Self {
        let pattern = query.as_bytes().to_vec();
        let mut failure = vec![0; pattern.len()];
        for i in 1..pattern.len() {
            let mut prefix = failure[i - 1];
            while prefix > 0 && pattern[i] != pattern[prefix] {
                prefix = failure[prefix - 1];
            }
            if pattern[i] == pattern[prefix] {
                prefix += 1;
            }
            failure[i] = prefix;
        }
        Self {
            path: path.to_path_buf(),
            pattern,
            failure,
            matched: 0,
            position: 0,
            line: 1,
            line_matched: false,
            first: true,
            context: VecDeque::new(),
            pending: None,
        }
    }
    pub fn feed(&mut self, bytes: &[u8], output: &mut Collector) -> bool {
        if self.pattern.is_empty() {
            return false;
        }
        if self.first {
            self.first = false;
            if bytes.contains(&0)
                || std::str::from_utf8(bytes).is_err_and(|error| error.error_len().is_some())
            {
                return false;
            }
        }
        for &byte in bytes {
            if byte == 0 {
                return false;
            }
            if byte == b'\n' {
                if !self.finish(output) {
                    return false;
                }
                self.line += 1;
                self.matched = 0;
                self.line_matched = false;
                self.context.clear();
            } else if let Some((_, snippet)) = &mut self.pending {
                snippet.push(byte);
                if snippet.len() >= limits::WORKSPACE_CONTENT_SNIPPET_BYTES && !self.finish(output)
                {
                    return false;
                }
            } else if !self.line_matched {
                self.context.push_back(byte);
                while self.context.len() > limits::WORKSPACE_CONTENT_SNIPPET_BYTES / 2 {
                    self.context.pop_front();
                }
                while self.matched > 0 && byte != self.pattern[self.matched] {
                    self.matched = self.failure[self.matched - 1];
                }
                if byte == self.pattern[self.matched] {
                    self.matched += 1;
                }
                if self.matched == self.pattern.len() {
                    self.line_matched = true;
                    self.pending = Some((
                        self.position + 1 - self.pattern.len() as u64,
                        self.context.iter().copied().collect(),
                    ));
                }
            }
            self.position += 1;
        }
        true
    }
    pub fn finish(&mut self, output: &mut Collector) -> bool {
        if let Some((offset, snippet)) = self.pending.take() {
            output.push(Match {
                path: self.path.clone(),
                line: self.line,
                offset,
                snippet: preview(&snippet),
            })
        } else {
            true
        }
    }
}

pub(crate) fn preview(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}
