//! Table presentation consumes admitted cells and emits semantic page requests.
use crate::views::reader::contract::ReaderHost;
use glim_core::table::TablePage;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    base::InteractiveElementExt as _,
    component::{
        ActiveTheme, Disableable, Sizable, button::Button, h_flex, scroll::Scrollbar, v_flex,
    },
    *,
};

#[derive(Clone, Copy)]
pub(super) enum TableRequest {
    Retry,
    PreviousRows,
    NextRows,
    PreviousColumns,
    NextColumns,
}

pub(super) struct TableView {
    host: ReaderHost,
    show_detail: fn(String, String, &mut Window, &mut App),
    columns_limit: usize,
    cell_chars: usize,
    pub(super) page: TablePage,
    format: &'static str,
    row_start: usize,
    column_start: usize,
    has_previous: bool,
    busy: bool,
    error: Option<String>,
    resume_scroll: Option<(Point<Pixels>, Point<Pixels>)>,
    scroll: UniformListScrollHandle,
    horizontal_scroll: ScrollHandle,
}
impl EventEmitter<TableRequest> for TableView {}
impl TableView {
    pub fn new(
        host: ReaderHost,
        columns_limit: usize,
        cell_chars: usize,
        show_detail: fn(String, String, &mut Window, &mut App),
    ) -> Self {
        Self {
            host,
            show_detail,
            columns_limit,
            cell_chars,
            page: TablePage::default(),
            format: "CSV",
            row_start: 0,
            column_start: 0,
            has_previous: false,
            busy: false,
            error: None,
            resume_scroll: None,
            scroll: UniformListScrollHandle::new(),
            horizontal_scroll: ScrollHandle::new(),
        }
    }
    pub fn reset_position(&mut self) {
        self.resume_scroll = None;
    }
    pub fn begin(&mut self, format: &'static str, row: usize, column: usize, has_previous: bool) {
        self.format = format;
        self.row_start = row;
        self.column_start = column;
        self.has_previous = has_previous;
        self.page = TablePage::default();
        self.error = None;
        self.busy = true;
    }
    pub fn publish(&mut self, page: TablePage) {
        self.page = page;
        self.busy = false;
        self.scroll = UniformListScrollHandle::new();
        if let Some((vertical, horizontal)) = self.resume_scroll.take() {
            self.scroll.0.borrow().base_handle.set_offset(vertical);
            self.horizontal_scroll.set_offset(horizontal);
        } else {
            self.horizontal_scroll.set_offset(point(px(0.), px(0.)));
        }
    }
    pub fn fail(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
    }
    pub fn suspend(&mut self, remember_scroll: bool) {
        if remember_scroll && !self.busy && self.error.is_none() {
            self.resume_scroll = Some((
                self.scroll.0.borrow().base_handle.offset(),
                self.horizontal_scroll.offset(),
            ));
        }
        self.page = TablePage::default();
        self.busy = false;
    }
    fn request(&self, request: TableRequest, cx: &mut Context<Self>) {
        let action = match request {
            TableRequest::Retry => "retry",
            TableRequest::PreviousRows => "previous_rows",
            TableRequest::NextRows => "next_rows",
            TableRequest::PreviousColumns => "previous_columns",
            TableRequest::NextColumns => "next_columns",
        };
        (self.host.action)("csv", action, &[]);
        cx.emit(request);
    }
    fn show_cell(&self, row: usize, column: usize, window: &mut Window, cx: &mut Context<Self>) {
        (self.host.action)("csv", "cell_detail", &[]);
        let Some(value) = self
            .page
            .rows
            .get(row)
            .and_then(|row| row.get(column))
            .cloned()
        else {
            return;
        };
        (self.show_detail)((self.host.translate)("Full cell value"), value, window, cx);
    }
}
impl Render for TableView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.busy;
        if busy {
            return super::super::controls::loading((self.host.translate)("Loading preview…"), cx);
        }
        let columns = self
            .page
            .columns
            .saturating_sub(self.column_start)
            .min(self.columns_limit);
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
                    .child(h_flex().h(px(32.)).child(div().w(px(64.)).child((self.host.translate)("Row")))
                        .children((0..columns).map(|column| div().w(px(200.)).px_3().child(if geographic {
                            (self.host.translate)(match column { 0 => "Longitude", 1 => "Latitude", _ => "Altitude" })
                        } else { (self.column_start + column + 1).to_string() }))))
                    .child(uniform_list("csv-rows", self.page.rows.len() + 3, cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                        range.map(|row| {
                            if row >= view.page.rows.len() { return div().h(px(32.)).into_any_element(); }
                            h_flex().h(px(32.)).w_full().overflow_hidden().border_b_1().border_color(cx.theme().border)
                                .child(div().w(px(64.)).flex_shrink_0().px_2().text_color(cx.theme().muted_foreground)
                                    .child((view.row_start + row + 1).to_string()))
                                .children((0..columns).map(|column| {
                                    let value = view.page.rows[row].get(column).map(String::as_str).unwrap_or("");
                                    div().id(("csv-cell", row * columns + column)).w(px(200.)).h_full().flex_shrink_0().px_3()
                                        .flex().items_center().overflow_hidden()
                                        .child(div().w_full().min_w_0().truncate().child(
                                            // Hard line breaks are not suppressed by nowrap.
                                            // Flatten only the visible excerpt; show_cell keeps the original value.
                                            value.chars().take(view.cell_chars)
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
            )).child(super::super::controls::floating("csv-controls", super::super::controls::ActionBindings::new().action("status",div().min_w_0().truncate().child(if busy { (self.host.translate)("Loading table…") } else {
                    self.error.clone().unwrap_or_else(|| format!("{} · rows {}–{} · columns {}–{}", self.format,
                        self.row_start + 1, self.row_start + self.page.rows.len(), self.column_start + 1, self.column_start + columns))
                }))
                .when(self.error.is_some(), |bar| bar.action("retry",
                    Button::new("csv-retry").small().label((self.host.translate)("Retry"))
                        .on_click(cx.listener(|view, _, _, cx| view.request(TableRequest::Retry, cx)))
                ))
                .when(self.has_previous || self.page.more, |bar| bar
                .action("previous-rows",crate::views::icon_button("csv-prev", gpui_kit::assets::IconName::ChevronLeft, (self.host.translate)("Previous"))
                    .disabled(busy || !self.has_previous).on_click(cx.listener(|view, _, _, cx| view.request(TableRequest::PreviousRows, cx))))
                .action("next-rows",crate::views::icon_button("csv-next", gpui_kit::assets::IconName::ChevronRight, (self.host.translate)("Next page"))
                    .disabled(busy || !self.page.more).on_click(cx.listener(|view, _, _, cx| view.request(TableRequest::NextRows, cx))))
                )
                .when(self.column_start > 0 || self.column_start + columns < self.page.columns, |bar| bar
                .action("previous-columns",crate::views::icon_button("csv-prev-cols", gpui_kit::assets::IconName::ChevronsLeft, (self.host.translate)("Previous columns"))
                    .disabled(busy || self.column_start == 0).on_click(cx.listener(|view, _, _, cx| view.request(TableRequest::PreviousColumns, cx))))
                .action("next-columns",crate::views::icon_button("csv-next-cols", gpui_kit::assets::IconName::ChevronsRight, (self.host.translate)("Next columns"))
                    .disabled(busy || self.column_start + columns >= self.page.columns).on_click(cx.listener(|view, _, _, cx| view.request(TableRequest::NextColumns, cx))))), cx)).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{ReaderHost, TablePage, TableView};
    use glim_core::resources::{ResourcePool, TextRowsBuilder};
    use gpui_kit::{point, px};

    fn contract_page(pool: &ResourcePool) -> TablePage {
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../assets/reader-contracts/table.json"
        ))
        .unwrap();
        let mut rows = TextRowsBuilder::new(pool);
        for row in expected["rows"].as_array().unwrap() {
            rows.try_push(
                row.as_array()
                    .unwrap()
                    .iter()
                    .map(|cell| cell.as_str().unwrap()),
            )
            .unwrap();
        }
        TablePage {
            rows: rows.finish(),
            columns: expected["columns"].as_u64().unwrap() as usize,
            more: expected["more"].as_bool().unwrap(),
        }
    }

    #[test]
    fn contract_handoff_and_repeated_suspension_preserve_values_and_scroll() {
        let pool = ResourcePool::with_budgets(0, 16 * 1024);
        let host = ReaderHost {
            translate: str::to_owned,
            action: |_, _, _| {},
            resources: pool.clone(),
        };
        let mut view = TableView::new(host, 3, 10, |_, _, _, _| {});
        let page = contract_page(&pool);
        let original = page.rows[1][2].as_ptr();
        let bytes = pool.snapshot().working_bytes;
        view.begin("CSV", 120, 4, true);
        view.publish(page);
        assert_eq!(view.page.rows[1][2].as_ptr(), original);
        assert_eq!(view.page.rows[1][2], "line 1\nline 2 \"yes\"");
        assert_eq!(view.page.rows[2], ["short"]);
        assert_eq!(pool.snapshot().working_bytes, bytes);
        let vertical = point(px(0.), px(-64.));
        let horizontal = point(px(-120.), px(0.));
        view.scroll.0.borrow().base_handle.set_offset(vertical);
        view.horizontal_scroll.set_offset(horizontal);
        view.suspend(true);
        view.suspend(false);
        assert_eq!(pool.snapshot().working_bytes, 0);
        assert_eq!(
            (view.row_start, view.column_start, view.has_previous),
            (120, 4, true)
        );
        view.begin("CSV", 120, 4, true);
        view.fail("retry".into());
        assert!(view.page.rows.is_empty());
        view.begin("CSV", 120, 4, true);
        view.publish(contract_page(&pool));
        assert_eq!(view.scroll.0.borrow().base_handle.offset(), vertical);
        assert_eq!(view.horizontal_scroll.offset(), horizontal);
        drop(view);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
}
