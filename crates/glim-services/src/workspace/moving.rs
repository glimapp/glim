use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// Move the entry, retaining symlinks and refusing to replace an existing name.
pub fn move_entry(source: &Path, directory: &Path) -> Result<PathBuf> {
    let name = source
        .file_name()
        .context("Cannot move a filesystem root")?;
    relocate_entry(source, &directory.join(name), false)
}

pub fn rename_entry(source: &Path, name: &OsStr) -> Result<PathBuf> {
    ensure!(
        Path::new(name).file_name() == Some(name),
        "Enter a filename without slashes"
    );
    let parent = source.parent().context("Cannot rename a filesystem root")?;
    relocate_entry(source, &parent.join(name), true)
}

fn relocate_entry(source: &Path, destination: &Path, renaming: bool) -> Result<PathBuf> {
    let _guard = crate::files::SAVE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("File operation lock unavailable"))?;
    let source = std::path::absolute(source)?;
    let destination = std::path::absolute(destination)?;
    let directory = destination
        .parent()
        .context("Cannot move to a filesystem root")?;
    source
        .file_name()
        .context("Cannot move a filesystem root")?;
    let metadata = std::fs::symlink_metadata(&source).context("The item no longer exists")?;
    let parent = source
        .parent()
        .context("Cannot move a filesystem root")?
        .canonicalize()?;
    let target = directory
        .canonicalize()
        .context("Cannot open the destination folder")?;
    ensure!(target.is_dir(), "The destination is not a folder");
    ensure!(
        renaming || parent != target,
        "The item is already in this folder"
    );
    if source == destination {
        return Ok(destination);
    }
    if metadata.is_dir() {
        let real_source = source.canonicalize()?;
        ensure!(
            !target.starts_with(&real_source),
            "A folder cannot be moved into itself or its descendants"
        );
    }
    match std::fs::symlink_metadata(&destination) {
        Ok(existing) => {
            // Case-insensitive volumes can resolve both spellings to one entry.
            use std::os::unix::fs::MetadataExt;
            ensure!(
                renaming
                    && metadata.dev() == existing.dev()
                    && metadata.ino() == existing.ino()
                    && source.canonicalize()? == destination.canonicalize()?,
                "An item with this name already exists in the destination folder"
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("Cannot inspect the destination"),
    }
    // Foundation also refuses a destination created after the check, and handles
    // moves between volumes without following the source entry's symlink.
    super::run_file_operation(&[
        OsStr::new("--move"),
        source.as_os_str(),
        destination.as_os_str(),
    ])?;
    Ok(destination)
}

#[cfg(test)]
mod local_file_action_tests {
    use super::*;
    use std::{fs, os::unix::fs::symlink};

    #[test]
    fn rename_files_and_folders_preserves_contents_and_move_support() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let folder = temp.path().join("before");
        fs::create_dir(&folder)?;
        fs::write(folder.join("note.md"), "unsaved on disk")?;
        let renamed = rename_entry(&folder, OsStr::new("after"))?;
        let file = rename_entry(&renamed.join("note.md"), OsStr::new("新名称.md"))?;
        assert!(!folder.exists());
        assert_eq!(fs::read_to_string(&file)?, "unsaved on disk");
        let moved = move_entry(&file, temp.path())?;
        assert_eq!(fs::read_to_string(moved)?, "unsaved on disk");
        Ok(())
    }

    #[test]
    fn conflicts_and_invalid_names_leave_original_entries_untouched() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source = temp.path().join("source.txt");
        let existing = temp.path().join("existing.txt");
        fs::write(&source, "source")?;
        fs::write(&existing, "existing")?;
        fs::hard_link(&source, temp.path().join("hard-link.txt"))?;
        for name in [
            "existing.txt",
            "hard-link.txt",
            "",
            ".",
            "..",
            "nested/name",
            "/absolute",
        ] {
            assert!(
                rename_entry(&source, OsStr::new(name)).is_err(),
                "Accepted {name:?}"
            );
        }
        assert!(super::super::create_file(&source).is_err());
        assert!(rename_entry(Path::new("/"), OsStr::new("root")).is_err());
        assert_eq!(fs::read_to_string(source)?, "source");
        assert_eq!(fs::read_to_string(existing)?, "existing");
        Ok(())
    }

    #[test]
    fn rename_symlink_moves_link_without_changing_target() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let target = temp.path().join("target.txt");
        let link = temp.path().join("link.txt");
        fs::write(&target, "keep")?;
        symlink(&target, &link)?;
        let renamed = rename_entry(&link, OsStr::new("renamed-link.txt"))?;
        assert!(!link.try_exists()?);
        assert!(fs::symlink_metadata(&renamed)?.is_symlink());
        assert_eq!(fs::read_link(renamed)?, target);
        assert_eq!(fs::read_to_string(target)?, "keep");
        Ok(())
    }

    #[test]
    fn case_only_rename_and_unchanged_name_are_supported() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source = temp.path().join("MixedCase.md");
        fs::write(&source, "keep")?;
        assert_eq!(rename_entry(&source, OsStr::new("MixedCase.md"))?, source);
        let renamed = rename_entry(&source, OsStr::new("mixedcase.md"))?;
        assert_eq!(fs::read_to_string(renamed)?, "keep");
        let names = fs::read_dir(temp.path())?
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(names, vec![OsStr::new("mixedcase.md")]);
        Ok(())
    }
}
