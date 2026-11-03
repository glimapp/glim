use crate::app::reader_jobs::{self, JobClass, Priority};
use glim_core::resources::TextSnapshot;
use glim_services::limits as reader_limits;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    base::InteractiveElementExt as _,
    component::{ActiveTheme, Disableable, button::Button, h_flex, scroll::Scrollbar, v_flex},
    *,
};
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
    resume_scroll: Option<(Point<Pixels>, Point<Pixels>)>,
    source: Option<Source>,
    delimiter: u8,
    rows: glim_core::resources::TextRows,
    columns: usize,
    format: &'static str,
    row_start: usize,
    column_start: usize,
    previous: Vec<usize>,
    more: bool,
    task: Option<Task<()>>,
    error: Option<String>,
    scroll: UniformListScrollHandle,
    horizontal_scroll: ScrollHandle,
}
impl CsvTable {
    pub fn new() -> Self {
        Self {
            suspended: false,
            resume_scroll: None,
            source: None,
            delimiter: b',',
            rows: Default::default(),
            columns: 0,
            format: "CSV",
            row_start: 0,
            column_start: 0,
            previous: Vec::new(),
            more: false,
            task: None,
            error: None,
            scroll: UniformListScrollHandle::new(),
            horizontal_scroll: ScrollHandle::new(),
        }
    }
    pub fn from_file(
        path: PathBuf,
        format: glim_services::documents::presentation::TableFormat,
    ) -> Self {
        let mut table = Self::new();
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
        self.resume_scroll = None;
        self.row_start = 0;
        self.column_start = 0;
        self.previous.clear();
        self.load_page(cx);
    }
    pub fn suspend(&mut self) {
        if !self.suspended && self.task.is_none() && self.error.is_none() {
            self.resume_scroll = Some((
                self.scroll.0.borrow().base_handle.offset(),
                self.horizontal_scroll.offset(),
            ));
        }
        self.suspended = true;
        self.task = None;
        self.rows = Default::default();
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
        self.error = None;
        self.rows = Default::default();
        self.more = false;
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
                match result {
                    Ok(page) => {
                        view.rows = page.rows;
                        view.columns = page.columns;
                        view.more = page.more;
                        view.scroll = UniformListScrollHandle::new();
                        if let Some((vertical, horizontal)) = view.resume_scroll.take() {
                            view.scroll.0.borrow().base_handle.set_offset(vertical);
                            view.horizontal_scroll.set_offset(horizontal);
                        } else {
                            view.horizontal_scroll.set_offset(point(px(0.), px(0.)));
                        }
                    }
                    Err(error) => {
                        view.error = Some(format!("Cannot show {} table: {error:#}", view.format))
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn show_cell(&self, row: usize, column: usize, window: &mut Window, cx: &mut Context<Self>) {
        glim_services::telemetry::reader_feature("csv", "cell_detail", []);
        let Some(value) = self.rows.get(row).and_then(|row| row.get(column)).cloned() else {
            return;
        };
        super::source::show_text_dialog(
            glim_services::locale::tr("Full cell value"),
            value,
            window,
            cx,
        );
    }
}
impl Render for CsvTable {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.task.is_some();
        if busy {
            return super::preview_loading(cx);
        }
        let columns = self
            .columns
            .saturating_sub(self.column_start)
            .min(reader_limits::CSV_COLUMNS);
        let width = 64. + columns as f32 * 200.;
        let geographic = matches!(self.format, "GPX" | "KML");
        let horizontal = self.horizontal_scroll.clone();
        let vertical = self.scroll.0.borrow().base_handle.clone();
        let view = cx.entity().downgrade();
        v_flex().relative().size_full().min_h_0()
            .child(canvas(
                |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                move |_, hitbox, window, _| {
                    let horizontal = horizontal.clone();
                    let vertical = vertical.clone();
                    let view = view.clone();
                    window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                        if phase != DispatchPhase::Capture
                            || !hitbox.should_handle_scroll(window)
                            || !horizontal.bounds().contains(&event.position) {
                            return;
                        }
                        let delta = event.delta.pixel_delta(window.line_height());
                        if event.modifiers.shift {
                            // macOS may already map Shift-wheel's vertical delta to x.
                            let distance = if delta.x.abs() > delta.y.abs() { delta.x } else { delta.y };
                            let offset = horizontal.offset();
                            horizontal.set_offset(point(
                                (offset.x + distance).clamp(-horizontal.max_offset().x.max(px(0.)), px(0.)),
                                px(0.),
                            ));
                        } else {
                            let offset = vertical.offset();
                            vertical.set_offset(point(px(0.),
                                (offset.y + delta.y).clamp(-vertical.max_offset().y.max(px(0.)), px(0.))));
                        }
                        // Consume even at an edge: the nested scrollers must not
                        // reinterpret a remaining delta as movement on the other axis.
                        cx.stop_propagation();
                        let _ = view.update(cx, |_, cx| cx.notify());
                    });
                },
            ).absolute().size_full())

            .child(div().id("csv-horizontal").flex_1().min_h_0().overflow_x_scroll().track_scroll(&self.horizontal_scroll).child(
                v_flex().w(px(width)).h_full()
                    .child(h_flex().h(px(32.)).child(div().w(px(64.)).child(glim_services::locale::tr("Row")))
                        .children((0..columns).map(|column| div().w(px(200.)).px_3().child(if geographic {
                            glim_services::locale::tr(match column { 0 => "Longitude", 1 => "Latitude", _ => "Altitude" })
                        } else { (self.column_start + column + 1).to_string() }))))
                    .child(uniform_list("csv-rows", self.rows.len() + 3, cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                        range.map(|row| {
                            if row >= view.rows.len() { return div().h(px(32.)).into_any_element(); }
                            h_flex().h(px(32.)).w_full().overflow_hidden().border_b_1().border_color(cx.theme().border)
                                .child(div().w(px(64.)).flex_shrink_0().px_2().text_color(cx.theme().muted_foreground)
                                    .child((view.row_start + row + 1).to_string()))
                                .children((0..columns).map(|column| {
                                    let value = view.rows[row].get(column).map(String::as_str).unwrap_or("");
                                    div().id(("csv-cell", row * columns + column)).w(px(200.)).h_full().flex_shrink_0().px_3()
                                        .flex().items_center().overflow_hidden()
                                        .child(div().w_full().min_w_0().truncate().child(
                                            // Hard line breaks are not suppressed by nowrap.
                                            // Flatten only the visible excerpt; show_cell keeps the original value.
                                            value.chars().take(reader_limits::CSV_CELL_CHARS)
                                                .map(|ch| if matches!(ch, '\n' | '\r' | '\t' | '\u{0085}' | '\u{2028}' | '\u{2029}') { ' ' } else { ch })
                                                .collect::<String>()
                                        ))
                                        .on_double_click(cx.listener(move |view, _, window, cx| view.show_cell(row, column, window, cx)))
                                })).into_any_element()
                        }).collect()
                    })).track_scroll(&self.scroll).flex_1().min_h_0())
            ))
            .child(div().h(Scrollbar::width()).flex_shrink_0().child(
                Scrollbar::horizontal(&self.horizontal_scroll).viewport_from_layout(),
            )).child(super::controls::floating("csv-controls", h_flex().p_0().gap_2().text_xs()
                .child(div().min_w_0().truncate().child(if busy { glim_services::locale::tr("Loading table…") } else {
                    self.error.clone().unwrap_or_else(|| format!("{} · rows {}–{} · columns {}–{}", self.format,
                        self.row_start + 1, self.row_start + self.rows.len(), self.column_start + 1, self.column_start + columns))
                }))
                .when(self.error.is_some(), |bar| bar.child(
                    Button::new("csv-retry").label(glim_services::locale::tr("Retry"))
                        .on_click(cx.listener(|view, _, _, cx| view.load_page(cx)))
                ))
                .when(!self.previous.is_empty() || self.more, |bar| bar
                .child(Button::new("csv-prev").label(glim_services::locale::tr("Previous"))
                    .disabled(busy || self.previous.is_empty()).on_click(cx.listener(|view, _, _, cx| {
                        glim_services::telemetry::reader_feature("csv", "previous_rows", []); if let Some(start) = view.previous.pop() { view.row_start = start; view.load_page(cx); }
                    })))
                .child(Button::new("csv-next").label(glim_services::locale::tr("Next page"))
                    .disabled(busy || !self.more).on_click(cx.listener(|view, _, _, cx| {
                        glim_services::telemetry::reader_feature("csv", "next_rows", []); view.previous.push(view.row_start); view.row_start += view.rows.len(); view.load_page(cx);
                    })))
                )
                .when(self.column_start > 0 || self.column_start + columns < self.columns, |bar| bar
                .child(Button::new("csv-prev-cols").label(glim_services::locale::tr("Previous columns"))
                    .disabled(busy || self.column_start == 0).on_click(cx.listener(|view, _, _, cx| {
                        glim_services::telemetry::reader_feature("csv", "previous_columns", []); view.column_start = view.column_start.saturating_sub(reader_limits::CSV_COLUMNS); view.load_page(cx);
                    })))
                .child(Button::new("csv-next-cols").label(glim_services::locale::tr("Next columns"))
                    .disabled(busy || self.column_start + columns >= self.columns).on_click(cx.listener(|view, _, _, cx| {
                        glim_services::telemetry::reader_feature("csv", "next_columns", []); view.column_start += reader_limits::CSV_COLUMNS; view.load_page(cx);
                    })))), cx)).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{CsvTable, Source, TextSnapshot};
    use glim_core::resources::{ResourcePool, TextRowsBuilder};
    use std::sync::Arc;
    #[test]
    fn a_running_projection_retains_source_accounting_after_its_view_closes() {
        let pool = ResourcePool::new(0);
        let mut table = CsvTable::new();
        table.source = Some(Source::Text(Arc::new(
            TextSnapshot::from_chunks(std::iter::once("a,b".repeat(1024).as_str()), &pool).unwrap(),
        )));
        let running_input = table.source.clone();
        drop(table);
        assert!(pool.snapshot().working_bytes > 0);
        drop(running_input);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
    #[test]
    fn suspension_releases_cells_and_preserves_reconstruction_and_page() {
        let pool = ResourcePool::new(0);
        let mut table = CsvTable::new();
        table.source = Some(Source::Text(Arc::new(
            TextSnapshot::from_chunks(std::iter::once("a,b"), &pool).unwrap(),
        )));
        table.row_start = 120;
        table.column_start = 4;
        table.previous = vec![0, 60];
        let mut rows = TextRowsBuilder::new(&pool);
        rows.try_push(["long cell".repeat(100).as_str()].into_iter())
            .unwrap();
        table.rows = rows.finish();
        let before = pool.snapshot().working_bytes;
        table.suspend();
        assert!(table.rows.is_empty());
        assert_eq!(table.row_start, 120);
        assert_eq!(table.column_start, 4);
        assert_eq!(table.previous, [0, 60]);
        assert!(matches!(table.source, Some(Source::Text(_))));
        assert!(pool.snapshot().working_bytes > 0);
        assert!(pool.snapshot().working_bytes < before);
        drop(table);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
}
