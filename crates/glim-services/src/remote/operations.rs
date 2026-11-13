use super::{Session, sftp::Sftp};
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
    pub fn discover_git_roots(
        &self,
        roots: &[PathBuf],
        cancel: &Cancellation,
    ) -> Result<Vec<PathBuf>> {
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

#[cfg(test)]
mod tests {
    use super::*;
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

#[cfg(test)]
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
