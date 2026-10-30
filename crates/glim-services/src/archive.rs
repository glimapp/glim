//! ZIP filename decoding and streaming extraction into a new, isolated directory.
use crate::work::Cancellation;
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    path::{Component, Path, PathBuf},
};

pub struct Entry {
    pub name: String,
    pub size: u64,
    pub directory: bool,
}
pub struct Archive {
    pub path: PathBuf,
    pub entries: Vec<Entry>,
    pub bytes: u64,
}
pub fn name(entry: &zip::read::ZipFile<'_>) -> String {
    // Some macOS writers store UTF-8 without setting ZIP's UTF-8 flag.
    std::str::from_utf8(entry.name_raw())
        .map(str::to_owned)
        .unwrap_or_else(|_| entry.name().to_owned())
}
fn safe(name: &str) -> Result<PathBuf> {
    let path = Path::new(name);
    ensure!(
        !name.contains('\\')
            && !path.is_absolute()
            && !name.contains('\0')
            && !path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_))),
        "Unsafe ZIP path: {name}"
    );
    ensure!(!path.as_os_str().is_empty(), "Empty ZIP path");
    Ok(path.to_path_buf())
}
pub fn read(path: &Path, cancel: &Cancellation) -> Result<Archive> {
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    let mut entries = Vec::new();
    let mut bytes = 0u64;
    for i in 0..zip.len() {
        cancel.check()?;
        let entry = zip.by_index(i)?;
        let name = name(&entry);
        bytes = bytes.saturating_add(entry.size());
        entries.push(Entry {
            directory: name.ends_with('/'),
            name,
            size: entry.size(),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Archive {
        path: path.to_path_buf(),
        entries,
        bytes,
    })
}
pub fn extract(path: &Path, parent: &Path, cancel: &Cancellation) -> Result<PathBuf> {
    let stage = tempfile::Builder::new()
        .prefix(".glim-extract-")
        .tempdir_in(parent)?;
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    // Links are created last, so no file writes can traverse an archive-created link.
    let mut links = Vec::new();
    for i in 0..zip.len() {
        cancel.check()?;
        let mut entry = zip.by_index(i)?;
        let relative = safe(&name(&entry))?;
        let target = stage.path().join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mode = entry.unix_mode().unwrap_or(0o644);
        if mode & 0o170000 == 0o120000 {
            let mut value = Vec::new();
            // Symlink targets are filesystem paths, not arbitrary file payloads.
            use std::io::Read;
            (&mut entry).take(4097).read_to_end(&mut value)?;
            ensure!(value.len() <= 4096, "Invalid symlink target");
            let value = String::from_utf8(value)?;
            ensure!(!Path::new(&value).is_absolute(), "Absolute ZIP symlink");
            let mut depth = relative.parent().map_or(0, |p| p.components().count());
            for c in Path::new(&value).components() {
                match c {
                    Component::ParentDir => {
                        ensure!(depth > 0, "ZIP symlink escapes destination");
                        depth -= 1;
                    }
                    Component::Normal(_) => depth += 1,
                    _ => {}
                }
            }
            links.push((target, value));
        } else {
            ensure!(
                mode & 0o170000 == 0 || mode & 0o170000 == 0o100000,
                "Unsupported ZIP entry type"
            );
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            cancel.copy(&mut entry, &mut output)?;
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o777))?;
        }
    }
    for (target, value) in &links {
        cancel.check()?;
        let parent = target.parent().context("Missing symlink parent")?;
        ensure!(
            parent
                .canonicalize()?
                .starts_with(stage.path().canonicalize()?),
            "ZIP symlink parent escapes destination"
        );
        std::os::unix::fs::symlink(value, target)?;
    }
    for (target, _) in &links {
        cancel.check()?;
        ensure!(
            target
                .canonicalize()?
                .starts_with(stage.path().canonicalize()?),
            "ZIP symlink resolves outside destination"
        );
    }
    cancel.check()?;
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    for n in 0u64.. {
        let label = if n == 0 {
            stem.to_string()
        } else {
            format!("{stem} ({n})")
        };
        let destination = parent.join(label);
        match fs::create_dir(&destination) {
            Ok(()) => {
                if let Err(error) = fs::rename(stage.path(), &destination) {
                    let _ = fs::remove_dir(&destination);
                    return Err(error.into());
                }
                return Ok(destination);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    unreachable!()
}
