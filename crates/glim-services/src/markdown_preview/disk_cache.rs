//! Optional persistent results. Call only from the diagram background job.
use super::{Diagram, DiagramImage};
use crate::limits;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, FileTimes},
    io::{self, Read, Write},
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const MAGIC: &[u8; 8] = b"GLIMDIA1";
const HEADER_BYTES: usize = 84;
const SUFFIX: &str = ".glim-diagram";

struct DiskCache {
    directory: PathBuf,
    namespace: [u8; 32],
    budget: u64,
    initialized: bool,
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
        let home = PathBuf::from(std::env::var_os("HOME").ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Home directory unavailable")
        })?);
        *cache = Some(DiskCache {
            directory: home.join("Library/Caches/Glim/Diagrams"),
            namespace: environment(&home, cancelled)?,
            budget: limits::DIAGRAM_DISK_CACHE_BYTES,
            initialized: false,
        });
    }
    operation(cache.as_mut().unwrap())
}

pub(super) fn load(diagram: &Diagram, edge: u32, cancelled: &AtomicBool) -> Option<DiagramImage> {
    match with_cache(cancelled, |cache| cache.load(diagram, edge, cancelled)) {
        Ok(image) => image,
        Err(error) => {
            log::debug!("Diagram disk cache read skipped: {error}");
            None
        }
    }
}

pub(super) fn remember(diagram: &Diagram, edge: u32, image: &DiagramImage, cancelled: &AtomicBool) {
    if let Err(error) = with_cache(cancelled, |cache| {
        cache.remember(diagram, edge, image, cancelled)
    }) {
        log::debug!("Diagram disk cache write skipped: {error}");
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

    fn path(&self, key: &[u8; 32]) -> PathBuf {
        use std::fmt::Write;
        let mut name = String::with_capacity(64 + SUFFIX.len());
        for byte in key {
            write!(&mut name, "{byte:02x}").unwrap();
        }
        name.push_str(SUFFIX);
        self.directory.join(name)
    }

    fn load(
        &mut self,
        diagram: &Diagram,
        edge: u32,
        cancelled: &AtomicBool,
    ) -> io::Result<Option<DiagramImage>> {
        if !self.initialized {
            self.prune(0, cancelled)?;
            self.initialized = true;
        }
        let key = self.key(diagram, edge);
        let path = self.path(&key);
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let maximum = HEADER_BYTES as u64 + limits::DIAGRAM_AGGREGATE_ENCODED_BYTES as u64;
        if file.metadata()?.len() > maximum {
            let _ = fs::remove_file(path);
            return Ok(None);
        }
        let mut bytes = Vec::new();
        let mut reader = (&file).take(maximum + 1);
        let mut buffer = vec![0; limits::IO_BUFFER_BYTES];
        loop {
            check_cancelled(cancelled)?;
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..count]);
        }
        check_cancelled(cancelled)?;
        let image = decode(&bytes, &key, edge);
        if image.is_none() {
            let _ = fs::remove_file(path);
        } else {
            let _ = file.set_times(FileTimes::new().set_modified(SystemTime::now()));
        }
        Ok(image)
    }

    fn remember(
        &mut self,
        diagram: &Diagram,
        edge: u32,
        image: &DiagramImage,
        cancelled: &AtomicBool,
    ) -> io::Result<()> {
        check_cancelled(cancelled)?;
        let bytes = HEADER_BYTES as u64 + image.png.len() as u64;
        if bytes > self.budget || image.png.len() > limits::DIAGRAM_AGGREGATE_ENCODED_BYTES {
            return Ok(());
        }
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.directory)?;
        self.prune(bytes, cancelled)?;
        let key = self.key(diagram, edge);
        let mut header = Vec::with_capacity(HEADER_BYTES);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&key);
        header.extend_from_slice(&image.width.to_le_bytes());
        header.extend_from_slice(&image.height.to_le_bytes());
        header.extend_from_slice(&image.baseline.to_le_bytes());
        let mut digest = Sha256::new();
        digest.update(&header);
        digest.update(&image.png);
        header.extend_from_slice(&digest.finalize());
        let mut temporary = tempfile::Builder::new()
            .prefix(".pending-")
            .tempfile_in(&self.directory)?;
        temporary.write_all(&header)?;
        for chunk in image.png.chunks(limits::IO_BUFFER_BYTES) {
            check_cancelled(cancelled)?;
            temporary.write_all(chunk)?;
        }
        check_cancelled(cancelled)?;
        temporary
            .persist(self.path(&key))
            .map_err(|error| error.error)?;
        Ok(())
    }

    fn prune(&self, incoming: u64, cancelled: &AtomicBool) -> io::Result<()> {
        type Candidate = (SystemTime, PathBuf, u64);
        fn cost(path: &Path) -> u64 {
            (std::mem::size_of::<Candidate>() + path.as_os_str().as_encoded_bytes().len()) as u64
                * 2
        }
        loop {
            let directory = match fs::read_dir(&self.directory) {
                Ok(directory) => directory,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error),
            };
            // Retain only a bounded window of oldest candidates, then rescan if needed.
            let mut oldest = std::collections::BinaryHeap::new();
            let mut working_bytes = 0_u64;
            let mut used = incoming;
            for entry in directory {
                check_cancelled(cancelled)?;
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let cache_file = name.strip_suffix(SUFFIX).is_some_and(|stem| {
                    stem.len() == 64 && stem.bytes().all(|byte| byte.is_ascii_hexdigit())
                });
                if !cache_file && !name.starts_with(".pending-") {
                    continue;
                }
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let metadata = match entry.metadata() {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error),
                };
                used = used.saturating_add(metadata.len());
                let path = entry.path();
                working_bytes = working_bytes.saturating_add(cost(&path));
                oldest.push((
                    metadata.modified().unwrap_or(UNIX_EPOCH),
                    path,
                    metadata.len(),
                ));
                while working_bytes > limits::IO_CACHE_BYTES {
                    let (_, path, _) = oldest.pop().unwrap();
                    working_bytes = working_bytes.saturating_sub(cost(&path));
                }
            }
            if used <= self.budget {
                return Ok(());
            }
            if oldest.is_empty() {
                return Err(io::Error::other("Cannot free diagram cache capacity"));
            }
            for (_, path, bytes) in oldest.into_sorted_vec() {
                if used <= self.budget {
                    return Ok(());
                }
                check_cancelled(cancelled)?;
                match fs::remove_file(path) {
                    Ok(()) => used = used.saturating_sub(bytes),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        used = used.saturating_sub(bytes)
                    }
                    Err(error) => return Err(error),
                }
            }
        }
    }
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

    fn cache(directory: &Path, budget: u64) -> DiskCache {
        DiskCache {
            directory: directory.to_path_buf(),
            namespace: [1; 32],
            budget,
            initialized: false,
        }
    }

    #[test]
    fn reopening_restores_results_and_isolates_source_engine_and_resolution() {
        let directory = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(false);
        let diagram = diagram("graph LR; A-->B");
        let expected = image();
        cache(directory.path(), u64::MAX)
            .remember(&diagram, 32, &expected, &cancelled)
            .unwrap();
        let mut reopened = cache(directory.path(), u64::MAX);
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
    fn corrupt_entries_are_removed_and_can_be_rebuilt() {
        let directory = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(false);
        let diagram = diagram("graph LR; A-->B");
        let image = image();
        let mut cache = cache(directory.path(), u64::MAX);
        cache.remember(&diagram, 32, &image, &cancelled).unwrap();
        let path = cache.path(&cache.key(&diagram, 32));
        let valid = fs::read(&path).unwrap();
        for broken in [valid[..HEADER_BYTES - 1].to_vec(), {
            let mut damaged = valid.clone();
            damaged[HEADER_BYTES + 30] ^= 1;
            damaged
        }] {
            fs::write(&path, broken).unwrap();
            assert!(cache.load(&diagram, 32, &cancelled).unwrap().is_none());
            assert!(!path.exists());
        }
        cache.remember(&diagram, 32, &image, &cancelled).unwrap();
        assert!(cache.load(&diagram, 32, &cancelled).unwrap().is_some());
    }

    #[test]
    fn capacity_eviction_respects_recent_reads_and_cancelled_writes() {
        let directory = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(false);
        let image = image();
        let bytes = (HEADER_BYTES + image.png.len()) as u64;
        let mut cache = cache(directory.path(), bytes * 2);
        let a = diagram("a");
        let b = diagram("b");
        let c = diagram("c");
        for (index, diagram) in [&a, &b].into_iter().enumerate() {
            cache.remember(diagram, 32, &image, &cancelled).unwrap();
            File::open(cache.path(&cache.key(diagram, 32)))
                .unwrap()
                .set_times(
                    FileTimes::new()
                        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(index as u64)),
                )
                .unwrap();
        }
        assert!(cache.load(&a, 32, &cancelled).unwrap().is_some());
        cache.remember(&c, 32, &image, &cancelled).unwrap();
        assert!(cache.load(&a, 32, &cancelled).unwrap().is_some());
        assert!(cache.load(&b, 32, &cancelled).unwrap().is_none());
        assert!(cache.load(&c, 32, &cancelled).unwrap().is_some());
        cancelled.store(true, Ordering::Relaxed);
        assert_eq!(
            cache
                .remember(&b, 32, &image, &cancelled)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert!(!cache.path(&cache.key(&b, 32)).exists());
        let total: u64 = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().metadata().unwrap().len())
            .sum();
        assert!(total <= bytes * 2);
    }
}
