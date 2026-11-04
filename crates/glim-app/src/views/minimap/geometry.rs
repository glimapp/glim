//! Immutable row geometry shares untouched blocks across painted frames.
use glim_core::resources::{ResourcePool, WorkingPressure, WorkingReservation};
use gpui_kit::component::input::{Rope, RopeExt};
use std::sync::Arc;

pub type Row = (u8, u8);
// A storage block, not a limit on document rows or visible content.
const BLOCK_ROWS: usize = 256;

struct Block {
    rows: Vec<Row>,
    _memory: WorkingReservation,
}
struct Chunk {
    end: usize,
    block: Arc<Block>,
}
pub struct Lines {
    chunks: Vec<Chunk>,
    _memory: WorkingReservation,
}
impl Lines {
    pub fn get(&self, row: usize) -> Option<Row> {
        let chunk = self
            .chunks
            .get(self.chunks.partition_point(|chunk| chunk.end <= row))?;
        chunk
            .block
            .rows
            .get(row - (chunk.end - chunk.block.rows.len()))
            .copied()
    }
}

pub struct Geometry {
    text: Rope,
    lines: Option<Arc<Lines>>,
    pool: ResourcePool,
}
impl Geometry {
    pub fn new(text: &Rope, pool: ResourcePool) -> Self {
        let mut geometry = Self {
            text: text.clone(),
            lines: None,
            pool,
        };
        let _ = geometry.update(text);
        geometry
    }
    pub fn suspend(&mut self, text: &Rope) {
        self.text = text.clone();
        self.lines = None;
    }
    pub fn len(&self) -> usize {
        self.text.lines_len()
    }
    pub fn snapshot(&self) -> Option<Arc<Lines>> {
        self.lines.clone()
    }
    pub fn update(&mut self, text: &Rope) -> Result<usize, WorkingPressure> {
        let result = self.prepare(text);
        self.text = text.clone();
        match result {
            Ok((lines, rows)) => {
                self.lines = Some(lines);
                Ok(rows)
            }
            Err(error) => {
                // Optional geometry must never reject an edit. Keep the current
                // row count for seeking; a later update can retry reconstruction.
                self.lines = None;
                Err(error)
            }
        }
    }
    fn prepare(&self, text: &Rope) -> Result<(Arc<Lines>, usize), WorkingPressure> {
        let (old_rows, new_rows) = if let Some(lines) = &self.lines {
            let prefix = matching_bytes(
                self.text.chunks(),
                text.chunks(),
                false,
                self.text.len().min(text.len()),
            );
            if prefix == self.text.len() && prefix == text.len() {
                return Ok((lines.clone(), 0));
            }
            let prefix = self.text.floor_char_boundary(prefix);
            let suffix = matching_bytes(
                self.text.chunks_at(self.text.len()).0.reversed(),
                text.chunks_at(text.len()).0.reversed(),
                true,
                (self.text.len() - prefix).min(text.len() - prefix),
            );
            let old_end = self.text.ceil_char_boundary(self.text.len() - suffix);
            let new_end = text.ceil_char_boundary(text.len() - suffix);
            let start = self.text.offset_to_point(prefix).row;
            (
                start..self.text.offset_to_point(old_end).row + 1,
                start..text.offset_to_point(new_end).row + 1,
            )
        } else {
            (0..0, 0..text.lines_len())
        };
        let old = self.lines.as_deref();
        let (first, last, cover_start, cover_end) = if let Some(old) = old {
            let first = old
                .chunks
                .partition_point(|chunk| chunk.end <= old_rows.start)
                .saturating_sub(1);
            let last = (old.chunks.partition_point(|chunk| chunk.end < old_rows.end) + 2)
                .min(old.chunks.len());
            let start = if first == 0 {
                0
            } else {
                old.chunks[first - 1].end
            };
            (first, last, start, old.chunks[last - 1].end)
        } else {
            (0, 0, 0, 0)
        };
        let replacement_rows = cover_end - cover_start - old_rows.len() + new_rows.len();
        let replacement_chunks = replacement_rows.div_ceil(BLOCK_ROWS);
        let remaining_chunks = old.map_or(0, |old| old.chunks.len() - last);
        let total_chunks = first + replacement_chunks + remaining_chunks;
        let bytes = (total_chunks.saturating_mul(size_of::<Chunk>())
            + size_of::<Lines>()
            + 2 * size_of::<usize>()) as u64;
        let memory = self.pool.reserve_working(bytes, bytes)?;
        let mut chunks = Vec::with_capacity(total_chunks);
        if let Some(old) = old {
            chunks.extend(old.chunks[..first].iter().map(|chunk| Chunk {
                end: chunk.end,
                block: chunk.block.clone(),
            }));
        }
        let changed = text
            .slice(text.line_start_offset(new_rows.start)..text.line_start_offset(new_rows.end));
        let mut changed_rows = row_shapes(changed.chunks());
        let mut row = cover_start;
        for index in 0..replacement_chunks {
            // Balance the replacement to keep interior blocks at least half
            // full; repeated small edits cannot accumulate one-row fragments.
            let count = replacement_rows / replacement_chunks
                + usize::from(index < replacement_rows % replacement_chunks);
            let bytes =
                (count * size_of::<Row>() + size_of::<Block>() + 2 * size_of::<usize>()) as u64;
            let memory = self.pool.reserve_working(bytes, bytes)?;
            let mut rows = Vec::with_capacity(count);
            for _ in 0..count {
                let geometry = if row < new_rows.start {
                    old.expect("unchanged prefix").get(row).expect("prefix row")
                } else if row < new_rows.end {
                    changed_rows.next().expect("changed row")
                } else {
                    old.expect("unchanged suffix")
                        .get(old_rows.end + row - new_rows.end)
                        .expect("suffix row")
                };
                rows.push(geometry);
                row += 1;
            }
            chunks.push(Chunk {
                end: row,
                block: Arc::new(Block {
                    rows,
                    _memory: memory,
                }),
            });
        }
        if let Some(old) = old {
            for chunk in &old.chunks[last..] {
                row += chunk.block.rows.len();
                chunks.push(Chunk {
                    end: row,
                    block: chunk.block.clone(),
                });
            }
        }
        debug_assert_eq!(row, text.lines_len());
        Ok((
            Arc::new(Lines {
                chunks,
                _memory: memory,
            }),
            new_rows.len(),
        ))
    }
}

fn row_shapes<'a>(mut chunks: impl Iterator<Item = &'a str>) -> impl Iterator<Item = Row> {
    let mut current = "";
    let (mut indent, mut width, mut leading) = (0_u8, 0_u8, true);
    let mut finished = false;
    std::iter::from_fn(move || {
        if finished {
            return None;
        }
        loop {
            let (fragment, tail) = match current.split_once('\n') {
                Some((fragment, tail)) => (fragment, Some(tail)),
                None => (current, None),
            };
            for ch in fragment.chars().take(120 - width as usize) {
                width += 1;
                if leading && ch.is_whitespace() {
                    indent = (indent + 1).min(100);
                } else {
                    leading = false;
                }
            }
            if let Some(tail) = tail {
                current = tail;
                let result = (indent, width - indent);
                (indent, width, leading) = (0, 0, true);
                return Some(result);
            }
            match chunks.next() {
                Some(chunk) => current = chunk,
                None => {
                    finished = true;
                    return Some((indent, width - indent));
                }
            }
        }
    })
}

/// Shared Rope leaves are skipped by address. Unshared or differently split
/// chunks still compare bytes, so correctness never depends on sharing.
fn matching_bytes<'a, 'b>(
    mut left: impl Iterator<Item = &'a str>,
    mut right: impl Iterator<Item = &'b str>,
    reversed: bool,
    limit: usize,
) -> usize {
    let (mut a, mut b) = (&[][..], &[][..]);
    let mut matched = 0;
    while matched < limit {
        if a.is_empty() {
            let Some(chunk) = left.next() else {
                break;
            };
            a = chunk.as_bytes();
            continue;
        }
        if b.is_empty() {
            let Some(chunk) = right.next() else {
                break;
            };
            b = chunk.as_bytes();
            continue;
        }
        let length = a.len().min(b.len()).min(limit - matched);
        let (a_part, b_part) = if reversed {
            (&a[a.len() - length..], &b[b.len() - length..])
        } else {
            (&a[..length], &b[..length])
        };
        let equal = if a_part.as_ptr() == b_part.as_ptr() {
            length
        } else if reversed {
            a_part
                .iter()
                .rev()
                .zip(b_part.iter().rev())
                .take_while(|(a, b)| a == b)
                .count()
        } else {
            a_part
                .iter()
                .zip(b_part)
                .take_while(|(a, b)| a == b)
                .count()
        };
        matched += equal;
        if equal < length {
            break;
        }
        if reversed {
            a = &a[..a.len() - length];
            b = &b[..b.len() - length];
        } else {
            a = &a[length..];
            b = &b[length..];
        }
    }
    matched
}

#[cfg(test)]
mod tests {
    use super::{BLOCK_ROWS, Geometry, Rope, row_shapes};
    use glim_core::resources::ResourcePool;
    use std::sync::Arc;

    fn previous_geometry(text: &str) -> Vec<(u8, u8)> {
        text.split('\n')
            .map(|line| {
                let indent = line
                    .chars()
                    .take_while(|ch| ch.is_whitespace())
                    .count()
                    .min(100);
                let width = line.chars().take(120).count().saturating_sub(indent);
                (indent as u8, width as u8)
            })
            .collect()
    }
    fn check(geometry: &Geometry, text: &str) {
        let expected = previous_geometry(text);
        assert_eq!(geometry.len(), expected.len());
        let lines = geometry.snapshot().expect("admitted geometry");
        for (row, expected) in expected.into_iter().enumerate() {
            assert_eq!(lines.get(row), Some(expected), "row {row}");
        }
        assert_eq!(lines.get(geometry.len()), None);
        if geometry.len() >= BLOCK_ROWS / 2 {
            assert!(
                lines
                    .chunks
                    .iter()
                    .all(|chunk| (BLOCK_ROWS / 2..=BLOCK_ROWS).contains(&chunk.block.rows.len()))
            );
        }
    }

    #[test]
    fn chunk_boundaries_preserve_unicode_whitespace_and_empty_lines() {
        let text = format!(
            " \t\u{2003}中文😀\r\n\n{}suffix\n{}\n",
            " ".repeat(130),
            "長".repeat(150)
        );
        let expected = previous_geometry(&text);
        for split in text
            .char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(text.len()))
        {
            let chunks = ["", &text[..split], "", &text[split..], ""];
            assert_eq!(
                row_shapes(chunks.into_iter()).collect::<Vec<_>>(),
                expected,
                "split {split}"
            );
        }
    }

    #[test]
    fn incremental_rows_match_full_geometry_across_unicode_and_line_edits() {
        let pool = ResourcePool::with_budgets(0, 4 * 1024 * 1024);
        let original = format!(
            "\r\n中文😀\n\t  mixed\r\n{}\n{}\n",
            " ".repeat(10_000),
            "長".repeat(2000)
        );
        let mut text = original;
        let mut rope = Rope::from_str(&text);
        let mut geometry = Geometry::new(&rope, pool.clone());
        let mut random = 0x7349_u64;
        for step in 0..350 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let chars = text.chars().count();
            let position = (random as usize) % (chars + 1);
            let offset = text
                .char_indices()
                .nth(position)
                .map_or(text.len(), |(byte, _)| byte);
            if step % 3 == 0 && offset < text.len() {
                let end = offset + text[offset..].chars().next().unwrap().len_utf8();
                text.replace_range(offset..end, "");
                rope.remove(offset..end);
            } else {
                let inserted = ["\n", "é", "\t ", "中文😀", "\r", "x"][step % 6];
                text.insert_str(offset, inserted);
                rope.insert(offset, inserted);
            }
            if step % 17 == 0 {
                rope = Rope::from_str(&text);
            }
            geometry.update(&rope).unwrap();
            check(&geometry, &text);
        }
        for replacement in [
            "",
            "\n",
            "\r\n",
            "é",
            "ê",
            "\n\n",
            "tail\n",
            "tail",
            "\t\u{2003}中文😀",
        ] {
            rope = Rope::from_str(replacement);
            geometry.update(&rope).unwrap();
            check(&geometry, replacement);
        }
        assert_eq!(geometry.update(&rope).unwrap(), 0);
        drop(geometry);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }

    #[test]
    fn a_local_edit_reuses_distant_blocks_and_frames_retain_their_own_admission() {
        let pool = ResourcePool::with_budgets(0, 4 * 1024 * 1024);
        let source = (0..20_000)
            .map(|i| format!("  row {i}: 中文内容\n"))
            .collect::<String>();
        let mut rope = Rope::from_str(&source);
        let mut geometry = Geometry::new(&rope, pool.clone());
        let frame = geometry.snapshot().unwrap();
        let before = pool.snapshot().working_bytes;
        let offset = source.find("row 10000").unwrap();
        rope.insert(offset, "new ");
        assert_eq!(geometry.update(&rope).unwrap(), 1);
        let next_frame = geometry.snapshot().unwrap();
        assert!(Arc::ptr_eq(
            &frame.chunks[0].block,
            &next_frame.chunks[0].block
        ));
        assert!(Arc::ptr_eq(
            &frame.chunks.last().unwrap().block,
            &next_frame.chunks.last().unwrap().block
        ));
        assert!(pool.snapshot().working_bytes > before);
        check(&geometry, &rope.to_string());
        drop(geometry);
        assert!(pool.snapshot().working_bytes > 0);
        drop(frame);
        assert!(pool.snapshot().working_bytes > 0);
        drop(next_frame);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }

    #[test]
    fn pressure_keeps_current_navigation_and_retry_reconstructs_geometry() {
        let budget = 64 * 1024;
        let pool = ResourcePool::with_budgets(0, budget);
        let mut rope = Rope::from_str(&"row\n".repeat(1000));
        let mut geometry = Geometry::new(&rope, pool.clone());
        let frame = geometry.snapshot().unwrap();
        let retained = pool.snapshot().working_bytes;
        let blocker = pool
            .reserve_working(budget - retained, budget - retained)
            .unwrap();
        rope.insert(0, "new\n");
        assert!(geometry.update(&rope).is_err());
        assert!(geometry.snapshot().is_none());
        assert_eq!(geometry.len(), 1002);
        assert_eq!(pool.snapshot().working_bytes, budget);
        drop(blocker);
        assert_eq!(pool.snapshot().working_bytes, retained);
        drop(frame);
        assert_eq!(pool.snapshot().working_bytes, 0);
        assert_eq!(geometry.update(&rope).unwrap(), 1002);
        check(&geometry, &rope.to_string());
        geometry.suspend(&rope);
        assert_eq!(pool.snapshot().working_bytes, 0);
        assert!(geometry.snapshot().is_none());
        assert_eq!(geometry.update(&rope).unwrap(), 1002);
        check(&geometry, &rope.to_string());
        drop(geometry);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
}
