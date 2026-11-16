//! Persistent diagram results in the shared SQLite cache; no automatic eviction.
use super::{Diagram, DiagramImage};
use crate::{limits, storage::cache::CacheStore};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::UNIX_EPOCH,
};

const MAGIC: &[u8; 8] = b"GLIMDIA1";
const HEADER_BYTES: usize = 84;
const SUFFIX: &str = ".glim-diagram";

struct DiskCache {
    database: CacheStore,
    namespace: [u8; 32],
}

fn check_cancelled(cancelled: &AtomicBool) -> io::Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Diagram cache cancelled",
        ))
    } else {
        Ok(())
    }
}

fn environment(home: &Path, cancelled: &AtomicBool) -> io::Result<[u8; 32]> {
    let mut digest = Sha256::new();
    digest.update(env!("GLIM_DIAGRAM_CACHE_FINGERPRINT"));
    digest.update(std::env::consts::OS);
    digest.update(std::env::consts::ARCH);
    digest.update(fs::read("/System/Library/CoreServices/SystemVersion.plist").unwrap_or_default());
    let mut directories = vec![
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/Library/Fonts"),
        home.join("Library/Fonts"),
    ];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(directory) = directories.pop() {
        check_cancelled(cancelled)?;
        let canonical = match directory.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if !seen.insert(canonical) {
            continue;
        }
        let mut entries = fs::read_dir(directory)?.collect::<io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            check_cancelled(cancelled)?;
            let path = entry.path();
            let metadata = match fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            digest.update(path.as_os_str().as_encoded_bytes());
            digest.update(metadata.len().to_le_bytes());
            digest.update(
                metadata
                    .modified()?
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
                    .to_le_bytes(),
            );
            if metadata.is_dir() {
                directories.push(path);
            }
        }
    }
    Ok(digest.finalize().into())
}

fn with_cache<R>(
    cancelled: &AtomicBool,
    operation: impl FnOnce(&mut DiskCache) -> io::Result<R>,
) -> io::Result<R> {
    static CACHE: OnceLock<Mutex<Option<DiskCache>>> = OnceLock::new();
    check_cancelled(cancelled)?;
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if cache.is_none() {
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory unavailable"))?;
        *cache = Some(DiskCache {
            database: CacheStore::open().map_err(io::Error::other)?,
            namespace: environment(&home, cancelled)?,
        });
    }
    operation(cache.as_mut().unwrap())
}

pub(super) fn load(diagram: &Diagram, edge: u32, cancelled: &AtomicBool) -> Option<DiagramImage> {
    match with_cache(cancelled, |cache| cache.load(diagram, edge, cancelled)) {
        Ok(image) => image,
        Err(error) => {
            log::debug!("Diagram SQLite cache read skipped: {error}");
            None
        }
    }
}

pub(super) fn remember(diagram: &Diagram, edge: u32, image: &DiagramImage, cancelled: &AtomicBool) {
    if let Err(error) = with_cache(cancelled, |cache| {
        cache.remember(diagram, edge, image, cancelled)
    }) {
        log::debug!("Diagram SQLite cache write skipped: {error}");
    }
}

impl DiskCache {
    fn key(&self, diagram: &Diagram, edge: u32) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(MAGIC);
        digest.update(self.namespace);
        digest.update(edge.to_le_bytes());
        digest.update((diagram.kind.len() as u64).to_le_bytes());
        digest.update(diagram.kind.as_bytes());
        digest.update(diagram.source.as_bytes());
        digest.finalize().into()
    }

    fn load(
        &self,
        diagram: &Diagram,
        edge: u32,
        cancelled: &AtomicBool,
    ) -> io::Result<Option<DiagramImage>> {
        check_cancelled(cancelled)?;
        let key = self.key(diagram, edge);
        let bytes = self
            .database
            .load(
                "diagram",
                &key,
                HEADER_BYTES + limits::DIAGRAM_AGGREGATE_ENCODED_BYTES,
            )
            .map_err(io::Error::other)?;
        check_cancelled(cancelled)?;
        Ok(bytes.and_then(|bytes| decode(&bytes, &key, edge)))
    }

    fn remember(
        &self,
        diagram: &Diagram,
        edge: u32,
        image: &DiagramImage,
        cancelled: &AtomicBool,
    ) -> io::Result<()> {
        check_cancelled(cancelled)?;
        if image.png.len() > limits::DIAGRAM_AGGREGATE_ENCODED_BYTES {
            return Ok(());
        }
        let key = self.key(diagram, edge);
        let bytes = encode(&key, image);
        check_cancelled(cancelled)?;
        self.database
            .remember("diagram", &key, &bytes)
            .map_err(io::Error::other)
    }
}

fn encode(key: &[u8; 32], image: &DiagramImage) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER_BYTES + image.png.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(key);
    bytes.extend_from_slice(&image.width.to_le_bytes());
    bytes.extend_from_slice(&image.height.to_le_bytes());
    bytes.extend_from_slice(&image.baseline.to_le_bytes());
    let mut digest = Sha256::new();
    digest.update(&bytes);
    digest.update(&image.png);
    bytes.extend_from_slice(&digest.finalize());
    bytes.extend_from_slice(&image.png);
    bytes
}

pub(super) fn migrate_legacy() -> io::Result<()> {
    let Some(home) = std::env::var_os("HOME") else {
        return Ok(());
    };
    let root = std::path::PathBuf::from(home).join("Library/Caches/Glim");
    let directory = root.join("Diagrams");
    if !directory.is_dir() {
        return Ok(());
    }
    let database = CacheStore::open().map_err(io::Error::other)?;
    migrate_directory(&directory, &database)?;
    let _ = fs::remove_dir(&directory);
    let _ = fs::remove_dir(&root);
    Ok(())
}

fn migrate_directory(directory: &Path, database: &CacheStore) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(stem) = name.to_str().and_then(|name| name.strip_suffix(SUFFIX)) else {
            continue;
        };
        if stem.len() != 64
            || !stem.bytes().all(|b| b.is_ascii_hexdigit())
            || !entry.file_type()?.is_file()
        {
            continue;
        }
        let mut key = [0; 32];
        for (index, byte) in key.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&stem[index * 2..index * 2 + 2], 16)
                .map_err(io::Error::other)?;
        }
        let maximum = HEADER_BYTES + limits::DIAGRAM_AGGREGATE_ENCODED_BYTES;
        let mut bytes = Vec::new();
        File::open(entry.path())?
            .take(maximum as u64 + 1)
            .read_to_end(&mut bytes)?;
        if decode(&bytes, &key, u32::MAX).is_none() {
            log::warn!("Invalid legacy diagram cache entry preserved");
            continue;
        }
        database
            .remember("diagram", &key, &bytes)
            .map_err(io::Error::other)?;
        let stored = database
            .load("diagram", &key, maximum)
            .map_err(io::Error::other)?;
        if stored.as_deref() != Some(bytes.as_slice()) {
            return Err(io::Error::other(
                "Diagram cache migration verification failed",
            ));
        }
        fs::remove_file(entry.path())?;
    }
    Ok(())
}

fn decode(bytes: &[u8], key: &[u8; 32], edge: u32) -> Option<DiagramImage> {
    if bytes.len() < HEADER_BYTES + 33
        || bytes.len() > HEADER_BYTES + limits::DIAGRAM_AGGREGATE_ENCODED_BYTES
        || &bytes[..8] != MAGIC
        || &bytes[8..40] != key
    {
        return None;
    }
    let width = u32::from_le_bytes(bytes[40..44].try_into().ok()?);
    let height = u32::from_le_bytes(bytes[44..48].try_into().ok()?);
    let baseline = f32::from_le_bytes(bytes[48..52].try_into().ok()?);
    if width == 0
        || height == 0
        || width > edge
        || height > edge
        || !baseline.is_finite()
        || baseline < 0.
        || baseline > height as f32
    {
        return None;
    }
    let png = &bytes[HEADER_BYTES..];
    if &png[..8] != b"\x89PNG\r\n\x1a\n"
        || png[8..12] != 13_u32.to_be_bytes()
        || &png[12..16] != b"IHDR"
        || !png.ends_with(&[0, 0, 0, 0, b'I', b'E', b'N', b'D', 174, 66, 96, 130])
        || u32::from_be_bytes(png[16..20].try_into().ok()?) != width
        || u32::from_be_bytes(png[20..24].try_into().ok()?) != height
    {
        return None;
    }
    let mut digest = Sha256::new();
    digest.update(&bytes[..52]);
    digest.update(png);
    if digest.finalize().as_slice() != &bytes[52..HEADER_BYTES] {
        return None;
    }
    Some(DiagramImage {
        png: png.to_vec(),
        width,
        height,
        baseline,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagram(source: &str) -> Diagram {
        Diagram {
            kind: "mermaid".into(),
            source: source.into(),
        }
    }
    fn image() -> DiagramImage {
        let mut png = io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 1)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        DiagramImage {
            png: png.into_inner(),
            width: 2,
            height: 1,
            baseline: 0.5,
        }
    }
    fn cache(directory: &Path) -> DiskCache {
        DiskCache {
            database: CacheStore::open_at(directory).unwrap(),
            namespace: [1; 32],
        }
    }

    #[test]
    fn reopening_restores_results_and_isolates_source_engine_and_resolution() {
        let directory = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(false);
        let diagram = diagram("graph LR; A-->B");
        let expected = image();
        cache(directory.path())
            .remember(&diagram, 32, &expected, &cancelled)
            .unwrap();
        let mut reopened = cache(directory.path());
        let actual = reopened.load(&diagram, 32, &cancelled).unwrap().unwrap();
        assert_eq!(actual.png, expected.png);
        assert_eq!((actual.width, actual.height, actual.baseline), (2, 1, 0.5));
        for changed in [
            Diagram {
                source: "graph LR; A-->C".into(),
                ..diagram.clone()
            },
            Diagram {
                kind: "dot".into(),
                ..diagram.clone()
            },
        ] {
            assert!(reopened.load(&changed, 32, &cancelled).unwrap().is_none());
        }
        assert!(reopened.load(&diagram, 64, &cancelled).unwrap().is_none());
        reopened.namespace = [2; 32];
        assert!(reopened.load(&diagram, 32, &cancelled).unwrap().is_none());
    }

    #[test]
    fn corrupt_entries_miss_and_can_be_rebuilt() {
        let directory = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(false);
        let diagram = diagram("graph LR; A-->B");
        let image = image();
        let cache = cache(directory.path());
        let key = cache.key(&diagram, 32);
        let valid = encode(&key, &image);
        for broken in [valid[..HEADER_BYTES - 1].to_vec(), {
            let mut damaged = valid.clone();
            damaged[HEADER_BYTES + 30] ^= 1;
            damaged
        }] {
            cache.database.remember("diagram", &key, &broken).unwrap();
            assert!(cache.load(&diagram, 32, &cancelled).unwrap().is_none());
        }
        cache.remember(&diagram, 32, &image, &cancelled).unwrap();
        assert!(cache.load(&diagram, 32, &cancelled).unwrap().is_some());
    }

    #[test]
    fn new_results_keep_older_entries_and_cancelled_writes_do_not_publish() {
        let directory = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(false);
        let image = image();
        let cache = cache(directory.path());
        for name in ["a", "b", "c"] {
            cache
                .remember(&diagram(name), 32, &image, &cancelled)
                .unwrap();
        }
        for name in ["a", "b", "c"] {
            assert!(
                cache
                    .load(&diagram(name), 32, &cancelled)
                    .unwrap()
                    .is_some()
            );
        }
        cancelled.store(true, Ordering::Relaxed);
        assert_eq!(
            cache
                .remember(&diagram("d"), 32, &image, &cancelled)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        cancelled.store(false, Ordering::Relaxed);
        assert!(cache.load(&diagram("d"), 32, &cancelled).unwrap().is_none());
    }

    #[test]
    fn legacy_files_are_removed_only_after_verified_sqlite_import() {
        let directory = tempfile::tempdir().unwrap();
        let legacy = directory.path().join("Diagrams");
        fs::create_dir(&legacy).unwrap();
        let cache = cache(directory.path());
        let key = cache.key(&diagram("old"), 32);
        let name: String = key.iter().map(|b| format!("{b:02x}")).collect();
        let source = legacy.join(format!("{name}{SUFFIX}"));
        fs::write(&source, encode(&key, &image())).unwrap();
        let unknown = legacy.join("unrelated.txt");
        fs::write(&unknown, "keep").unwrap();
        migrate_directory(&legacy, &cache.database).unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read_to_string(unknown).unwrap(), "keep");
        assert!(
            cache
                .load(&diagram("old"), 32, &AtomicBool::new(false))
                .unwrap()
                .is_some()
        );
    }
}
