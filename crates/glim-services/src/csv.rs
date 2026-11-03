//! Bounded CSV records for a read-only table; source bytes are never rewritten.
use crate::limits as reader_limits;
use anyhow::Result;
pub struct TablePage {
    pub rows: crate::resources::TextRows,
    pub columns: usize,
    pub more: bool,
}
struct RecordReader<'a, R> {
    inner: R,
    remaining: usize,
    cancel: crate::work::Cancellation,
    read_in_record: usize,
    peak: usize,
    pressured: bool,
    memory: &'a mut crate::resources::WorkingReservation,
}
impl<R: std::io::Read> std::io::Read for RecordReader<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.cancel.check().map_err(std::io::Error::other)?;
        if self.remaining == 0 {
            return Err(std::io::Error::other(
                "CSV record exceeds the active table budget; use Source to read or edit the complete value.",
            ));
        }
        let size = bytes.len().min(self.remaining);
        let peak = self
            .peak
            .max(self.read_in_record + size + reader_limits::IO_BUFFER_BYTES);
        if let Err(error) = self.memory.try_grow_to(parser_bytes(peak)) {
            self.pressured = true;
            return Err(std::io::Error::other(error));
        }
        self.peak = peak;
        let read = self.inner.read(&mut bytes[..size])?;
        self.read_in_record += read;
        self.remaining -= read;
        Ok(read)
    }
}
// csv 1.4 retains the current record and byte/string headers. Each record has
// byte and field-end arrays with doubling growth; one old allocation can also
// coexist with its replacement. Include already buffered input at row resets.
fn parser_bytes(raw: usize) -> u64 {
    (raw as u64 + 1) * (3 * 2 + 1) * (1 + std::mem::size_of::<usize>() as u64)
        + reader_limits::IO_BUFFER_BYTES as u64
}
pub fn read_page(
    input: impl std::io::Read,
    delimiter: u8,
    row_start: usize,
    column_start: usize,
    cancel: &crate::work::Cancellation,
) -> Result<TablePage> {
    read_page_with_pool(
        input,
        delimiter,
        row_start,
        column_start,
        cancel,
        crate::resources::pool(),
    )
}
fn read_page_with_pool(
    input: impl std::io::Read,
    delimiter: u8,
    row_start: usize,
    column_start: usize,
    cancel: &crate::work::Cancellation,
    pool: &crate::resources::ResourcePool,
) -> Result<TablePage> {
    cancel.check()?;
    let initial = parser_bytes(reader_limits::IO_BUFFER_BYTES);
    let mut memory = pool.reserve_working(initial, initial)?;
    let input = RecordReader {
        inner: input,
        read_in_record: 0,
        peak: reader_limits::IO_BUFFER_BYTES,
        pressured: false,
        memory: &mut memory,
        remaining: reader_limits::CSV_INPUT_BYTES,
        cancel: cancel.clone(),
    };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .buffer_capacity(crate::limits::IO_BUFFER_BYTES)
        .from_reader(input);
    let mut record = csv::StringRecord::new();
    let mut rows = crate::resources::TextRowsBuilder::new(pool);
    let mut columns = 0;
    let mut bytes = 0;
    let mut cells = 0;
    let mut index = 0;
    loop {
        match reader.read_record(&mut record) {
            Ok(false) => break,
            Ok(true) => {}
            Err(_) if reader.get_ref().pressured && !rows.is_empty() => {
                return Ok(TablePage {
                    rows: rows.finish(),
                    columns,
                    more: true,
                });
            }
            Err(error) => return Err(error.into()),
        }
        cancel.check()?;
        reader.get_mut().remaining = reader_limits::CSV_INPUT_BYTES;
        reader.get_mut().read_in_record = 0;
        if index < row_start {
            index += 1;
            continue;
        }
        columns = columns.max(record.len());
        let values = record
            .iter()
            .skip(column_start)
            .take(reader_limits::CSV_COLUMNS);
        let row_bytes = values.clone().map(str::len).sum::<usize>();
        let row_cells = values.clone().count();
        if !rows.is_empty()
            && (rows.len() >= reader_limits::CSV_ROWS
                || bytes + row_bytes > reader_limits::CSV_INPUT_BYTES
                || cells + row_cells > reader_limits::CSV_CELLS)
        {
            return Ok(TablePage {
                rows: rows.finish(),
                columns,
                more: true,
            });
        }
        bytes += row_bytes;
        cells += row_cells;
        if let Err(error) = rows.try_push(values) {
            if rows.is_empty() {
                return Err(error.into());
            }
            return Ok(TablePage {
                rows: rows.finish(),
                columns,
                more: true,
            });
        }
        index += 1;
    }
    Ok(TablePage {
        rows: rows.finish(),
        columns,
        more: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{resources::ResourcePool, work::Cancellation};

    #[test]
    fn shared_pressure_pages_rows_without_skipping_quoted_or_unicode_values() {
        let input = (0..31)
            .map(|i| format!("\"行{i},value\",\"line {i}\ncontinued\"\n"))
            .collect::<String>();
        let expected = (0..31)
            .map(|i| vec![format!("行{i},value"), format!("line {i}\ncontinued")])
            .collect::<Vec<_>>();
        let pool =
            ResourcePool::with_budgets(0, parser_bytes(2 * reader_limits::IO_BUFFER_BYTES) + 512);
        let mut found = Vec::new();
        loop {
            let page = read_page_with_pool(
                input.as_bytes(),
                b',',
                found.len(),
                0,
                &Cancellation::default(),
                &pool,
            )
            .unwrap();
            assert!(!page.rows.is_empty());
            assert!(pool.snapshot().working_bytes <= 512);
            found.extend(page.rows.iter().cloned());
            if !page.more {
                break;
            }
        }
        assert_eq!(found, expected);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }

    #[test]
    fn a_row_that_does_not_fit_is_not_silently_skipped() {
        let input = format!("first\n{}\nlast\n", "x".repeat(1000));
        let pool =
            ResourcePool::with_budgets(0, parser_bytes(2 * reader_limits::IO_BUFFER_BYTES) + 512);
        let page = read_page_with_pool(
            input.as_bytes(),
            b',',
            0,
            0,
            &Cancellation::default(),
            &pool,
        )
        .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert!(page.more);
        drop(page);
        let result = read_page_with_pool(
            input.as_bytes(),
            b',',
            1,
            0,
            &Cancellation::default(),
            &pool,
        );
        assert!(matches!(result, Err(error) if error.is::<crate::resources::WorkingPressure>()));
        assert_eq!(pool.snapshot().working_bytes, 0);
    }

    #[test]
    fn cancellation_returns_the_parser_and_result_reservations() {
        let pool = ResourcePool::with_budgets(0, 1024);
        let (cancel, owner) = Cancellation::task();
        drop(owner);
        assert!(read_page_with_pool(b"a,b\n".as_slice(), b',', 0, 0, &cancel, &pool).is_err());
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
    #[test]
    fn quoted_commas_newlines_bom_and_ragged_records() {
        let page = read_page(("\u{feff}name,value\r\n中文,\"a,b\"\r\nquoted,\"line 1\nline 2 \"\"yes\"\"\"\r\nshort\r\n").as_bytes(), b',', 0, 0, &Cancellation::default()).unwrap();
        let rows = page.rows;
        assert_eq!(rows[0], ["name", "value"]);
        assert_eq!(rows[1], ["中文", "a,b"]);
        assert_eq!(rows[2][1], "line 1\nline 2 \"yes\"");
        assert_eq!(rows[3], ["short"]);
    }
}
