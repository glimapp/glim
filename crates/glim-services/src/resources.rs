//! Process-wide policy for buffers owned by Glim. Framework allocations are separate.
pub use glim_core::resources::*;
use std::sync::OnceLock;

pub fn pool() -> &'static ResourcePool {
    static POOL: OnceLock<ResourcePool> = OnceLock::new();
    POOL.get_or_init(|| ResourcePool::new(crate::limits::RESOURCES_CACHE_BYTES))
}
