//! Application adapter for CSV/geographic preparation and page navigation.
mod view;
use self::view::{TableRequest, TableView};
use crate::app::reader_jobs::{self, JobClass, Priority};
use glim_core::resources::TextSnapshot;
use glim_services::limits as reader_limits;
use gpui_kit::*;
use std::{path::PathBuf, sync::Arc};

#[derive(Clone)]
enum Source {
    Text(Arc<TextSnapshot>),
    File(PathBuf),
    GeoText(Arc<TextSnapshot>),
    GeoFile(PathBuf),
}
pub(super) struct CsvTable {
    suspended: bool,
    source: Option<Source>,
    delimiter: u8,
    format: &'static str,
    row_start: usize,
    column_start: usize,
    previous: Vec<usize>,
    task: Option<Task<()>>,
    view: Entity<TableView>,
    _subscription: Subscription,
}
impl CsvTable {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let view = cx.new(|_| {
            TableView::new(
                super::host(),
                reader_limits::CSV_COLUMNS,
                reader_limits::CSV_CELL_CHARS,
                super::source::show_text_dialog,
            )
        });
        let subscription = cx.subscribe(&view, |table, _, request, cx| table.request(*request, cx));
        Self {
            suspended: false,
            source: None,
            delimiter: b',',
            format: "CSV",
            row_start: 0,
            column_start: 0,
            previous: Vec::new(),
            task: None,
            view,
            _subscription: subscription,
        }
    }
    pub fn from_file(
        path: PathBuf,
        format: glim_services::documents::presentation::TableFormat,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut table = Self::new(cx);
        table.delimiter = format.delimiter();
        if let Some(name) = format.geo_name() {
            table.format = name;
            table.source = Some(Source::GeoFile(path));
        } else {
            table.source = Some(Source::File(path));
            table.format = if table.delimiter == b'\t' {
                "TSV"
            } else {
                "CSV"
            };
        }
        table
    }
    pub fn input_snapshot(&self) -> Option<&Arc<TextSnapshot>> {
        match &self.source {
            Some(Source::Text(text) | Source::GeoText(text)) => Some(text),
            _ => None,
        }
    }
    pub fn load(&mut self, text: Arc<TextSnapshot>, delimiter: u8, cx: &mut Context<Self>) {
        if matches!(&self.source, Some(Source::Text(old)) if Arc::ptr_eq(old, &text) || old.as_str() == text.as_str())
        {
            self.resume(cx);
            return;
        }
        self.source = Some(Source::Text(text));
        self.delimiter = delimiter;
        self.format = if delimiter == b'\t' { "TSV" } else { "CSV" };
        self.reload(cx);
    }
    pub fn load_geo(
        &mut self,
        text: Arc<TextSnapshot>,
        format: &'static str,
        cx: &mut Context<Self>,
    ) {
        if matches!(&self.source, Some(Source::GeoText(old)) if Arc::ptr_eq(old, &text) || old.as_str() == text.as_str())
        {
            self.resume(cx);
            return;
        }
        self.source = Some(Source::GeoText(text));
        self.format = format;
        self.reload(cx);
    }
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.view.update(cx, |view, _| view.reset_position());
        self.row_start = 0;
        self.column_start = 0;
        self.previous.clear();
        self.load_page(cx);
    }
    pub fn suspend(&mut self, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            view.suspend(!self.suspended);
            cx.notify();
        });
        self.suspended = true;
        self.task = None;
    }
    fn request(&mut self, request: TableRequest, cx: &mut Context<Self>) {
        if self.suspended || self.task.is_some() {
            return;
        }
        let page = &self.view.read(cx).page;
        match request {
            TableRequest::Retry => {}
            TableRequest::PreviousRows => {
                let Some(start) = self.previous.pop() else {
                    return;
                };
                self.row_start = start;
            }
            TableRequest::NextRows => {
                if !page.more || page.rows.is_empty() {
                    return;
                }
                self.previous.push(self.row_start);
                self.row_start += page.rows.len();
            }
            TableRequest::PreviousColumns => {
                if self.column_start == 0 {
                    return;
                }
                self.column_start = self.column_start.saturating_sub(reader_limits::CSV_COLUMNS);
            }
            TableRequest::NextColumns => {
                if self.column_start + reader_limits::CSV_COLUMNS >= page.columns {
                    return;
                }
                self.column_start += reader_limits::CSV_COLUMNS;
            }
        }
        self.load_page(cx);
    }
    pub fn resume(&mut self, cx: &mut Context<Self>) {
        if self.suspended {
            self.load_page(cx);
        }
    }
    fn load_page(&mut self, cx: &mut Context<Self>) {
        self.suspended = false;
        glim_services::telemetry::track(
            "csv_page_requested",
            [
                (
                    "row_offset",
                    glim_services::telemetry::number(self.row_start as u64),
                ),
                (
                    "column_offset",
                    glim_services::telemetry::number(self.column_start as u64),
                ),
                (
                    "format",
                    glim_services::telemetry::text(match self.format {
                        "CSV" => "csv",
                        "TSV" => "tsv",
                        "GeoJSON" => "geojson",
                        _ => "table",
                    }),
                ),
            ],
        );
        let Some(source) = self.source.clone() else {
            return;
        };
        self.task = None;
        self.view.update(cx, |view, cx| {
            view.begin(
                self.format,
                self.row_start,
                self.column_start,
                !self.previous.is_empty(),
            );
            cx.notify();
        });
        let (delimiter, row, column) = (self.delimiter, self.row_start, self.column_start);
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let telemetry_started = std::time::Instant::now();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |cancel| match source {
                Source::GeoText(text) => {
                    glim_services::geo::read_page(text.as_str().as_bytes(), row, cancel)
                }
                Source::GeoFile(path) => {
                    glim_services::geo::read_page(std::fs::File::open(path)?, row, cancel)
                }
                Source::Text(text) => glim_services::csv::read_page(
                    text.as_str().as_bytes(),
                    delimiter,
                    row,
                    column,
                    cancel,
                ),
                Source::File(path) => glim_services::csv::read_page(
                    std::fs::File::open(path)?,
                    delimiter,
                    row,
                    column,
                    cancel,
                ),
            },
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result("csv_load", None, telemetry_started, &result);
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                view.view.update(cx, |table, cx| {
                    match result {
                        Ok(page) => table.publish(page),
                        Err(error) => {
                            table.fail(format!("Cannot show {} table: {error:#}", view.format))
                        }
                    }
                    cx.notify();
                });
                cx.notify();
            });
        }));
        cx.notify();
    }
}
impl Render for CsvTable {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.view.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{CsvTable, TextSnapshot};
    use glim_core::resources::ResourcePool;
    use gpui_kit::{AppContext, TestAppContext};
    use std::sync::Arc;

    #[gpui_kit::test]
    fn replacement_and_cancelled_resume_cannot_publish_an_obsolete_page(cx: &mut TestAppContext) {
        let pool = ResourcePool::with_budgets(0, 16 * 1024);
        let snapshot =
            |text: &str| Arc::new(TextSnapshot::from_chunks(std::iter::once(text), &pool).unwrap());
        let first = snapshot("obsolete,value\nold,old\n");
        let current = snapshot(include_str!(
            "../../../../../assets/reader-contracts/table.csv"
        ));
        let table = cx.update(|cx| cx.new(CsvTable::new));
        cx.update(|cx| {
            table.update(cx, |table, cx| {
                table.load(first, b',', cx);
                table.load(current, b',', cx);
                table.suspend(cx);
                table.suspend(cx);
                table.resume(cx);
                table.suspend(cx);
                table.resume(cx);
            })
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let adapter = table.read(cx);
            assert!(adapter.task.is_none());
            assert!(!adapter.suspended);
            let page = &adapter.view.read(cx).page;
            assert_eq!(page.rows[1], ["中文", "a,b", "line 1\nline 2 \"yes\""]);
            assert_eq!(page.rows.len(), 4);
            let input = adapter.source.clone();
            drop(table);
            assert!(pool.snapshot().working_bytes > 0);
            drop(input);
        });
        cx.run_until_parked();
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
}
