use super::{ResourcePool, WorkingPressure, WorkingReservation};
use std::ops::Deref;

/// Immutable table cells retain their admission through worker/UI handoff.
#[derive(Default)]
pub struct TextRows {
    rows: Vec<Vec<String>>,
    memory: Option<WorkingReservation>,
}
impl Deref for TextRows {
    type Target = [Vec<String>];
    fn deref(&self) -> &Self::Target {
        &self.rows
    }
}

pub struct TextRowsBuilder(TextRows);
impl TextRowsBuilder {
    pub fn new(pool: &ResourcePool) -> Self {
        Self(TextRows {
            rows: Vec::new(),
            memory: Some(pool.reserve_working(0, 0).expect("zero-byte reservation")),
        })
    }

    /// Denial leaves all earlier rows intact. The producer decides whether its
    /// paging protocol permits publishing that prefix or requires retrying.
    pub fn try_push<'a>(
        &mut self,
        values: impl Iterator<Item = &'a str> + Clone,
    ) -> Result<(), WorkingPressure> {
        let cells = values.clone().count();
        let cell_bytes =
            values.clone().map(str::len).sum::<usize>() + cells * std::mem::size_of::<String>();
        let old_capacity = self.0.rows.capacity();
        let grow = self.0.rows.len() == old_capacity;
        let mut new_capacity = if grow {
            old_capacity.saturating_mul(2).max(4)
        } else {
            old_capacity
        };
        let memory = self.0.memory.as_mut().expect("builder admission");
        let old_bytes = memory.bytes();
        // Moving the outer vector temporarily retains both allocations. Reserve
        // that peak too, then return its old storage after the move finishes.
        let peak = old_bytes
            + cell_bytes as u64
            + if grow {
                (new_capacity * std::mem::size_of::<Vec<String>>()) as u64
            } else {
                0
            };
        if let Err(error) = memory.try_grow_to(peak) {
            if !grow {
                return Err(error);
            }
            // Under pressure, try only the next required slot before refusing
            // a row that would fit without speculative spare capacity.
            new_capacity = self.0.rows.len() + 1;
            memory.try_grow_to(
                old_bytes
                    + cell_bytes as u64
                    + (new_capacity * std::mem::size_of::<Vec<String>>()) as u64,
            )?;
        }
        let mut row = Vec::with_capacity(cells);
        row.extend(values.map(str::to_owned));
        let actual = row.capacity() * std::mem::size_of::<String>()
            + row.iter().map(String::capacity).sum::<usize>();
        if grow {
            let mut rows = Vec::with_capacity(new_capacity);
            rows.append(&mut self.0.rows);
            self.0.rows = rows;
        }
        self.0.rows.push(row);
        memory.shrink_to(
            old_bytes
                + actual as u64
                + ((self.0.rows.capacity() - old_capacity) * std::mem::size_of::<Vec<String>>())
                    as u64,
        );
        Ok(())
    }

    pub fn finish(self) -> TextRows {
        self.0
    }
}
impl Deref for TextRowsBuilder {
    type Target = [Vec<String>];
    fn deref(&self) -> &Self::Target {
        &self.0.rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_growth_preserves_rows_and_handoff_keeps_the_reservation() {
        let pool = ResourcePool::with_budgets(0, 256);
        let mut rows = TextRowsBuilder::new(&pool);
        rows.try_push(["name", "中文"].into_iter()).unwrap();
        let charged = pool.snapshot().working_bytes;
        assert!(
            rows.try_push(["x".repeat(256).as_str()].into_iter())
                .is_err()
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(pool.snapshot().working_bytes, charged);
        let ready = rows.finish();
        assert_eq!(ready[0], ["name", "中文"]);
        assert_eq!(pool.snapshot().working_bytes, charged);
        drop(ready);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }

    #[test]
    fn a_single_row_can_use_the_last_available_capacity() {
        let bytes = (std::mem::size_of::<Vec<String>>() + std::mem::size_of::<String>() + 1) as u64;
        let pool = ResourcePool::with_budgets(0, bytes);
        let mut rows = TextRowsBuilder::new(&pool);
        rows.try_push(["a"].into_iter()).unwrap();
        assert_eq!(rows[0], ["a"]);
        assert_eq!(pool.snapshot().working_bytes, bytes);
    }

    #[test]
    fn reallocation_reserves_both_outer_vectors() {
        let row_bytes = std::mem::size_of::<Vec<String>>() as u64;
        let pool = ResourcePool::with_budgets(0, 8 * row_bytes);
        let mut rows = TextRowsBuilder::new(&pool);
        for _ in 0..4 {
            rows.try_push(std::iter::empty()).unwrap();
        }
        assert!(rows.try_push(std::iter::empty()).is_err());
        assert_eq!(rows.len(), 4);
        assert_eq!(pool.snapshot().working_bytes, 4 * row_bytes);
    }
}
