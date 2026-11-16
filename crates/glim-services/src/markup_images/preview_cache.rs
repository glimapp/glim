//! Session-only, disk-backed thumbnails; source downloads and failures are not retained.
use std::{
    collections::VecDeque,
    fs::File,
    io::Write,
    os::unix::fs::FileExt,
    sync::{Arc, Mutex},
};

struct Entry {
    url: String,
    file: Arc<File>,
    bytes: u64,
}

static CACHE: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());

pub(super) fn load(url: &str) -> Option<Vec<u8>> {
    let file = {
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let index = cache.iter().position(|entry| entry.url == url)?;
        let entry = cache.remove(index)?;
        let file = entry.file.clone();
        cache.push_back(entry);
        file
    };
    let read = || -> std::io::Result<_> {
        let mut png = vec![
            0;
            file.metadata()?
                .len()
                .try_into()
                .map_err(std::io::Error::other)?
        ];
        file.read_exact_at(&mut png, 0)?;
        Ok(png)
    };
    match read() {
        Ok(png) => Some(png),
        Err(_) => {
            let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
            cache.retain(|entry| !Arc::ptr_eq(&entry.file, &file));
            None
        }
    }
}

pub(super) fn remember(url: String, png: &[u8]) {
    let bytes = (url.len() + png.len() + std::mem::size_of::<Entry>()) as u64;
    let budget = crate::limits::REMOTE_IMAGE_PREVIEW_CACHE_BYTES;
    if bytes > budget {
        return;
    }
    let write = || -> std::io::Result<_> {
        // Anonymous files leave no orphaned paths, including after a crash.
        let mut file = tempfile::tempfile()?;
        file.write_all(png)?;
        Ok(Arc::new(file))
    };
    let Ok(file) = write() else {
        return;
    };
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.retain(|entry| entry.url != url);
    let mut used: u64 = cache.iter().map(|entry| entry.bytes).sum();
    while used.saturating_add(bytes) > budget
        || cache.len() >= crate::limits::REMOTE_IMAGE_PREVIEW_CACHE_ENTRIES
    {
        let Some(entry) = cache.pop_front() else {
            break;
        };
        used = used.saturating_sub(entry.bytes);
    }
    cache.push_back(Entry { url, file, bytes });
}
