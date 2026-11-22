use std::{
    fs::{File, Metadata},
    io,
    path::Path,
};

#[cfg(unix)]
pub(crate) fn from_file(_file: &File, metadata: &Metadata) -> io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
pub(crate) fn from_file(file: &File, _metadata: &Metadata) -> io::Result<(u64, u64)> {
    let info = winapi_util::file::information(file)?;
    Ok((info.volume_serial_number(), info.file_index()))
}

#[cfg(unix)]
pub(crate) fn from_path(_path: &Path, metadata: &Metadata) -> io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
pub(crate) fn from_path(path: &Path, metadata: &Metadata) -> io::Result<(u64, u64)> {
    from_file(&File::open(path)?, metadata)
}

#[cfg(windows)]
pub(crate) fn from_entry(path: &Path, metadata: &Metadata) -> io::Result<(u64, u64)> {
    use std::os::windows::fs::OpenOptionsExt;
    const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let file = std::fs::OpenOptions::new()
        .access_mode(0)
        .custom_flags(OPEN_REPARSE_POINT | BACKUP_SEMANTICS)
        .open(path)?;
    from_file(&file, metadata)
}

#[cfg(unix)]
pub(crate) use from_path as from_entry;
