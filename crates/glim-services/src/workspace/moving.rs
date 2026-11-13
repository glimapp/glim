use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

/// Move the entry, retaining symlinks and refusing to replace an existing name.
pub fn move_entry(source: &Path, directory: &Path) -> Result<PathBuf> {
    let _guard = crate::files::SAVE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("File operation lock unavailable"))?;
    let source = std::path::absolute(source)?;
    let directory = std::path::absolute(directory)?;
    let name = source
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
    ensure!(parent != target, "The item is already in this folder");
    if metadata.is_dir() {
        let real_source = source.canonicalize()?;
        ensure!(
            !target.starts_with(&real_source),
            "A folder cannot be moved into itself or its descendants"
        );
    }
    let destination = directory.join(name);
    match std::fs::symlink_metadata(&destination) {
        Ok(_) => anyhow::bail!("An item with this name already exists in the destination folder"),
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
