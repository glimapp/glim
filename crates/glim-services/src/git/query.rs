use super::*;

#[derive(Clone, Debug, Default)]
pub struct Metadata {
    pub head: String,
    pub branch: String,
    pub upstream: String,
    pub ahead: usize,
    pub behind: usize,
    pub operation: Option<String>,
}

pub(super) fn strings(
    root: &Path,
    remote: Option<&Session>,
    args: &[&str],
    cancel: &Cancellation,
) -> Result<String> {
    let output = run(
        command(
            root,
            remote,
            &args.iter().map(OsString::from).collect::<Vec<_>>(),
        )?,
        false,
        cancel,
    )?;
    Ok(
        String::from_utf8_lossy(&crate::resources::read_to_end(output.reopen()?, cancel)?)
            .into_owned(),
    )
}

pub fn commit_identity(
    root: &Path,
    remote: Option<&Session>,
    cancel: &Cancellation,
) -> Result<[String; 2]> {
    let read = |key| {
        strings(
            root,
            remote,
            &["config", "--default", "", "--get", key],
            cancel,
        )
        .map(|v| v.trim_end().to_owned())
    };
    Ok([read("user.name")?, read("user.email")?])
}

pub fn metadata(root: &Path, remote: Option<&Session>, cancel: &Cancellation) -> Result<Metadata> {
    let status = strings(
        root,
        remote,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=no",
        ],
        cancel,
    )?;
    let mut result = Metadata::default();
    for line in status.lines() {
        if let Some(value) = line.strip_prefix("# branch.oid ") {
            result.head = if value == "(initial)" {
                String::new()
            } else {
                value.into()
            };
        }
        if let Some(value) = line.strip_prefix("# branch.head ") {
            result.branch = value.into();
        }
        if let Some(value) = line.strip_prefix("# branch.upstream ") {
            result.upstream = value.into();
        }
        if let Some(value) = line.strip_prefix("# branch.ab ") {
            let mut parts = value.split_whitespace();
            result.ahead = parts
                .next()
                .unwrap_or("")
                .trim_start_matches('+')
                .parse()
                .unwrap_or(0);
            result.behind = parts
                .next()
                .unwrap_or("")
                .trim_start_matches('-')
                .parse()
                .unwrap_or(0);
        }
    }
    for (marker, kind) in [
        ("rebase-merge", "rebase"),
        ("rebase-apply", "rebase"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("REVERT_HEAD", "revert"),
        ("MERGE_HEAD", "merge"),
    ] {
        let output = run(
            command(
                root,
                remote,
                &["rev-parse".into(), "--git-path".into(), marker.into()],
            )?,
            false,
            cancel,
        )?;
        let mut path = crate::resources::read_to_end(output.reopen()?, cancel)?;
        if path.last() == Some(&b'\n') {
            path.pop();
        }
        let path = PathBuf::from(OsString::from_vec(path));
        let path = if path.is_absolute() {
            path
        } else {
            root.join(path)
        };
        let exists = match remote {
            Some(session) => session.git_path_exists(&path, cancel)?,
            None => path.try_exists()?,
        };
        if exists {
            result.operation = Some(kind.into());
            break;
        }
    }
    Ok(result)
}

#[derive(Clone, Debug)]
pub struct Commit {
    pub id: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}

pub const HISTORY_PAGE: usize = 100;
pub struct History {
    pub commits: Vec<Commit>,
    pub more: bool,
}

pub fn history(
    root: &Path,
    remote: Option<&Session>,
    offset: usize,
    limit: usize,
    search: &str,
    path: Option<&Path>,
    cancel: &Cancellation,
) -> Result<History> {
    let mut args: Vec<OsString> = vec![
        "log".into(),
        "--all".into(),
        "--date-order".into(),
        "--date=format:%Y-%m-%d %H:%M:%S".into(),
        "-z".into(),
        "--format=%H%x00%an%x00%ad%x00%s".into(),
        format!("--max-count={}", limit.saturating_add(1)).into(),
        format!("--skip={offset}").into(),
    ];
    if !search.trim().is_empty() {
        args.extend([
            "--fixed-strings".into(),
            "--regexp-ignore-case".into(),
            format!("--grep={}", search.trim()).into(),
        ]);
    }
    args.push("--".into());
    if let Some(path) = path {
        relative(path.as_os_str().as_bytes().to_vec())?;
        args.push(path.as_os_str().to_owned());
    }
    let output = run(command(root, remote, &args)?, false, cancel)?;
    let mut records = BufReader::new(output.reopen()?).split(0);
    let mut commits = Vec::new();
    while let Some(id) = records.next() {
        cancel.check()?;
        let id = String::from_utf8_lossy(&id?).into_owned();
        if id.is_empty() {
            continue;
        }
        let mut next = || -> Result<String> {
            Ok(
                String::from_utf8_lossy(&records.next().context("Incomplete Git log")??)
                    .into_owned(),
            )
        };
        commits.push(Commit {
            id,
            author: next()?,
            date: next()?,
            subject: next()?,
        });
    }
    let more = commits.len() > limit;
    commits.truncate(limit);
    Ok(History { commits, more })
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub name: String,
    pub full: String,
    pub id: String,
    pub upstream: String,
    pub tracking: String,
    pub current: bool,
    pub tag: Option<Box<TagDetails>>,
}
#[derive(Clone, Debug)]
pub struct TagDetails {
    pub author: String,
    pub date: String,
    pub subject: String,
}
#[derive(Clone, Debug)]
pub struct Remote {
    pub name: String,
    pub url: String,
}
#[derive(Clone, Debug)]
pub struct Stash {
    pub name: String,
    pub id: String,
    pub subject: String,
}
#[derive(Clone, Debug)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: String,
    pub branch: String,
    pub locked: bool,
    pub prunable: bool,
}
#[derive(Default)]
pub struct Catalog {
    pub default_branch: Option<String>,
    pub references: Vec<Reference>,
    pub remotes: Vec<Remote>,
    pub stashes: Vec<Stash>,
    pub worktrees: Vec<Worktree>,
}

pub fn catalog(root: &Path, remote: Option<&Session>, cancel: &Cancellation) -> Result<Catalog> {
    let mut result = Catalog::default();
    let format = "--format=%(refname)%00%(refname:short)%00%(objectname)%00%(upstream:short)%00%(upstream:track)%00%(HEAD)%00%(symref)";
    // Tag objects have a tagger and annotation. Lightweight tags on commits
    // expose the commit author and subject through the same ref query.
    let tag_format = format!(
        "{format}%00%(taggername)%(authorname)%00%(taggerdate:format:%Y-%m-%d %H:%M:%S)%(authordate:format:%Y-%m-%d %H:%M:%S)%00%(contents:subject)"
    );
    for args in [
        vec![
            "for-each-ref",
            "--sort=refname",
            "--sort=-committerdate",
            format,
            "refs/heads",
            "refs/remotes",
        ],
        vec![
            "for-each-ref",
            "--sort=-version:refname",
            &tag_format,
            "refs/tags",
        ],
    ] {
        let refs = strings(root, remote, &args, cancel)?;
        for line in refs.lines() {
            let fields: Vec<_> = line.split('\0').collect();
            let [
                full,
                short,
                id,
                upstream,
                tracking,
                head,
                symref,
                details @ ..,
            ] = fields.as_slice()
            else {
                continue;
            };
            if !symref.is_empty() {
                if full.starts_with("refs/remotes/")
                    && full.ends_with("/HEAD")
                    && (result.default_branch.is_none() || *full == "refs/remotes/origin/HEAD")
                {
                    result.default_branch = Some(symref.to_string());
                }
                continue;
            }
            result.references.push(Reference {
                full: full.to_string(),
                name: short.to_string(),
                id: id.to_string(),
                upstream: upstream.to_string(),
                tracking: tracking.to_string(),
                current: *head == "*",
                tag: match details {
                    [author, date, subject] => Some(Box::new(TagDetails {
                        author: (*author).into(),
                        date: (*date).into(),
                        subject: (*subject).into(),
                    })),
                    _ => None,
                },
            });
        }
    }
    let local = |name: &str| {
        result
            .references
            .iter()
            .find(|r| r.full.strip_prefix("refs/heads/") == Some(name))
            .map(|r| r.full.clone())
    };
    result.default_branch = match result.default_branch.take() {
        Some(remote_ref) => remote_ref
            .strip_prefix("refs/remotes/")
            .and_then(|r| r.split_once('/'))
            .and_then(|(_, name)| local(name))
            .or(Some(remote_ref)),
        None => local("main").or_else(|| local("master")).or_else(|| {
            result
                .references
                .iter()
                .find(|r| r.current)
                .map(|r| r.full.clone())
        }),
    };
    for name in strings(root, remote, &["remote"], cancel)?.lines() {
        result.remotes.push(Remote {
            name: name.into(),
            url: strings(root, remote, &["remote", "get-url", name], cancel)?
                .trim_end()
                .into(),
        });
    }
    let stash = strings(
        root,
        remote,
        &["stash", "list", "-z", "--format=%gd%x00%H%x00%gs"],
        cancel,
    )?;
    let fields: Vec<_> = stash.split_terminator('\0').collect();
    for chunk in fields.chunks_exact(3) {
        result.stashes.push(Stash {
            name: chunk[0].into(),
            id: chunk[1].into(),
            subject: chunk[2].into(),
        });
    }
    let output = run(
        command(
            root,
            remote,
            &["worktree", "list", "--porcelain", "-z"].map(OsString::from),
        )?,
        false,
        cancel,
    )?;
    for field in BufReader::new(output.reopen()?).split(0) {
        let field = field?;
        if let Some(path) = field.strip_prefix(b"worktree ") {
            result.worktrees.push(Worktree {
                path: PathBuf::from(OsString::from_vec(path.to_vec())),
                head: String::new(),
                branch: String::new(),
                locked: false,
                prunable: false,
            });
        } else if let Some(tree) = result.worktrees.last_mut() {
            if let Some(value) = field.strip_prefix(b"HEAD ") {
                tree.head = String::from_utf8_lossy(value).into_owned();
            }
            if let Some(value) = field.strip_prefix(b"branch ") {
                tree.branch = String::from_utf8_lossy(value)
                    .trim_start_matches("refs/heads/")
                    .into();
            }
            tree.locked |= field.starts_with(b"locked");
            tree.prunable |= field.starts_with(b"prunable");
        }
    }
    Ok(result)
}

pub(super) fn revision(
    root: &Path,
    remote: Option<&Session>,
    value: &str,
    cancel: &Cancellation,
) -> Result<String> {
    strings(
        root,
        remote,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{value}^{{commit}}"),
        ],
        cancel,
    )
    .map(|s| s.trim().to_owned())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Comparison {
    AllChanges,
    Commit(String),
    Between(String, String),
    Stash(String),
    FileHistory(String, PathBuf),
    Details(String),
    Blame(PathBuf),
}

impl Comparison {
    pub fn title(&self) -> String {
        match self {
            Self::AllChanges => tr("All changes"),
            Self::Commit(id) => format!("{} · {}", tr("Commit"), &id[..id.len().min(12)]),
            Self::Between(base, target) => format!("{base} → {target}"),
            Self::Stash(id) => format!("Stash · {}", &id[..id.len().min(12)]),
            Self::FileHistory(id, path) => {
                format!("{} · {}", path.display(), &id[..id.len().min(12)])
            }
            Self::Details(id) => format!("{} · {}", tr("Commit details"), &id[..id.len().min(12)]),
            Self::Blame(path) => format!("{} · {}", tr("Blame"), path.display()),
        }
    }
    pub fn is_raw(&self) -> bool {
        matches!(self, Self::Details(_) | Self::Blame(_))
    }
}

pub fn comparison_patch(
    root: &Path,
    remote: Option<&Session>,
    comparison: &Comparison,
    cancel: &Cancellation,
) -> Result<tempfile::NamedTempFile> {
    if *comparison == Comparison::AllChanges {
        use std::io::{Read, Write};
        let mut output = tempfile::NamedTempFile::new()?;
        let mut buffer = vec![0; crate::limits::IO_BUFFER_BYTES];
        for change in super::changes(root, remote, cancel)?.changes {
            let patch = super::patch(root, &change, remote, cancel)?;
            let mut input = patch.reopen()?;
            loop {
                cancel.check()?;
                let n = input.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                output.write_all(&buffer[..n])?;
            }
        }
        return Ok(output);
    }
    if let Comparison::Details(id) = comparison {
        return run(
            command(
                root,
                remote,
                &[
                    "show".into(),
                    "--no-patch".into(),
                    "--format=fuller".into(),
                    revision(root, remote, id, cancel)?.into(),
                    "--".into(),
                ],
            )?,
            false,
            cancel,
        );
    }
    if let Comparison::Blame(path) = comparison {
        relative(path.as_os_str().as_bytes().to_vec())?;
        return run(
            command(
                root,
                remote,
                &[
                    "blame".into(),
                    "--date=short".into(),
                    "--".into(),
                    path.as_os_str().to_owned(),
                ],
            )?,
            false,
            cancel,
        );
    }
    let mut args: Vec<OsString> = match comparison {
        Comparison::Commit(id) | Comparison::FileHistory(id, _) => vec![
            "show".into(),
            "--format=fuller".into(),
            "--first-parent".into(),
            revision(root, remote, id, cancel)?.into(),
        ],
        Comparison::Between(left, right) => vec![
            "diff".into(),
            revision(root, remote, left, cancel)?.into(),
            revision(root, remote, right, cancel)?.into(),
        ],
        Comparison::AllChanges | Comparison::Details(_) | Comparison::Blame(_) => unreachable!(),
        Comparison::Stash(id) => vec![
            "stash".into(),
            "show".into(),
            "--include-untracked".into(),
            "-p".into(),
            revision(root, remote, id, cancel)?.into(),
        ],
    };
    args.extend(
        [
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--unified=20",
            "--find-renames",
        ]
        .map(OsString::from),
    );
    args.push("--".into());
    if let Comparison::FileHistory(_, path) = comparison {
        relative(path.as_os_str().as_bytes().to_vec())?;
        args.push(path.as_os_str().to_owned());
    }
    run(command(root, remote, &args)?, false, cancel)
}

pub fn conflict_source(
    root: &Path,
    remote: Option<&Session>,
    path: &Path,
    cancel: &Cancellation,
) -> Result<tempfile::NamedTempFile> {
    ensure_enabled()?;
    relative(path.as_os_str().as_bytes().to_vec())?;
    let source = match remote {
        Some(session) => session.download(&root.join(path), cancel)?,
        None => root.join(path),
    };
    let output = tempfile::NamedTempFile::new()?;
    cancel.copy(std::fs::File::open(source)?, output.reopen()?)?;
    Ok(output)
}
