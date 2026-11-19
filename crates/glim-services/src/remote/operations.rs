use super::{RemoteVersion, Session, sftp::Sftp};
use crate::{locale::tr, work::Cancellation};
use anyhow::{Context, Result, ensure};
use std::path::{Component, Path, PathBuf};

fn entry_path(transport: &mut Sftp, path: &Path, cancel: &Cancellation) -> Result<PathBuf> {
    ensure!(
        path.is_absolute()
            && path
                .components()
                .all(|part| matches!(part, Component::RootDir | Component::Normal(_))),
        tr("Invalid remote path")
    );
    let name = path
        .file_name()
        .context(tr("Cannot modify the filesystem root"))?;
    ensure!(
        !name.as_encoded_bytes().contains(&0),
        tr("Invalid remote filename")
    );
    let parent = transport.realpath(path.parent().context(tr("Missing remote parent"))?, cancel)?;
    Ok(parent.join(name))
}
impl Session {
    pub fn save_text(
        &self,
        path: &Path,
        expected: &str,
        text: &str,
        mut version: RemoteVersion,
        cancel: &Cancellation,
    ) -> Result<RemoteVersion> {
        ensure!(!text.contains('\0'), "Text contains a null byte");
        self.with_transport(cancel, |transport| {
            let entry = entry_path(transport, path, cancel)?;
            let target = transport.realpath(&entry, cancel)?;
            let original = transport
                .lstat_optional(&target, cancel)?
                .context("The remote file was moved or deleted")?;
            ensure!(
                original.mode & 0o170000 == 0o100000 && original.mode & 0o222 != 0,
                "Remote file is not a writable regular file"
            );
            transport.check_writable(&target, cancel)?;
            let changed = "Remote file changed on the server. Your edits are still open.";
            if !matches_remote_text(transport, &target, expected, cancel)? {
                // A reply can be lost after the server has applied a replacement.
                ensure!(
                    matches_remote_text(transport, &target, text, cancel)?,
                    changed
                );
                ensure!(
                    transport.realpath(path, cancel)? == target
                        && transport.lstat_optional(&target, cancel)?.as_ref() == Some(&original),
                    changed
                );
                version.insert(path.to_path_buf(), Some(original));
                return Ok(version);
            }

            // Use a private, exclusively created sibling so replacement stays on the same filesystem.
            let nonce = tempfile::Builder::new().prefix(".glim-save-").tempfile()?;
            let temporary =
                target.with_file_name(nonce.path().file_name().context("Invalid save file name")?);
            let watch = transport.watch(cancel);
            let handle = transport.open_exclusive(&temporary)?;
            let result = (|| {
                let uploaded = transport.upload(&handle, text.as_bytes(), cancel);
                let closed = transport.close_handle(&handle);
                uploaded?;
                closed?;
                let temporary_stamp = transport.stat(&temporary, cancel)?;
                transport.copy_save_attributes(&temporary, &original, &temporary_stamp)?;
                let saved = transport.stat(&temporary, cancel)?;
                ensure!(
                    saved.mode == original.mode && saved.owner == original.owner,
                    "Could not preserve remote file permissions"
                );
                ensure!(
                    matches_remote_text(transport, &target, expected, cancel)?
                        && transport.realpath(path, cancel)? == target
                        && transport.lstat_optional(&target, cancel)?.as_ref() == Some(&original),
                    changed
                );
                cancel.check()?;
                transport.replace_file(&temporary, &target)?;
                version.insert(path.to_path_buf(), Some(saved));
                Ok(version)
            })();
            drop(watch);
            if result.is_err() && transport.healthy() {
                let _cleanup = transport.watch(&Cancellation::default());
                let _ = transport.remove_save_file(&temporary);
            }
            result
        })
    }

    pub(crate) fn git_path_exists(&self, path: &Path, cancel: &Cancellation) -> Result<bool> {
        self.with_transport(cancel, |transport| {
            Ok(transport.stat_optional(path, cancel)?.is_some())
        })
    }
    pub fn discover_git_roots(
        &self,
        roots: &[PathBuf],
        cancel: &Cancellation,
    ) -> Result<Vec<PathBuf>> {
        crate::git::ensure_enabled()?;
        self.with_transport(cancel, |transport| {
            let mut repositories = Vec::new();
            let mut inspected = std::collections::HashMap::new();
            for root in roots {
                let root = transport.realpath(root, cancel)?;
                for directory in root.ancestors() {
                    cancel.check()?;
                    let present = if let Some(present) = inspected.get(directory) {
                        *present
                    } else {
                        let present = transport
                            .stat_optional(&directory.join(".git"), cancel)?
                            .is_some_and(|stamp| {
                                matches!(stamp.mode & 0o170000, 0o040000 | 0o100000)
                            });
                        inspected.insert(directory.to_path_buf(), present);
                        present
                    };
                    if present {
                        if !repositories
                            .iter()
                            .any(|repository| repository == directory)
                        {
                            repositories.push(directory.to_path_buf());
                        }
                        break;
                    }
                }
            }
            Ok(repositories)
        })
    }

    pub fn create_entry(
        &self,
        path: &Path,
        directory: bool,
        cancel: &Cancellation,
    ) -> Result<PathBuf> {
        self.with_transport(cancel, |transport| {
            let path = entry_path(transport, path, cancel)?;
            ensure!(
                transport.lstat_optional(&path, cancel)?.is_none(),
                tr("An item with this name already exists")
            );
            if directory {
                transport.mkdir(&path, cancel)?;
            } else {
                transport.create_file(&path, cancel)?;
            }
            Ok(path)
        })
    }
    pub fn move_entry(
        &self,
        source: &Path,
        destination: &Path,
        cancel: &Cancellation,
    ) -> Result<PathBuf> {
        self.with_transport(cancel, |transport| {
            let source = entry_path(transport, source, cancel)?;
            let destination = entry_path(transport, destination, cancel)?;
            let stamp = transport
                .lstat_optional(&source, cancel)?
                .context(tr("Remote item no longer exists"))?;
            ensure!(
                source != destination && transport.lstat_optional(&destination, cancel)?.is_none(),
                tr("An item with this name already exists")
            );
            if stamp.mode & 0o170000 == 0o040000 {
                ensure!(
                    !destination.starts_with(&source),
                    tr("Cannot move a folder inside itself")
                );
            }
            transport.rename(&source, &destination, cancel)?;
            Ok(destination)
        })
    }
    /// Reversible deletion using only SFTP; no shell commands or recursive removal.
    pub fn trash_entry(&self, source: &Path, cancel: &Cancellation) -> Result<PathBuf> {
        self.with_transport(cancel, |transport| {
            let source = entry_path(transport, source, cancel)?;
            let base = self.home.join(".glim-trash");
            ensure!(
                !base.starts_with(&source) && !source.starts_with(&base),
                tr("Cannot move this folder to the remote Trash")
            );
            match transport.lstat_optional(&base, cancel)? {
                Some(stamp) => ensure!(
                    stamp.mode & 0o170000 == 0o040000 && stamp.mode & 0o077 == 0,
                    tr("Remote Trash must be a private directory")
                ),
                None => transport.mkdir(&base, cancel)?,
            }
            let token = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos();
            let directory = base.join(format!("{token}-{}", std::process::id()));
            transport.mkdir(&directory, cancel)?;
            let destination =
                directory.join(source.file_name().context(tr("Missing remote filename"))?);
            transport.rename(&source, &destination, cancel)?;
            Ok(destination)
        })
    }
}

fn matches_remote_text(
    transport: &mut Sftp,
    path: &Path,
    text: &str,
    cancel: &Cancellation,
) -> Result<bool> {
    let mut remaining = text.as_bytes();
    let complete = transport.read_chunks(path, cancel, |chunk| {
        let Some(rest) = remaining.strip_prefix(chunk) else {
            return Ok(false);
        };
        remaining = rest;
        Ok(true)
    })?;
    Ok(complete && remaining.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_text_save_replaces_complete_files_and_keeps_external_changes() -> Result<()> {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = tempfile::tempdir()?;
        let root = directory.path().canonicalize()?;
        let cancel = Cancellation::default();
        let transport = Sftp::local_test(&root)?;
        let session =
            Session::from_transport(super::super::Connection::default(), transport, &cancel)?;
        let file = session.create_entry(&root.join("new.txt"), false, &cancel)?;
        let link = root.join("link.txt");
        symlink(&file, &link)?;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640))?;
        let first = "第一行\n".repeat(crate::limits::REMOTE_TRANSFER_BYTES as usize / 4 + 1);
        let version = session.save_text(&link, "", &first, RemoteVersion::new(), &cancel)?;
        assert_eq!(std::fs::read_to_string(&file)?, first);
        assert_eq!(
            std::fs::metadata(&file)?.permissions().mode() & 0o777,
            0o640
        );
        assert!(std::fs::symlink_metadata(&link)?.is_symlink());
        assert!(!session.has_changed(&version, &cancel)?);
        session.save_text(&link, "", &first, RemoteVersion::new(), &cancel)?;

        let external = "第两行\n".repeat(crate::limits::REMOTE_TRANSFER_BYTES as usize / 4 + 1);
        assert_eq!(external.len(), first.len());
        std::fs::write(&file, &external)?;
        assert!(
            session
                .save_text(&link, &first, "draft", version, &cancel)
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&file)?, external);
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o440))?;
        assert!(
            session
                .save_text(&link, &external, "draft", RemoteVersion::new(), &cancel)
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&file)?, external);
        assert_eq!(std::fs::read_dir(&root)?.count(), 2);
        Ok(())
    }

    #[test]
    fn remote_operations_preserve_conflicts_links_and_trashed_subtrees() -> Result<()> {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir()?;
        let root = directory.path().canonicalize()?;
        let cancel = Cancellation::default();
        let transport = Sftp::local_test(&root)?;
        let session =
            Session::from_transport(super::super::Connection::default(), transport, &cancel)?;
        let folder = session.create_entry(&root.join("中文 ; $(name)"), true, &cancel)?;
        let file = session.create_entry(&folder.join("data.txt"), false, &cancel)?;
        std::fs::write(&file, "retained contents")?;
        assert!(session.create_entry(&file, false, &cancel).is_err());
        assert_eq!(std::fs::read_to_string(&file)?, "retained contents");
        let conflict = session.create_entry(&root.join("conflict.txt"), false, &cancel)?;
        assert!(session.move_entry(&file, &conflict, &cancel).is_err());
        assert!(
            session
                .move_entry(&folder, &folder.join("nested"), &cancel)
                .is_err()
        );
        let renamed = session.move_entry(&file, &folder.join("renamed.txt"), &cancel)?;
        symlink(&root, folder.join("link"))?;
        let trashed = session.trash_entry(&folder, &cancel)?;
        assert!(!folder.exists());
        assert_eq!(
            std::fs::read_to_string(trashed.join(renamed.file_name().unwrap()))?,
            "retained contents"
        );
        assert!(std::fs::symlink_metadata(trashed.join("link"))?.is_symlink());
        assert!(conflict.exists());
        assert!(session.trash_entry(&root, &cancel).is_err());
        assert!(
            session
                .trash_entry(&root.join(".glim-trash"), &cancel)
                .is_err()
        );
        let restored = session.move_entry(&trashed, &folder, &cancel)?;
        assert!(restored.join("renamed.txt").exists());
        Ok(())
    }
}

#[cfg(all(test, feature = "git"))]
mod git_tests {
    use super::*;
    #[test]
    fn remote_git_detection_finds_ancestors_worktrees_and_deduplicates() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().canonicalize()?;
        let repository = root.join("repo");
        let worktree = root.join("worktree");
        std::fs::create_dir_all(repository.join(".git"))?;
        std::fs::create_dir_all(repository.join("nested/deep"))?;
        std::fs::create_dir_all(worktree.join("src"))?;
        std::fs::write(
            worktree.join(".git"),
            "gitdir: ../repo/.git/worktrees/linked\n",
        )?;
        std::fs::create_dir(root.join("plain"))?;
        let cancel = Cancellation::default();
        let session = Session::from_transport(
            super::super::Connection::default(),
            Sftp::local_test(&root)?,
            &cancel,
        )?;
        assert_eq!(
            session.discover_git_roots(
                &[
                    repository.join("nested"),
                    repository.join("nested/deep"),
                    worktree.join("src"),
                    root.join("plain")
                ],
                &cancel
            )?,
            vec![repository, worktree]
        );
        let (cancelled, guard) = Cancellation::task();
        drop(guard);
        assert!(session.discover_git_roots(&[root], &cancelled).is_err());
        Ok(())
    }
}
