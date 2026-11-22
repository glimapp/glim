//! Git queries and serialized mutations. Call only on background workers.
mod operations;
mod query;
use crate::{locale::tr, remote::Session, work::Cancellation};
use anyhow::{Context, Result, ensure};
pub use operations::*;
pub use query::*;
use std::{
    ffi::OsString,
    io::{BufRead, BufReader, Read},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

pub const AVAILABLE: bool = cfg!(all(feature = "git", target_os = "macos"));

pub fn enabled() -> bool {
    AVAILABLE
}

pub fn ensure_enabled() -> Result<()> {
    ensure!(AVAILABLE, tr("Git is unavailable in this build"));
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    Working,
    Staged,
    Untracked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: PathBuf,
    pub previous: Option<PathBuf>,
    pub area: Area,
    pub status: char,
}

pub struct Repository {
    pub root: PathBuf,
    pub changes: Vec<Change>,
    pub branches: std::sync::Arc<Vec<String>>,
    pub metadata: Metadata,
}

fn command(root: &Path, remote: Option<&Session>, args: &[OsString]) -> Result<Command> {
    ensure_enabled()?;
    let mut arguments = vec![
        OsString::from("--no-pager"),
        OsString::from("--literal-pathspecs"),
        OsString::from("-c"),
        OsString::from("core.quotePath=true"),
        OsString::from("-c"),
        OsString::from("color.ui=false"),
        OsString::from("-c"),
        OsString::from("core.fsmonitor=false"),
        OsString::from("-c"),
        OsString::from("diff.noprefix=false"),
        OsString::from("-c"),
        OsString::from("diff.mnemonicPrefix=false"),
        OsString::from("-C"),
        root.as_os_str().to_owned(),
    ];
    arguments.extend_from_slice(args);
    match remote {
        None => {
            #[cfg(unix)]
            let mut command = Command::new("/usr/bin/git");
            #[cfg(windows)]
            let mut command = {
                use std::os::windows::process::CommandExt;
                let mut command = Command::new("git.exe");
                command.creation_flags(0x08000000);
                command
            };
            command
                .args(arguments)
                .env("GIT_OPTIONAL_LOCKS", "0")
                .env("LC_ALL", "C")
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_EDITOR", "true")
                .env("GIT_SEQUENCE_EDITOR", "true");
            Ok(command)
        }
        Some(session) => session.git_command(&arguments),
    }
}

fn run(
    command: Command,
    different: bool,
    cancel: &Cancellation,
) -> Result<tempfile::NamedTempFile> {
    run_input(command, different, None, cancel)
}

fn run_input(
    mut command: Command,
    different: bool,
    input: Option<&tempfile::NamedTempFile>,
    cancel: &Cancellation,
) -> Result<tempfile::NamedTempFile> {
    ensure_enabled()?;
    cancel.check()?;
    let output = tempfile::NamedTempFile::new()?;
    let errors = tempfile::NamedTempFile::new()?;
    command
        .stdin(match input {
            Some(input) => Stdio::from(input.reopen()?),
            None => Stdio::null(),
        })
        .stdout(output.reopen()?)
        .stderr(errors.reopen()?);
    let status = crate::work::run(&mut command, cancel).context(tr("Cannot run Git"))?;
    if !status.success() && !(different && status.code() == Some(1)) {
        let mut message = String::new();
        errors
            .reopen()?
            .take(crate::limits::TEXT_PAGE_BYTES)
            .read_to_string(&mut message)?;
        let mut stdout = String::new();
        output
            .reopen()?
            .take(crate::limits::TEXT_PAGE_BYTES)
            .read_to_string(&mut stdout)
            .ok();
        anyhow::bail!(
            "{}: {}\n{}",
            tr("Git command failed"),
            message.trim(),
            stdout.trim()
        );
    }
    cancel.check()?;
    Ok(output)
}

fn relative(bytes: Vec<u8>) -> Result<PathBuf> {
    let path = PathBuf::from(crate::platform::decode_path_bytes(bytes)?);
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "Invalid Git path"
    );
    Ok(path)
}

pub fn changes(root: &Path, remote: Option<&Session>, cancel: &Cancellation) -> Result<Repository> {
    let args = [
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--ignore-submodules=none",
    ]
    .map(OsString::from);
    let output = run(command(root, remote, &args)?, false, cancel)?;
    let mut records = BufReader::new(output.reopen()?).split(0);
    let mut changes = Vec::new();
    while let Some(record) = records.next() {
        cancel.check()?;
        let record = record?;
        ensure!(record.len() >= 4 && record[2] == b' ', "Invalid Git status");
        let path = relative(record[3..].to_vec())?;
        let previous = if record[..2]
            .iter()
            .any(|status| matches!(status, b'R' | b'C'))
        {
            Some(relative(
                records.next().context("Missing Git rename source")??,
            )?)
        } else {
            None
        };
        if &record[..2] == b"??" {
            changes.push(Change {
                path,
                previous,
                area: Area::Untracked,
                status: 'A',
            });
        } else {
            let conflict = record[..2].contains(&b'U') || matches!(&record[..2], b"AA" | b"DD");
            if record[0] != b' ' && !conflict {
                changes.push(Change {
                    path: path.clone(),
                    previous: previous.clone(),
                    area: Area::Staged,
                    status: record[0] as char,
                });
            }
            if record[1] != b' ' || conflict {
                changes.push(Change {
                    path,
                    previous,
                    area: Area::Working,
                    status: if conflict { 'U' } else { record[1] as char },
                });
            }
        }
    }
    let branches = query::strings(
        root,
        remote,
        &["for-each-ref", "--format=%(refname:lstrip=2)", "refs/heads"],
        cancel,
    )?;
    Ok(Repository {
        branches: std::sync::Arc::new(branches.lines().map(String::from).collect()),
        root: root.to_owned(),
        changes,
        metadata: metadata(root, remote, cancel)?,
    })
}

pub fn patch(
    root: &Path,
    change: &Change,
    remote: Option<&Session>,
    cancel: &Cancellation,
) -> Result<tempfile::NamedTempFile> {
    // Status paths remain literal even for names containing Git pathspec syntax.
    relative(change.path.as_os_str().as_encoded_bytes().to_vec())?;
    let mut args: Vec<OsString> = [
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--unified=20",
        "--find-renames",
        "--submodule=short",
    ]
    .map(OsString::from)
    .into();
    match change.area {
        Area::Staged => args.push("--cached".into()),
        Area::Untracked => args.push("--no-index".into()),
        Area::Working if change.status == 'U' => args.push("--ours".into()),
        Area::Working => (),
    }
    args.push("--".into());
    if change.area == Area::Untracked {
        args.push("/dev/null".into());
    } else if let Some(previous) = &change.previous {
        relative(previous.as_os_str().as_encoded_bytes().to_vec())?;
        args.push(previous.as_os_str().to_owned());
    }
    args.push(change.path.as_os_str().to_owned());
    run(
        command(root, remote, &args)?,
        change.area == Area::Untracked,
        cancel,
    )
}
