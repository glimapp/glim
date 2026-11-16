use super::*;
use std::{io::Write, sync::Arc};

#[derive(Clone, Debug)]
pub enum Operation {
    Init(PathBuf),
    Identity {
        name: String,
        email: String,
    },
    Submodules,
    Clone {
        url: String,
        path: PathBuf,
    },
    Ignore(PathBuf),
    Stage(Option<Change>),
    Unstage(Option<Change>),
    Discard(Change),
    Commit {
        message: String,
        amend: bool,
    },
    CreateBranch(String),
    Switch(String),
    RenameBranch(String, String),
    DeleteBranch(String),
    Tag(String, String),
    DeleteTag(String),
    Merge(String),
    Rebase(String),
    CherryPick(String),
    Revert(String),
    Reset {
        revision: String,
        mode: String,
    },
    RestoreFile {
        revision: String,
        path: PathBuf,
    },
    Fetch(Option<String>),
    Pull(bool),
    Push(Option<String>),
    ForcePush {
        remote: String,
        branch: String,
        expected: String,
    },
    PushTag {
        remote: String,
        tag: String,
    },
    DeleteRemoteTag {
        remote: String,
        tag: String,
    },
    DeleteRemoteBranch {
        remote: String,
        branch: String,
    },
    SetUpstream(String),
    AddRemote(String, String),
    RemoveRemote(String),
    StashSave(String),
    StashApply(String),
    StashPop {
        name: String,
        id: String,
    },
    StashDrop {
        name: String,
        id: String,
    },
    AddWorktree {
        path: PathBuf,
        revision: String,
    },
    RemoveWorktree(PathBuf),
    Continue(String),
    Abort(String),
    ConflictSide {
        path: PathBuf,
        incoming: bool,
    },
    Resolve {
        path: PathBuf,
        original: Arc<String>,
        text: Arc<String>,
    },
    Hunks {
        change: Change,
        patch: Arc<String>,
        start: usize,
        end: usize,
        original: bool,
    },
}
impl Operation {
    pub fn requires_editor_lock(&self) -> bool {
        !matches!(
            self,
            Self::Fetch(_)
                | Self::Push(_)
                | Self::ForcePush { .. }
                | Self::PushTag { .. }
                | Self::DeleteRemoteTag { .. }
                | Self::DeleteRemoteBranch { .. }
                | Self::AddRemote(..)
                | Self::RemoveRemote(_)
                | Self::SetUpstream(_)
                | Self::Identity { .. }
                | Self::Tag(..)
                | Self::DeleteTag(_)
                | Self::DeleteBranch(_)
                | Self::RenameBranch(..)
        )
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Init(_) => "Initialize repository",
            Self::Identity { .. } => "Commit identity",
            Self::Submodules => "Update submodules",
            Self::Clone { .. } => "Clone repository",
            Self::Ignore(_) => "Ignore file",
            Self::Stage(_) => "Stage",
            Self::Unstage(_) => "Unstage",
            Self::Discard(_) => "Discard changes",
            Self::Commit { amend: false, .. } => "Commit",
            Self::Commit { amend: true, .. } => "Amend commit",
            Self::CreateBranch(_) => "Create branch",
            Self::Switch(_) => "Switch branch",
            Self::RenameBranch(..) => "Rename branch",
            Self::DeleteBranch(_) => "Delete branch",
            Self::Tag(..) => "Create tag",
            Self::DeleteTag(_) => "Delete tag",
            Self::Merge(_) => "Merge",
            Self::Rebase(_) => "Rebase",
            Self::CherryPick(_) => "Cherry-pick",
            Self::Revert(_) => "Revert commit",
            Self::Reset { mode, .. } => match mode.as_str() {
                "soft" => "Reset · soft",
                "mixed" => "Reset · mixed",
                _ => "Reset · hard",
            },
            Self::RestoreFile { .. } => "Restore file",
            Self::Fetch(_) => "Fetch",
            Self::Pull(true) => "Pull with rebase",
            Self::Pull(false) => "Pull · fast-forward",
            Self::Push(Some(_)) => "Publish branch",
            Self::Push(None) => "Push",
            Self::ForcePush { .. } => "Force push with lease",
            Self::PushTag { .. } => "Push tag",
            Self::DeleteRemoteTag { .. } => "Delete remote tag",
            Self::DeleteRemoteBranch { .. } => "Delete remote branch",
            Self::SetUpstream(_) => "Set upstream",
            Self::AddRemote(..) => "Add remote",
            Self::RemoveRemote(_) => "Remove remote",
            Self::StashSave(_) => "Save stash",
            Self::StashApply(_) => "Apply stash",
            Self::StashPop { .. } => "Pop stash",
            Self::StashDrop { .. } => "Drop stash",
            Self::AddWorktree { .. } => "Add worktree",
            Self::RemoveWorktree(_) => "Remove worktree",
            Self::Continue(_) => "Continue",
            Self::Abort(_) => "Abort",
            Self::ConflictSide {
                incoming: false, ..
            } => "Use current file",
            Self::ConflictSide { incoming: true, .. } => "Use incoming file",
            Self::Resolve { .. } => "Save resolution and stage",
            Self::Hunks { change, .. } => {
                if change.area == Area::Staged {
                    "Unstage selected blocks"
                } else {
                    "Stage selected blocks"
                }
            }
        }
    }
    pub fn menu_label(&self) -> String {
        match self {
            Self::PushTag { remote, .. }
            | Self::DeleteRemoteTag { remote, .. }
            | Self::DeleteRemoteBranch { remote, .. }
            | Self::ForcePush { remote, .. } => format!("{} · {remote}", tr(self.label())),
            _ => tr(self.label()),
        }
    }
    pub fn confirmation(&self) -> bool {
        matches!(
            self,
            Self::Discard(_)
                | Self::ForcePush { .. }
                | Self::DeleteRemoteTag { .. }
                | Self::DeleteRemoteBranch { .. }
                | Self::Commit { amend: true, .. }
                | Self::DeleteBranch(_)
                | Self::DeleteTag(_)
                | Self::Merge(_)
                | Self::Rebase(_)
                | Self::CherryPick(_)
                | Self::Revert(_)
                | Self::Reset { .. }
                | Self::RestoreFile { .. }
                | Self::RemoveRemote(_)
                | Self::StashPop { .. }
                | Self::StashDrop { .. }
                | Self::RemoveWorktree(_)
                | Self::Abort(_)
                | Self::ConflictSide { .. }
        )
    }
    pub fn details(&self) -> String {
        match self {
            Self::ForcePush {
                remote,
                branch,
                expected,
            } => format!(
                "{remote}/{branch}\n{}: {expected}\n{}",
                tr("Expected remote commit"),
                tr("Replace remote history only if the remote still points to this commit.")
            ),
            Self::PushTag { remote, tag } | Self::DeleteRemoteTag { remote, tag } => {
                format!("{remote} · {tag}")
            }
            Self::DeleteRemoteBranch { remote, branch } => format!("{remote}/{branch}"),
            Self::Discard(change) => format!(
                "{}\n{}",
                change.path.display(),
                tr("Unstaged changes will be permanently discarded. Staged changes are kept.")
            ),
            Self::Reset { revision, mode } => format!(
                "{revision} · {mode}\n{}",
                tr(if mode == "hard" {
                    "Tracked files and staged changes will be replaced. Untracked files blocking checkout can also be deleted."
                } else if mode == "mixed" {
                    "Move HEAD and clear the index. Working files are kept."
                } else {
                    "Move HEAD. The index and working files are kept."
                })
            ),
            Self::RestoreFile { revision, path } => format!(
                "{} ← {revision}\n{}",
                path.display(),
                tr("The working file will be replaced. Staged changes are kept.")
            ),
            Self::Commit { .. } => tr(
                "Replace the latest commit with the staged contents and this message. This rewrites its commit ID.",
            ),
            Self::StashPop { name, id } | Self::StashDrop { name, id } => format!("{name}\n{id}"),
            Self::ConflictSide { path, .. } => format!(
                "{}\n{}",
                path.display(),
                tr(
                    "Replace the entire working file with this side. Review it before marking resolved. During rebase, current is the target branch and incoming is the replayed commit."
                )
            ),
            Self::RemoveWorktree(path) => path.display().to_string(),
            Self::Switch(name)
            | Self::DeleteBranch(name)
            | Self::DeleteTag(name)
            | Self::Merge(name)
            | Self::Rebase(name)
            | Self::CherryPick(name)
            | Self::Revert(name)
            | Self::RemoveRemote(name)
            | Self::Abort(name) => name.clone(),
            _ => String::new(),
        }
    }
}

fn input_file(bytes: &[u8]) -> Result<tempfile::NamedTempFile> {
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(file)
}
fn name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && !value.starts_with('-') && !value.contains(['\0', '\n', '\r']),
        "Invalid Git name"
    );
    Ok(())
}
fn paths(change: &Change) -> Result<Vec<OsString>> {
    let mut paths = Vec::new();
    if let Some(previous) = &change.previous
        && matches!(change.status, 'R' | 'C')
    {
        relative(previous.as_os_str().as_bytes().to_vec())?;
        paths.push(previous.as_os_str().to_owned());
    }
    relative(change.path.as_os_str().as_bytes().to_vec())?;
    paths.push(change.path.as_os_str().to_owned());
    Ok(paths)
}

pub fn execute(
    root: &Path,
    remote: Option<&Session>,
    operation: &Operation,
    cancel: &Cancellation,
) -> Result<()> {
    // Share the file-save lock with local editors and filesystem operations.
    let _save = operation
        .requires_editor_lock()
        .then(|| {
            crate::files::SAVE_LOCK
                .lock()
                .map_err(|_| anyhow::anyhow!("File save lock unavailable"))
        })
        .transpose()?;
    match operation {
        Operation::Init(path) => {
            ensure!(path.is_absolute(), "Enter an absolute repository path");
            run(
                command(
                    Path::new("/"),
                    remote,
                    &["init".into(), "--".into(), path.as_os_str().to_owned()],
                )?,
                false,
                cancel,
            )?;
            return Ok(());
        }
        Operation::Clone { url, path } => {
            ensure!(
                path.is_absolute()
                    && !url.is_empty()
                    && !url.starts_with('-')
                    && !url.contains('\0'),
                "Enter a repository URL and an absolute destination path"
            );
            run(
                command(
                    Path::new("/"),
                    remote,
                    &[
                        "clone".into(),
                        "--".into(),
                        url.into(),
                        path.as_os_str().to_owned(),
                    ],
                )?,
                false,
                cancel,
            )?;
            return Ok(());
        }
        _ => {}
    }
    let run_args = |args: Vec<OsString>, input: Option<&tempfile::NamedTempFile>| -> Result<()> {
        run_input(command(root, remote, &args)?, false, input, cancel)?;
        Ok(())
    };
    let simple = |args: &[&str]| run_args(args.iter().map(OsString::from).collect(), None);
    let resolved = |value: &str| revision(root, remote, value, cancel);
    match operation {
        Operation::Init(_) | Operation::Clone { .. } => unreachable!(),
        Operation::Identity { name, email } => {
            ensure!(
                !name.trim().is_empty()
                    && !email.trim().is_empty()
                    && !name.contains(['\n', '\0'])
                    && !email.contains(['\n', '\0']),
                "Enter an author name and email"
            );
            simple(&["config", "--local", "user.name", name])?;
            simple(&["config", "--local", "user.email", email])
        }
        Operation::Submodules => simple(&["submodule", "update", "--init", "--recursive"]),
        Operation::Ignore(path) => {
            relative(path.as_os_str().as_bytes().to_vec())?;
            let filename = path.to_str().context("Ignore requires a UTF-8 filename")?;
            ensure!(
                !filename.contains(['\n', '\r']),
                "Cannot add a newline filename to .gitignore"
            );
            let ignore = root.join(".gitignore");
            let exists = match remote {
                Some(session) => session.git_path_exists(&ignore, cancel)?,
                None => ignore.try_exists()?,
            };
            let original = if exists {
                let source = conflict_source(root, remote, Path::new(".gitignore"), cancel)?;
                String::from_utf8(crate::resources::read_to_end(source.reopen()?, cancel)?)?
            } else {
                String::new()
            };
            let mut text = original.clone();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push('/');
            for ch in filename.chars() {
                if "\\#!*?[] ".contains(ch) {
                    text.push('\\');
                }
                text.push(ch);
            }
            text.push('\n');
            let mut patch = replacement_patch(Path::new(".gitignore"), &original, &text)?;
            if !exists {
                patch = patch.replace(
                    "--- \"a/.gitignore\"",
                    "new file mode 100644\n--- /dev/null",
                );
            }
            simple_apply(
                root,
                remote,
                &input_file(patch.as_bytes())?,
                false,
                false,
                cancel,
            )
        }
        Operation::Stage(change) | Operation::Unstage(change) => {
            let stage = matches!(operation, Operation::Stage(_));
            if let Some(change) = change {
                paths(change)?;
            }
            if stage
                && let Some(change) = change
                && change.status == 'U'
            {
                let exists = match remote {
                    Some(session) => session.git_path_exists(&root.join(&change.path), cancel)?,
                    None => root.join(&change.path).try_exists()?,
                };
                if exists {
                    let source = conflict_source(root, remote, &change.path, cancel)?;
                    let mut input = source.reopen()?;
                    let mut buffer = vec![0; crate::limits::IO_BUFFER_BYTES];
                    let mut prefix = Vec::with_capacity(8);
                    loop {
                        cancel.check()?;
                        let read = input.read(&mut buffer)?;
                        if read == 0 {
                            break;
                        }
                        for byte in &buffer[..read] {
                            if *byte == b'\n' {
                                prefix.clear();
                            } else if prefix.len() < 8 {
                                prefix.push(*byte);
                                ensure!(
                                    prefix.as_slice() != b"<<<<<<< "
                                        && prefix.as_slice() != b">>>>>>> ",
                                    "Resolve conflict markers before staging this file"
                                );
                            }
                        }
                    }
                }
            }
            let mut args: Vec<OsString> = if stage {
                vec!["add".into(), "--all".into()]
            } else if metadata(root, remote, cancel)?.head.is_empty() {
                vec![
                    "rm".into(),
                    "--cached".into(),
                    "--force".into(),
                    "-r".into(),
                    "--ignore-unmatch".into(),
                ]
            } else {
                vec!["restore".into(), "--staged".into()]
            };
            args.push("--".into());
            args.extend(match change {
                Some(change) => paths(change)?,
                None => vec![".".into()],
            });
            run_args(args, None)
        }
        Operation::Discard(change) => {
            ensure!(
                change.area == Area::Working && change.status != 'U',
                "Only unstaged tracked changes can be discarded"
            );
            let mut args = vec!["restore".into(), "--worktree".into(), "--".into()];
            args.extend(paths(change)?);
            run_args(args, None)
        }
        Operation::Commit { message, amend } => {
            ensure!(!message.trim().is_empty(), "Enter a commit message");
            let input = input_file(message.as_bytes())?;
            let mut args = vec!["commit".into(), "--file=-".into()];
            if *amend {
                args.push("--amend".into());
            }
            run_args(args, Some(&input))
        }
        Operation::CreateBranch(branch) => {
            name(branch)?;
            simple(&["check-ref-format", "--branch", branch])?;
            simple(&["switch", "--create", branch])
        }
        Operation::Switch(branch) => {
            name(branch)?;
            if branch.starts_with("refs/tags/") {
                simple(&["switch", "--detach", &resolved(branch)?])
            } else if let Some(branch) = branch.strip_prefix("refs/remotes/") {
                simple(&["switch", "--track", "--", branch])
            } else {
                simple(&[
                    "switch",
                    "--",
                    branch.strip_prefix("refs/heads/").unwrap_or(branch),
                ])
            }
        }
        Operation::RenameBranch(old, new) => {
            name(old)?;
            name(new)?;
            simple(&["branch", "--move", old, new])
        }
        Operation::DeleteBranch(branch) => {
            name(branch)?;
            simple(&["branch", "--delete", "--", branch])
        }
        Operation::Tag(tag, target) => {
            name(tag)?;
            simple(&["check-ref-format", &format!("refs/tags/{tag}")])?;
            simple(&["tag", "--", tag, &resolved(target)?])
        }
        Operation::DeleteTag(tag) => {
            name(tag)?;
            simple(&["tag", "--delete", "--", tag])
        }
        Operation::Merge(target) => simple(&["merge", "--no-edit", &resolved(target)?]),
        Operation::Rebase(target) => simple(&["rebase", &resolved(target)?]),
        Operation::CherryPick(target) => simple(&["cherry-pick", &resolved(target)?]),
        Operation::Revert(target) => simple(&["revert", "--no-edit", &resolved(target)?]),
        Operation::Reset { revision, mode } => {
            ensure!(
                ["soft", "mixed", "hard"].contains(&mode.as_str()),
                "Invalid reset mode"
            );
            simple(&["reset", &format!("--{mode}"), &resolved(revision)?])
        }
        Operation::RestoreFile { revision, path } => {
            relative(path.as_os_str().as_bytes().to_vec())?;
            run_args(
                vec![
                    "restore".into(),
                    format!("--source={}", resolved(revision)?).into(),
                    "--worktree".into(),
                    "--".into(),
                    path.as_os_str().to_owned(),
                ],
                None,
            )
        }
        Operation::Fetch(remote) => match remote {
            Some(remote) => {
                name(remote)?;
                simple(&["fetch", "--prune", "--", remote])
            }
            None => simple(&["fetch", "--all", "--prune"]),
        },
        Operation::Pull(rebase) => simple(if *rebase {
            &["pull", "--rebase"]
        } else {
            &["pull", "--ff-only"]
        }),
        Operation::Push(destination) => match destination {
            Some(destination) => {
                name(destination)?;
                let branch = metadata(root, remote, cancel)?.branch;
                name(&branch)?;
                ensure!(branch != "(detached)", "Create a branch before publishing");
                simple(&["push", "--set-upstream", "--", destination, &branch])
            }
            None => simple(&["push"]),
        },
        Operation::ForcePush {
            remote: destination,
            branch,
            expected,
        } => {
            name(destination)?;
            name(branch)?;
            ensure!(
                !expected.is_empty() && expected.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid expected remote commit"
            );
            simple(&[
                "push",
                &format!("--force-with-lease=refs/heads/{branch}:{expected}"),
                "--",
                destination,
                &format!("HEAD:refs/heads/{branch}"),
            ])
        }
        Operation::PushTag {
            remote: destination,
            tag,
        } => {
            name(destination)?;
            name(tag)?;
            simple(&[
                "push",
                "--",
                destination,
                &format!("refs/tags/{tag}:refs/tags/{tag}"),
            ])
        }
        Operation::DeleteRemoteTag {
            remote: destination,
            tag,
        } => {
            name(destination)?;
            name(tag)?;
            simple(&[
                "push",
                "--delete",
                "--",
                destination,
                &format!("refs/tags/{tag}"),
            ])
        }
        Operation::DeleteRemoteBranch {
            remote: destination,
            branch,
        } => {
            name(destination)?;
            name(branch)?;
            simple(&[
                "push",
                "--delete",
                "--",
                destination,
                &format!("refs/heads/{branch}"),
            ])
        }
        Operation::SetUpstream(upstream) => {
            name(upstream)?;
            simple(&["branch", &format!("--set-upstream-to={upstream}")])
        }
        Operation::AddRemote(remote, url) => {
            name(remote)?;
            ensure!(!url.is_empty() && !url.contains('\0'), "Enter a remote URL");
            simple(&["remote", "add", "--", remote, url])
        }
        Operation::RemoveRemote(remote) => {
            name(remote)?;
            simple(&["remote", "remove", remote])
        }
        Operation::StashSave(message) => {
            simple(&["stash", "push", "--include-untracked", "--message", message])
        }
        Operation::StashApply(id) => simple(&["stash", "apply", &resolved(id)?]),
        Operation::StashPop { name, id } | Operation::StashDrop { name, id } => {
            ensure!(
                name.starts_with("stash@{") && name.ends_with('}'),
                "Invalid stash reference"
            );
            ensure!(
                resolved(name)? == *id,
                "Stash changed. Refresh before retrying."
            );
            simple(&[
                "stash",
                if matches!(operation, Operation::StashPop { .. }) {
                    "pop"
                } else {
                    "drop"
                },
                name,
            ])
        }
        Operation::AddWorktree { path, revision } => {
            ensure!(path.is_absolute(), "Enter an absolute worktree path");
            run_args(
                vec![
                    "worktree".into(),
                    "add".into(),
                    "--detach".into(),
                    "--".into(),
                    path.as_os_str().to_owned(),
                    resolved(revision)?.into(),
                ],
                None,
            )
        }
        Operation::RemoveWorktree(path) => {
            ensure!(
                path.is_absolute() && path != root,
                "Cannot remove the active worktree"
            );
            run_args(
                vec![
                    "worktree".into(),
                    "remove".into(),
                    "--".into(),
                    path.as_os_str().to_owned(),
                ],
                None,
            )
        }
        Operation::Continue(kind) | Operation::Abort(kind) => {
            ensure!(
                ["merge", "rebase", "cherry-pick", "revert"].contains(&kind.as_str()),
                "Invalid Git operation"
            );
            simple(&[
                kind,
                if matches!(operation, Operation::Continue(_)) {
                    "--continue"
                } else {
                    "--abort"
                },
            ])
        }
        Operation::ConflictSide { path, incoming } => {
            relative(path.as_os_str().as_bytes().to_vec())?;
            run_args(
                vec![
                    "checkout".into(),
                    if *incoming {
                        "--theirs".into()
                    } else {
                        "--ours".into()
                    },
                    "--".into(),
                    path.as_os_str().to_owned(),
                ],
                None,
            )
        }
        Operation::Resolve {
            path,
            original,
            text,
        } => {
            relative(path.as_os_str().as_bytes().to_vec())?;
            let patch = replacement_patch(path, original, text)?;
            let input = input_file(patch.as_bytes())?;
            simple_apply(root, remote, &input, false, false, cancel)?;
            run_args(
                vec!["add".into(), "--".into(), path.as_os_str().to_owned()],
                None,
            )
        }
        Operation::Hunks {
            change,
            patch,
            start,
            end,
            original,
        } => {
            ensure!(
                change.status == 'M' && change.previous.is_none() && change.area != Area::Untracked,
                "Select a modified tracked file"
            );
            let patch = selected_hunks(patch, *start, *end, *original)?;
            simple_apply(
                root,
                remote,
                &input_file(patch.as_bytes())?,
                true,
                change.area == Area::Staged,
                cancel,
            )
        }
    }
}

fn simple_apply(
    root: &Path,
    remote: Option<&Session>,
    input: &tempfile::NamedTempFile,
    cached: bool,
    reverse: bool,
    cancel: &Cancellation,
) -> Result<()> {
    let mut args: Vec<OsString> = vec!["apply".into(), "--whitespace=nowarn".into()];
    if cached {
        args.push("--cached".into());
    }
    if reverse {
        args.push("--reverse".into());
    }
    let mut check = args.clone();
    check.push("--check".into());
    run_input(command(root, remote, &check)?, false, Some(input), cancel)?;
    run_input(command(root, remote, &args)?, false, Some(input), cancel)?;
    Ok(())
}

fn selected_hunks(patch: &str, start: usize, end: usize, original: bool) -> Result<String> {
    let mut header = String::new();
    let mut hunks = Vec::<(bool, String)>::new();
    for line in patch.split_inclusive('\n') {
        if line.starts_with("@@ ") {
            let range = line
                .split_whitespace()
                .nth(if original { 1 } else { 2 })
                .context("Invalid hunk")?;
            let mut values = range[1..].split(',');
            let first = values.next().unwrap_or("0").parse::<usize>()?;
            let length = values.next().unwrap_or("1").parse::<usize>()?;
            let last = first.saturating_add(length.saturating_sub(1));
            hunks.push((start <= last.max(first) && end >= first, line.into()));
        } else if let Some((_, hunk)) = hunks.last_mut() {
            hunk.push_str(line);
        } else {
            header.push_str(line);
        }
    }
    ensure!(
        hunks.iter().any(|(selected, _)| *selected),
        "Select lines inside a change block"
    );
    for (_, hunk) in hunks.into_iter().filter(|(selected, _)| *selected) {
        header.push_str(&hunk);
    }
    Ok(header)
}

fn quoted_path(prefix: &str, path: &Path) -> String {
    let mut value = format!("\"{prefix}/");
    for byte in path.as_os_str().as_bytes() {
        match byte {
            b'"' => value.push_str("\\\""),
            b'\\' => value.push_str("\\\\"),
            32..=126 => value.push(*byte as char),
            _ => value.push_str(&format!("\\{byte:03o}")),
        }
    }
    value.push('"');
    value
}
fn replacement_patch(path: &Path, original: &str, text: &str) -> Result<String> {
    ensure!(
        !original.contains('\0') && !text.contains('\0'),
        "Binary conflicts require choosing an entire side"
    );
    let old = quoted_path("a", path);
    let new = quoted_path("b", path);
    let old_count = original.split_inclusive('\n').count();
    let new_count = text.split_inclusive('\n').count();
    let mut patch = format!(
        "diff --git {old} {new}\n--- {old}\n+++ {new}\n@@ -{},{} +{},{} @@\n",
        usize::from(old_count > 0),
        old_count,
        usize::from(new_count > 0),
        new_count
    );
    for (prefix, content) in [('-', original), ('+', text)] {
        for line in content.split_inclusive('\n') {
            patch.push(prefix);
            patch.push_str(line);
            if !line.ends_with('\n') {
                patch.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    Ok(patch)
}
