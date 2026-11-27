use super::{ResourcePool, WorkingPressure, WorkingReservation};

/// A projection's immutable input; share the owner with any running worker.
pub struct TextSnapshot {
    text: String,
    _memory: WorkingReservation,
}
// The reservation remains owned by its snapshot and does not identify the text.
impl PartialEq for TextSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for TextSnapshot {}

impl TextSnapshot {
    pub fn from_chunks<'a>(
        chunks: impl Iterator<Item = &'a str> + Clone,
        pool: &ResourcePool,
    ) -> Result<Self, WorkingPressure> {
        let bytes = chunks.clone().map(str::len).sum::<usize>();
        let memory = pool.reserve_working(bytes as u64, bytes as u64)?;
        let mut text = String::with_capacity(bytes);
        for chunk in chunks {
            text.push_str(chunk);
        }
        Ok(Self {
            text,
            _memory: memory,
        })
    }

    pub fn matches_chunks<'a>(&self, chunks: impl Iterator<Item = &'a str>) -> bool {
        let mut offset = 0;
        for chunk in chunks {
            let end = offset + chunk.len();
            if self.text.get(offset..end) != Some(chunk) {
                return false;
            }
            offset = end;
        }
        offset == self.text.len()
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn snapshot_equality_compares_text_across_distinct_reservations() {
        let pool = ResourcePool::with_budgets(0, 64);
        let snapshot = |chunks: [&str; 2]| {
            Arc::new(TextSnapshot::from_chunks(chunks.into_iter(), &pool).unwrap())
        };
        let first = snapshot(["a", "中文"]);
        let same = snapshot(["a中", "文"]);
        let changed = snapshot(["a中", "字"]);

        assert!(!Arc::ptr_eq(&first, &same));
        assert!(first == same);
        assert!(first != changed);
    }

    #[test]
    fn input_admission_and_worker_lifetime_cover_the_same_allocation() {
        let pool = ResourcePool::with_budgets(0, 8);
        assert!(TextSnapshot::from_chunks(["too ", "large"].into_iter(), &pool).is_err());
        assert_eq!(pool.snapshot().working_bytes, 0);
        let view = Arc::new(TextSnapshot::from_chunks(["a", "中文"].into_iter(), &pool).unwrap());
        let worker = view.clone();
        drop(view);
        assert_eq!(worker.as_str(), "a中文");
        assert!(worker.matches_chunks(["a中", "文"].into_iter()));
        assert!(!worker.matches_chunks(["a", "中"].into_iter()));
        assert!(!worker.matches_chunks(["a", "中文!"].into_iter()));
        assert_eq!(pool.snapshot().working_bytes, 7);
        drop(worker);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
}
