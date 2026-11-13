//! Process-wide policy for buffers owned by Glim. Framework allocations are separate.
pub use glim_core::resources::*;
use std::{
    io::Read,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

static HEADROOM: AtomicU64 = AtomicU64::new(0);

fn headroom(total: u64, used: u64) -> u64 {
    let ceiling = total.saturating_mul(crate::limits::RESOURCES_SYSTEM_USED_PERCENT.min(100)) / 100;
    ceiling.saturating_sub(used)
}

fn start_memory_monitor() {
    static MONITOR: OnceLock<()> = OnceLock::new();
    MONITOR.get_or_init(|| {
        let mut system = sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::new().with_memory(sysinfo::MemoryRefreshKind::new().with_ram()),
        );
        HEADROOM.store(
            headroom(system.total_memory(), system.used_memory()),
            Ordering::Relaxed,
        );
        std::thread::Builder::new()
            .name("reader-memory".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(
                        crate::limits::RESOURCES_MEMORY_POLL_MS,
                    ));
                    system.refresh_memory_specifics(sysinfo::MemoryRefreshKind::new().with_ram());
                    let available = headroom(system.total_memory(), system.used_memory());
                    HEADROOM.store(available, Ordering::Relaxed);
                    if available == 0 {
                        pool().trim_to(0);
                    }
                }
            })
            .expect("Start reader memory monitor");
    });
}

/// A conservative admission includes outstanding managed reservations as well
/// as the system's actual use, covering workers whose allocations are pending.
pub fn available_memory() -> u64 {
    start_memory_monitor();
    HEADROOM.load(Ordering::Relaxed)
}

pub fn is_memory_pressure(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.is::<WorkingPressure>())
}

pub fn check_memory() -> Result<(), WorkingPressure> {
    let available = available_memory();
    if available == 0 {
        Err(WorkingPressure {
            minimum: 1,
            available,
        })
    } else {
        Ok(())
    }
}

/// Parsers can hold a DOM, strings and reallocation overlap simultaneously.
/// This envelope scales with input instead of imposing an input-size ceiling.
pub(crate) fn reserve_parser(input_bytes: usize) -> Result<WorkingReservation, WorkingPressure> {
    reserve_parser_with_pool(input_bytes, pool())
}

pub(crate) fn reserve_parser_with_pool(
    input_bytes: usize,
    pool: &ResourcePool,
) -> Result<WorkingReservation, WorkingPressure> {
    let desired =
        (input_bytes as u64).saturating_mul((2 * std::mem::size_of::<serde_json::Value>()) as u64);
    pool.reserve_working(desired, desired)
}

/// Read the whole input when it fits, reserving vector growth before allocation.
pub fn read_to_end(
    mut input: impl Read,
    cancel: &crate::work::Cancellation,
) -> anyhow::Result<Vec<u8>> {
    read_to_end_with_pool(&mut input, cancel, pool())
}

fn read_to_end_with_pool(
    mut input: impl Read,
    cancel: &crate::work::Cancellation,
    pool: &ResourcePool,
) -> anyhow::Result<Vec<u8>> {
    let mut reservation = pool.reserve_working(0, 0)?;
    let mut bytes = Vec::new();
    let mut buffer = vec![0; crate::limits::IO_BUFFER_BYTES];
    loop {
        cancel.check()?;
        check_memory()?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let required = bytes
            .len()
            .checked_add(count)
            .ok_or_else(|| anyhow::anyhow!("Document length overflow"))?;
        if required > bytes.capacity() {
            let capacity = required.max(bytes.capacity().saturating_mul(2));
            let capacity = if reservation
                .try_grow_to((bytes.capacity() as u64).saturating_add(capacity as u64))
                .is_ok()
            {
                capacity
            } else {
                reservation
                    .try_grow_to((bytes.capacity() as u64).saturating_add(required as u64))?;
                required
            };
            bytes.try_reserve_exact(capacity - bytes.len())?;
            reservation.shrink_to(bytes.capacity() as u64);
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(bytes)
}

pub fn pool() -> &'static ResourcePool {
    static POOL: OnceLock<ResourcePool> = OnceLock::new();
    POOL.get_or_init(|| {
        ResourcePool::with_budgets(
            crate::limits::RESOURCES_CACHE_BYTES,
            crate::limits::RESOURCES_WORKING_BYTES,
        )
        .with_admission(available_memory)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn whole_reads_stop_before_unadmitted_growth_and_retry_cleanly() {
        use super::*;
        let bytes = vec![b'x'; crate::limits::IO_BUFFER_BYTES + 1];
        let cancel = crate::work::Cancellation::default();
        let small = ResourcePool::with_budgets(0, crate::limits::IO_BUFFER_BYTES as u64);
        assert!(
            read_to_end_with_pool(bytes.as_slice(), &cancel, &small)
                .unwrap_err()
                .is::<WorkingPressure>()
        );
        assert_eq!(small.snapshot().working_bytes, 0);
        let enough = ResourcePool::new(0);
        assert_eq!(
            read_to_end_with_pool(bytes.as_slice(), &cancel, &enough).unwrap(),
            bytes
        );
        assert_eq!(enough.snapshot().working_bytes, 0);
    }

    #[test]
    fn system_headroom_reserves_five_percent_and_stops_at_ceiling() {
        let gib = 1024 * 1024 * 1024;
        assert_eq!(
            super::headroom(16 * gib, 15 * gib),
            16 * gib * 95 / 100 - 15 * gib
        );
        assert_eq!(super::headroom(16 * gib, 16 * gib), 0);
    }
}
