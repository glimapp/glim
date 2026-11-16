//! Remote thumbnails survive reader disposal and application restarts in SQLite.
use crate::{limits, storage::cache::CacheStore};
use sha2::{Digest, Sha256};

fn key(url: &str) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"inline-image-v1");
    hash.update(limits::INLINE_IMAGE_PREVIEW_EDGE.to_le_bytes());
    hash.update(url.as_bytes());
    hash.finalize().into()
}

pub(super) fn load(url: &str) -> Option<Vec<u8>> {
    let maximum = u64::from(limits::INLINE_IMAGE_PREVIEW_EDGE).pow(2) * 4
        + limits::INLINE_IMAGE_OUTPUT_OVERHEAD_BYTES;
    match CacheStore::open()
        .and_then(|cache| cache.load("remote-image", &key(url), usize::try_from(maximum)?))
    {
        Ok(png) => png,
        Err(error) => {
            log::warn!("Remote image cache read skipped: {error:#}");
            None
        }
    }
}

pub(super) fn remember(url: String, png: &[u8]) {
    if let Err(error) =
        CacheStore::open().and_then(|cache| cache.remember("remote-image", &key(&url), png))
    {
        log::warn!("Remote image cache write skipped: {error:#}");
    }
}
