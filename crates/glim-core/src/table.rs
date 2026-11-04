//! Prepared table windows preserve cell ownership across service and UI boundaries.
use crate::resources::TextRows;

#[derive(Default)]
pub struct TablePage {
    pub rows: TextRows,
    pub columns: usize,
    pub more: bool,
}

/// A fixed SQLite row page and its independently loaded column window.
pub struct SqlitePage {
    pub rows: TextRows,
    pub more: bool,
    pub start_column: usize,
}
