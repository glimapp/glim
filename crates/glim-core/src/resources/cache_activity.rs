use std::sync::atomic::{AtomicU64, Ordering};

/// Count consumer lookups, independently of cache publication and resident peeks.
#[derive(Default)]
pub struct CacheActivity {
    hits: AtomicU64,
    misses: AtomicU64,
    pending: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheActivitySnapshot {
    pub hits: u64,
    pub misses: u64,
    /// A previous miss already queued preparation; exclude from hit-rate denominators.
    pub pending: u64,
}

pub enum CacheLookup {
    Hit,
    Miss,
    Pending,
}

impl CacheActivity {
    pub const fn new() -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            pending: AtomicU64::new(0),
        }
    }

    pub fn record(&self, outcome: CacheLookup) {
        match outcome {
            CacheLookup::Hit => &self.hits,
            CacheLookup::Miss => &self.misses,
            CacheLookup::Pending => &self.pending,
        }
        .fetch_add(1, Ordering::Relaxed);
    }

    /// Cumulative counters; concurrent requests can advance between field reads.
    pub fn snapshot(&self) -> CacheActivitySnapshot {
        CacheActivitySnapshot {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            pending: self.pending.load(Ordering::Relaxed),
        }
    }
}
