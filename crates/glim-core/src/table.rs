//! Prepared table windows preserve cell ownership across service and UI boundaries.
use crate::resources::TextRows;

#[derive(Default)]
pub struct TablePage {
    pub rows: TextRows,
    pub columns: usize,
    pub more: bool,
}
