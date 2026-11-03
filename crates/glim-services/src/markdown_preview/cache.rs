//! Prepared PNG reuse. The caller owns synchronization and rendering on a miss.
use super::{Diagram, DiagramImage};
use crate::resources::{
    CacheActivity, CacheActivitySnapshot, CacheLookup, CachedResource, ResourcePool, ResourceValue,
    Retention,
};
use std::{collections::VecDeque, sync::Arc};

type Entry = (Diagram, u32, CachedResource<DiagramImage>);

pub struct ImageCache {
    entries: VecDeque<Entry>,
    pool: ResourcePool,
    byte_budget: u64,
    activity: CacheActivity,
}

impl ImageCache {
    pub fn new(pool: ResourcePool, byte_budget: u64) -> Self {
        Self {
            entries: VecDeque::new(),
            pool,
            byte_budget,
            activity: CacheActivity::new(),
        }
    }

    pub fn lookup(
        &mut self,
        diagram: &Diagram,
        edge: u32,
    ) -> Option<Arc<ResourceValue<DiagramImage>>> {
        self.entries.retain(|(_, _, image)| image.is_resident());
        let image = self
            .entries
            .iter()
            .position(|(key, size, _)| key == diagram && *size == edge)
            .and_then(|index| self.entries.remove(index))
            .and_then(|entry| {
                let image = entry.2.get(Retention::Background);
                self.entries.push_back(entry);
                image
            });
        self.activity.record(if image.is_some() {
            CacheLookup::Hit
        } else {
            CacheLookup::Miss
        });
        image
    }

    pub fn remember(&mut self, diagram: &Diagram, edge: u32, image: &DiagramImage) {
        let bytes = diagram
            .source
            .len()
            .saturating_add(diagram.kind.len())
            .saturating_add(image.png.len())
            .saturating_add(std::mem::size_of::<Entry>()) as u64;
        if bytes > self.byte_budget {
            return;
        }
        self.entries
            .retain(|(key, size, image)| image.is_resident() && (key != diagram || *size != edge));
        let mut used: u64 = self.entries.iter().map(|(_, _, image)| image.bytes()).sum();
        while used.saturating_add(bytes) > self.byte_budget {
            let Some((_, _, image)) = self.entries.pop_front() else {
                break;
            };
            used = used.saturating_sub(image.bytes());
        }
        let image = self.pool.cache(image.clone(), bytes, Retention::Background);
        if image.is_resident() {
            self.entries.push_back((diagram.clone(), edge, image));
        }
    }

    pub fn activity(&self) -> CacheActivitySnapshot {
        self.activity.snapshot()
    }
}
