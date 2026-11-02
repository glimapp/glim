//! Archive browsing and cancellable extraction into a new, isolated directory.
use crate::work::Cancellation;
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(serde::Deserialize)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub directory: bool,
}
pub struct Archive {
    pub path: PathBuf,
    pub format: String,
    pub entries: Vec<Entry>,
    pub bytes: u64,
}

const EXTENSIONS: &[&str] = &[
    "zip", "tar", "tgz", "tbz", "tbz2", "txz", "tlz", "tzst", "taz", "gz", "bz2", "xz", "lzma",
    "lz", "lz4", "zst", "zstd", "z", "7z", "rar", "cab", "cpio", "ar", "a", "xar", "iso", "lha",
    "lzh",
];

pub fn supports(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

fn stem(path: &Path) -> String {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let lower = name.to_ascii_lowercase();
    for extension in ["gz", "bz2", "xz", "lzma", "lz", "lz4", "zst", "zstd", "z"] {
        let compound = format!(".tar.{extension}");
        if lower.ends_with(&compound) {
            return name[..name.len() - compound.len()].to_owned();
        }
    }
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn native(
    path: &Path,
    destination: Option<&Path>,
    cancel: &Cancellation,
) -> Result<tempfile::NamedTempFile> {
    use std::os::unix::fs::PermissionsExt;
    cancel.check()?;
    let directory = tempfile::tempdir()?;
    let helper = directory.path().join("glim-archive");
    fs::write(
        &helper,
        include_bytes!(concat!(env!("OUT_DIR"), "/glim-archive")),
    )?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700))?;
    // Apple's libarchive delegates these codecs to optional command-line tools.
    // Decode into a temporary stream with bundled codecs instead.
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let decoded = if matches!(extension.as_str(), "zst" | "zstd" | "tzst" | "lz4") {
        let mut output = tempfile::NamedTempFile::new()?;
        let input = File::open(path)?;
        if extension == "lz4" {
            let mut input = BufReader::new(input);
            while !input.fill_buf()?.is_empty() {
                cancel.check()?;
                cancel.copy(
                    lz4_flex::frame::FrameDecoder::new(&mut input),
                    output.as_file_mut(),
                )?;
            }
        } else {
            cancel.copy(
                zstd::stream::read::Decoder::new(input)?,
                output.as_file_mut(),
            )?;
        }
        Some(output)
    } else {
        None
    };
    let output = tempfile::NamedTempFile::new()?;
    let mut errors = tempfile::NamedTempFile::new()?;
    let mut command = Command::new(helper);
    command
        .arg(if destination.is_some() {
            "extract"
        } else {
            "list"
        })
        .arg(
            decoded
                .as_ref()
                .map_or(path, |file| file.path())
                .canonicalize()?,
        )
        .arg(destination.unwrap_or(directory.path()))
        .arg(stem(path))
        .arg(crate::limits::IO_BUFFER_BYTES.to_string())
        .arg(
            if matches!(
                extension.as_str(),
                "gz" | "bz2" | "xz" | "lzma" | "lz" | "lz4" | "zst" | "zstd" | "z"
            ) {
                "1"
            } else {
                "0"
            },
        )
        .stdin(Stdio::null())
        .stdout(output.reopen()?)
        .stderr(errors.reopen()?);
    let status = crate::work::run(&mut command, cancel)?;
    if !status.success() {
        let mut detail = String::new();
        errors
            .as_file_mut()
            .take(crate::limits::IO_BUFFER_BYTES as u64)
            .read_to_string(&mut detail)?;
        anyhow::bail!("Archive operation failed: {}", detail.trim());
    }
    Ok(output)
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
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        let index = native(path, None, cancel)?;
        let mut entries = Vec::new();
        let mut bytes = 0u64;
        for line in BufReader::new(index.reopen()?).lines() {
            cancel.check()?;
            let mut entry: Entry = serde_json::from_str(&line?)?;
            entry.name = entry.name.trim_start_matches("./").to_owned();
            if entry.name.is_empty() || entry.name == "." {
                continue;
            }
            if entry.directory && !entry.name.ends_with('/') {
                entry.name.push('/');
            }
            bytes = bytes.saturating_add(entry.size);
            entries.push(entry);
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        let filename = path.file_name().unwrap_or_default().to_string_lossy();
        let base = stem(path);
        let format = filename
            .get(base.len()..)
            .unwrap_or_default()
            .trim_start_matches('.')
            .to_uppercase();
        return Ok(Archive {
            path: path.to_path_buf(),
            format,
            entries,
            bytes,
        });
    }
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
        format: "ZIP".to_owned(),
        entries,
        bytes,
    })
}
pub fn extract(path: &Path, parent: &Path, cancel: &Cancellation) -> Result<PathBuf> {
    let stage = tempfile::Builder::new()
        .prefix(".glim-extract-")
        .tempdir_in(parent)?;
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        extract_zip(path, stage.path(), cancel)?;
    } else {
        native(path, Some(stage.path()), cancel)?;
        validate_links(stage.path(), &stage.path().canonicalize()?, cancel)?;
    }
    cancel.check()?;
    let stem = stem(path);
    for n in 0u64.. {
        let label = if n == 0 {
            stem.clone()
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

fn validate_links(directory: &Path, root: &Path, cancel: &Cancellation) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        cancel.check()?;
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            ensure!(
                entry.path().canonicalize()?.starts_with(root),
                "Archive symlink resolves outside destination"
            );
        } else if kind.is_dir() {
            validate_links(&entry.path(), root, cancel)?;
        }
    }
    Ok(())
}

fn extract_zip(path: &Path, stage: &Path, cancel: &Cancellation) -> Result<()> {
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    // Links are created last, so no file writes can traverse an archive-created link.
    let mut links = Vec::new();
    for i in 0..zip.len() {
        cancel.check()?;
        let mut entry = zip.by_index(i)?;
        let relative = safe(&name(&entry))?;
        let target = stage.join(&relative);
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
            parent.canonicalize()?.starts_with(stage.canonicalize()?),
            "ZIP symlink parent escapes destination"
        );
        std::os::unix::fs::symlink(value, target)?;
    }
    for (target, _) in &links {
        cancel.check()?;
        ensure!(
            target.canonicalize()?.starts_with(stage.canonicalize()?),
            "ZIP symlink resolves outside destination"
        );
    }
    cancel.check()?;
    Ok(())
}
