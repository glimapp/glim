//! Shared accounting and reclamation for owned buffers, independent of UI and I/O.
use std::{
    collections::HashMap,
    ops::Deref,
    sync::{Arc, Mutex, Weak},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceClass {
    State,
    Working,
    Cache,
}
impl ResourceClass {
    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Retention {
    Background,
    Viewport,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ResourceSnapshot {
    pub state_bytes: u64,
    pub working_bytes: u64,
    pub working_budget: u64,
    pub cache_bytes: u64,
    pub cache_budget: u64,
    pub evictions: u64,
}

trait Reclaim: Send + Sync {
    fn reclaim(&self) -> bool;
}

struct Entry {
    bytes: u64,
    class: ResourceClass,
    retention: Retention,
    accessed: u64,
    reclaim: Option<Weak<dyn Reclaim>>,
}

struct State {
    bytes: [u64; 3],
    entries: HashMap<u64, Entry>,
    next_id: u64,
    clock: u64,
    cache_budget: u64,
    working_budget: u64,
    evictions: u64,
}

#[derive(Clone)]
pub struct ResourcePool(Arc<Mutex<State>>);

impl ResourcePool {
    pub fn new(cache_budget: u64) -> Self {
        Self::with_budgets(cache_budget, u64::MAX)
    }

    pub fn with_budgets(cache_budget: u64, working_budget: u64) -> Self {
        Self(Arc::new(Mutex::new(State {
            bytes: [0; 3],
            entries: HashMap::new(),
            next_id: 0,
            clock: 0,
            cache_budget,
            working_budget,
            evictions: 0,
        })))
    }

    pub fn track(&self, bytes: u64, class: ResourceClass) -> ResourceLease {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        self.track_locked(&mut state, bytes, class)
    }

    /// Reserve before allocating. Competing producers cannot claim the same
    /// capacity; legacy tracked owners also count against available memory.
    pub fn reserve_working(
        &self,
        minimum: u64,
        desired: u64,
    ) -> Result<WorkingReservation, WorkingPressure> {
        assert!(minimum <= desired);
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let available = state
            .working_budget
            .saturating_sub(state.bytes[ResourceClass::Working.index()]);
        let bytes = desired.min(available);
        if bytes < minimum {
            return Err(WorkingPressure { minimum, available });
        }
        Ok(WorkingReservation {
            lease: self.track_locked(&mut state, bytes, ResourceClass::Working),
            bytes,
        })
    }

    fn track_locked(&self, state: &mut State, bytes: u64, class: ResourceClass) -> ResourceLease {
        let id = state.next_id;
        state.next_id = state.next_id.wrapping_add(1);
        state.clock = state.clock.wrapping_add(1);
        let accessed = state.clock;
        state.bytes[class.index()] = state.bytes[class.index()].saturating_add(bytes);
        state.entries.insert(
            id,
            Entry {
                bytes,
                class,
                accessed,
                retention: Retention::Viewport,
                reclaim: None,
            },
        );
        ResourceLease(Arc::new(Charge {
            pool: Arc::downgrade(&self.0),
            id,
        }))
    }

    pub fn cache<T: Send + Sync + 'static>(
        &self,
        value: T,
        bytes: u64,
        retention: Retention,
    ) -> CachedResource<T> {
        let lease = self.track(bytes, ResourceClass::Cache);
        let id = lease.0.id;
        let cell = Arc::new(CacheCell(Mutex::new(Some(Arc::new(ResourceValue {
            value,
            _lease: lease,
        })))));
        let erased: Arc<dyn Reclaim> = cell.clone();
        {
            let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
            let entry = state.entries.get_mut(&id).expect("new resource charge");
            entry.retention = retention;
            entry.reclaim = Some(Arc::downgrade(&erased));
        }
        let resource = CachedResource {
            cell,
            pool: self.clone(),
            id,
        };
        // The producer must be able to publish a new value even when existing
        // users pin the entire budget. Later allocations can reclaim it.
        let publishing = resource.get(retention);
        self.trim();
        drop(publishing);
        resource
    }

    pub fn snapshot(&self) -> ResourceSnapshot {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        ResourceSnapshot {
            state_bytes: state.bytes[ResourceClass::State.index()],
            working_bytes: state.bytes[ResourceClass::Working.index()],
            working_budget: state.working_budget,
            cache_bytes: state.bytes[ResourceClass::Cache.index()],
            cache_budget: state.cache_budget,
            evictions: state.evictions,
        }
    }

    pub fn trim(&self) {
        let snapshot = self.snapshot();
        if snapshot.cache_bytes <= snapshot.cache_budget {
            return;
        }
        let mut candidates = {
            let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
            state
                .entries
                .values()
                .filter_map(|entry| {
                    Some((
                        entry.retention,
                        entry.accessed,
                        entry.reclaim.as_ref()?.upgrade()?,
                    ))
                })
                .collect::<Vec<_>>()
        };
        candidates.sort_by_key(|(retention, accessed, _)| (*retention, *accessed));
        for (_, _, candidate) in candidates {
            let snapshot = self.snapshot();
            if snapshot.cache_bytes <= snapshot.cache_budget {
                break;
            }
            // Reclamation can destroy a charge; never call it with the pool locked.
            if candidate.reclaim() {
                let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
                state.evictions = state.evictions.saturating_add(1);
            }
        }
    }
}

#[derive(Debug)]
pub struct WorkingPressure {
    pub minimum: u64,
    pub available: u64,
}
impl std::fmt::Display for WorkingPressure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "Preview memory is currently in use. Retry after other previews finish or close.",
        )
    }
}
impl std::error::Error for WorkingPressure {}

/// The reservation follows its result through publication and display. It can
/// shrink, but growth requires another admission rather than an unchecked resize.
pub struct WorkingReservation {
    lease: ResourceLease,
    bytes: u64,
}
impl WorkingReservation {
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn shrink_to(&mut self, bytes: u64) {
        assert!(bytes <= self.bytes);
        self.lease.resize(bytes);
        self.bytes = bytes;
    }
}

struct Charge {
    pool: Weak<Mutex<State>>,
    id: u64,
}
impl Drop for Charge {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.upgrade() {
            let mut state = pool.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = state.entries.remove(&self.id) {
                state.bytes[entry.class.index()] =
                    state.bytes[entry.class.index()].saturating_sub(entry.bytes);
            }
        }
    }
}

#[derive(Clone)]
pub struct ResourceLease(Arc<Charge>);
impl ResourceLease {
    pub fn resize(&self, bytes: u64) {
        if let Some(pool) = self.0.pool.upgrade() {
            let mut state = pool.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = state.entries.get_mut(&self.0.id) {
                let class = entry.class.index();
                let old = std::mem::replace(&mut entry.bytes, bytes);
                state.bytes[class] = state.bytes[class].saturating_sub(old).saturating_add(bytes);
            }
        }
    }
}

pub struct ResourceValue<T> {
    value: T,
    _lease: ResourceLease,
}
impl<T> Deref for ResourceValue<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

struct CacheCell<T>(Mutex<Option<Arc<ResourceValue<T>>>>);
impl<T: Send + Sync> Reclaim for CacheCell<T> {
    fn reclaim(&self) -> bool {
        let Ok(mut value) = self.0.try_lock() else {
            return false;
        };
        if value
            .as_ref()
            .is_some_and(|value| Arc::strong_count(value) == 1)
        {
            value.take();
            true
        } else {
            false
        }
    }
}

/// A cache key survives eviction; a miss tells the owner to reconstruct from source.
pub struct CachedResource<T> {
    cell: Arc<CacheCell<T>>,
    pool: ResourcePool,
    id: u64,
}
impl<T> Clone for CachedResource<T> {
    fn clone(&self) -> Self {
        Self {
            cell: self.cell.clone(),
            pool: self.pool.clone(),
            id: self.id,
        }
    }
}
impl<T> CachedResource<T> {
    pub fn is_resident(&self) -> bool {
        self.cell
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    pub fn bytes(&self) -> u64 {
        self.pool
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .get(&self.id)
            .map_or(0, |entry| entry.bytes)
    }

    pub fn get(&self, retention: Retention) -> Option<Arc<ResourceValue<T>>> {
        let value = self
            .cell
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()?;
        let mut state = self.pool.0.lock().unwrap_or_else(|e| e.into_inner());
        state.clock = state.clock.wrapping_add(1);
        let clock = state.clock;
        if let Some(entry) = state.entries.get_mut(&self.id) {
            entry.accessed = clock;
            entry.retention = retention;
        }
        Some(value)
    }
}

/// Owned cell storage, excluding allocator metadata and spare outer-vector capacity.
pub fn text_rows_bytes(rows: &[Vec<String>]) -> u64 {
    std::mem::size_of_val(rows) as u64
        + rows
            .iter()
            .map(|row| {
                (row.capacity() * std::mem::size_of::<String>()) as u64
                    + row.iter().map(|cell| cell.capacity() as u64).sum::<u64>()
            })
            .sum::<u64>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_include_existing_owners_and_follow_retained_results() {
        let pool = ResourcePool::with_budgets(0, 12);
        let protected = pool.track(100, ResourceClass::State);
        let legacy = pool.track(3, ResourceClass::Working);
        let mut result = pool.reserve_working(4, 20).unwrap();
        assert_eq!(result.bytes(), 9);
        assert!(pool.reserve_working(1, 1).is_err());
        result.shrink_to(4);
        let next = pool.reserve_working(1, 20).unwrap();
        assert_eq!(next.bytes(), 5);
        assert_eq!(pool.snapshot().working_bytes, 12);
        drop(result);
        assert_eq!(pool.snapshot().working_bytes, 8);
        drop((next, legacy));
        assert_eq!(pool.snapshot().working_bytes, 0);
        assert_eq!(pool.snapshot().state_bytes, 100);
        drop(protected);
    }

    #[test]
    fn concurrent_producers_cannot_reserve_the_same_capacity() {
        let pool = ResourcePool::with_budgets(0, 32);
        let start = Arc::new(std::sync::Barrier::new(8));
        let results = std::thread::scope(|scope| {
            (0..8)
                .map(|_| {
                    let pool = &pool;
                    let start = &start;
                    scope.spawn(move || {
                        start.wait();
                        pool.reserve_working(8, 8)
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|thread| thread.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 4);
        assert_eq!(pool.snapshot().working_bytes, 32);
        drop(results);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }

    #[test]
    fn eviction_is_shared_and_preserves_live_users_and_unsaved_state() {
        let pool = ResourcePool::new(12);
        let draft = pool.track(100, ResourceClass::State);
        let first = pool.cache("first", 6, Retention::Viewport);
        let active = first.get(Retention::Viewport).unwrap();
        let background = pool.cache("background", 6, Retention::Background);
        let next = pool.cache("next", 6, Retention::Viewport);
        assert!(background.get(Retention::Background).is_none());
        assert_eq!(**active, "first");
        assert!(next.get(Retention::Viewport).is_some());
        assert_eq!(pool.snapshot().cache_bytes, 12);
        assert_eq!(pool.snapshot().state_bytes, 100);
        draft.resize(120);
        assert_eq!(pool.snapshot().state_bytes, 120);
        drop(first);
        assert_eq!(pool.snapshot().cache_bytes, 12);
        drop(active);
        assert_eq!(pool.snapshot().cache_bytes, 6);
        drop(draft);
        assert_eq!(pool.snapshot().state_bytes, 0);
    }

    #[test]
    fn a_full_pinned_budget_does_not_discard_a_result_before_publication() {
        let pool = ResourcePool::new(4);
        let first = pool.cache(1, 4, Retention::Viewport);
        let active = first.get(Retention::Viewport).unwrap();
        let next = pool.cache(2, 4, Retention::Viewport);
        assert_eq!(**next.get(Retention::Viewport).unwrap(), 2);
        assert_eq!(pool.snapshot().cache_bytes, 8);
        drop(active);
        pool.trim();
        assert!(first.get(Retention::Viewport).is_none());
        assert_eq!(pool.snapshot().cache_bytes, 4);
    }

    #[test]
    fn touching_an_entry_changes_lru_order_and_expired_keys_can_be_rebuilt() {
        let pool = ResourcePool::new(8);
        let first = pool.cache(1, 4, Retention::Viewport);
        let second = pool.cache(2, 4, Retention::Viewport);
        drop(first.get(Retention::Viewport));
        let third = pool.cache(3, 4, Retention::Viewport);
        assert!(second.get(Retention::Viewport).is_none());
        assert!(first.get(Retention::Viewport).is_some());
        assert!(third.get(Retention::Viewport).is_some());
        let rebuilt = pool.cache(2, 4, Retention::Viewport);
        assert_eq!(**rebuilt.get(Retention::Viewport).unwrap(), 2);
        assert!(pool.snapshot().cache_bytes <= 8);
    }
}
