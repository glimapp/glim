//! Retained SQLite presentation consumes prepared cells and emits query requests.
use crate::views::reader::contract::ReaderHost;
use glim_core::{resources::TextRows, table::SqlitePage};
use gpui_kit::base::Selectable;
use gpui_kit::{
    component::{
        ActiveTheme, Disableable, Sizable,
        button::Button,
        h_flex,
        input::{Input, InputEvent, InputState},
        table::{
            Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableSelection, TableState,
        },
        v_flex,
    },
    prelude::FluentBuilder,
    *,
};
use std::{ops::Range, sync::Arc};

#[derive(Clone, Copy)]
pub(super) struct Policy {
    pub page_rows: usize,
    pub column_window: usize,
    pub text_page_bytes: usize,
}
#[derive(Clone, Debug)]
pub(super) enum Navigation {
    First,
    Previous,
    Next,
    Last,
    Jump(usize),
}
#[derive(Clone, Debug)]
pub(super) enum Request {
    SearchChanged(String),
    SelectTable(String),
    Sort(Option<(usize, bool)>),
    Columns(usize),
    Navigate(Navigation),
    Retry,
    Detail {
        row: usize,
        column: usize,
        offset: usize,
    },
    CloseDetail,
    Copy(String),
}
#[derive(Default)]
pub(super) struct Status {
    pub table: String,
    pub page: usize,
    pub total: Option<u64>,
    pub more: bool,
    pub error: Option<String>,
    pub count_error: bool,
    pub busy: bool,
}
pub(super) struct Detail {
    pub row: usize,
    pub column: usize,
    pub offset: usize,
    pub more: bool,
    pub content: AnyView,
}

struct Grid {
    owner: WeakEntity<SqliteView>,
    columns: Vec<String>,
    rows: TextRows,
    start: usize,
    offset: usize,
    busy: bool,
    column_window: usize,
}
impl TableDelegate for Grid {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len() + 1
    }
    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }
    fn column(&self, index: usize, _: &App) -> Column {
        if index == 0 {
            Column::new("row", "#")
                .width(px(70.))
                .fixed_left()
                .movable(false)
        } else {
            Column::new(index.to_string(), self.columns[index - 1].clone())
                .width(px(220.))
                .sortable()
                .movable(false)
        }
    }
    fn cell_text(&self, row: usize, col: usize, _: &App) -> String {
        if col == 0 {
            return (self.offset + row + 1).to_string();
        }
        col.checked_sub(self.start + 1)
            .and_then(|index| self.rows.get(row)?.get(index))
            .cloned()
            .unwrap_or_default()
    }
    fn loading(&self, _: &App) -> bool {
        self.busy && self.rows.is_empty()
    }
    fn render_td(
        &mut self,
        row: usize,
        col: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let value = if col == 0 {
            (self.offset + row + 1).to_string()
        } else {
            col.checked_sub(self.start + 1)
                .and_then(|index| self.rows.get(row)?.get(index))
                .cloned()
                .unwrap_or_else(|| "…".into())
        };
        div()
            .truncate()
            .child(value.replace(['\r', '\n', '\t'], " "))
    }
    fn perform_sort(
        &mut self,
        col: usize,
        sort: ColumnSort,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        if col == 0 {
            return;
        }
        let request = Request::Sort(match sort {
            ColumnSort::Default => None,
            ColumnSort::Ascending => Some((col - 1, false)),
            ColumnSort::Descending => Some((col - 1, true)),
        });
        let owner = self.owner.clone();
        cx.defer(move |cx| {
            let _ = owner.update(cx, |_, cx| cx.emit(request));
        });
    }
    fn visible_columns_changed(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        // The table excludes its fixed row-number column from this range.
        let start = range.start;
        let end = range.end.min(self.columns.len());
        if start >= self.start && end <= self.start + self.column_window {
            return;
        }
        let owner = self.owner.clone();
        cx.defer(move |cx| {
            let _ = owner.update(cx, |_, cx| cx.emit(Request::Columns(start)));
        });
    }
}

pub(super) struct SqliteView {
    host: ReaderHost,
    policy: Policy,
    pub(super) status: Status,
    tables: Arc<[String]>,
    grid: Entity<TableState<Grid>>,
    search: Entity<InputState>,
    table_search: Entity<InputState>,
    page_input: Entity<InputState>,
    error_details: bool,
    detail: Option<Detail>,
    resume_scroll: Option<(Point<Pixels>, Point<Pixels>)>,
    _subscriptions: Vec<Subscription>,
    grid_subscription: Option<Subscription>,
}
impl EventEmitter<Request> for SqliteView {}
impl SqliteView {
    pub fn new(
        host: ReaderHost,
        policy: Policy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner = cx.entity().downgrade();
        let grid = cx.new(|cx| {
            TableState::new(
                Grid {
                    owner,
                    columns: vec![],
                    rows: Default::default(),
                    start: 0,
                    offset: 0,
                    busy: true,
                    column_window: policy.column_window,
                },
                window,
                cx,
            )
            .cell_selectable(true)
            .row_header(false)
        });
        let tr = host.translate;
        let search = cx
            .new(|cx| InputState::new(window, cx).placeholder(tr("Search all rows and columns…")));
        let table_search =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr("Filter tables…")));
        let page_input = cx.new(|cx| InputState::new(window, cx).placeholder(tr("Go to page")));
        let subscriptions = vec![
            cx.subscribe(&table_search, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&search, |_, input, event, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.emit(Request::SearchChanged(input.read(cx).value().to_string()));
                }
            }),
            cx.subscribe(&page_input, |view, _, event, cx| {
                if matches!(event, InputEvent::PressEnter { .. })
                    && let Ok(page) = view.page_input.read(cx).value().parse::<usize>()
                {
                    cx.emit(Request::Navigate(Navigation::Jump(page.saturating_sub(1))));
                }
            }),
        ];
        Self {
            host,
            policy,
            status: Status::default(),
            tables: Arc::from([]),
            grid,
            search,
            table_search,
            page_input,
            error_details: false,
            detail: None,
            resume_scroll: None,
            _subscriptions: subscriptions,
            grid_subscription: None,
        }
    }
    pub fn bind_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.grid_subscription = Some(cx.subscribe_in(&self.grid, window, |_, _, event, _, cx| {
            if let TableEvent::DoubleClickedCell(row, col) = event
                && *col > 0
            {
                cx.emit(Request::Detail {
                    row: *row,
                    column: col - 1,
                    offset: 0,
                });
            }
        }));
    }
    pub fn set_tables(&mut self, tables: Vec<String>) {
        self.tables = tables.into();
    }
    pub fn set_status(&mut self, status: Status, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }
    pub fn reset_position(&mut self) {
        self.resume_scroll = None;
    }
    pub fn clear_detail(&mut self) {
        self.detail = None;
    }
    pub fn publish_detail(&mut self, detail: Detail) {
        self.detail = Some(detail);
    }
    pub fn has_rows(&self, cx: &App) -> bool {
        !self.grid.read(cx).delegate().rows.is_empty()
    }
    #[cfg(test)]
    pub(super) fn visible_cells(&self, cx: &App) -> Vec<Vec<String>> {
        self.grid.read(cx).delegate().rows.to_vec()
    }
    #[cfg(test)]
    pub(super) fn search_input(&self) -> Entity<InputState> {
        self.search.clone()
    }
    pub fn set_loading(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.grid.update(cx, |state, cx| {
            state.delegate_mut().busy = busy;
            cx.notify();
        });
    }
    pub fn set_columns(&mut self, columns: Vec<String>, cx: &mut Context<Self>) {
        self.grid.update(cx, |state, cx| {
            let grid = state.delegate_mut();
            grid.columns = columns;
            grid.rows = Default::default();
            grid.start = 0;
            state.refresh(cx);
        });
    }
    pub fn begin_page(&mut self, cx: &mut Context<Self>) {
        self.grid.update(cx, |state, cx| {
            let grid = state.delegate_mut();
            grid.rows = Default::default();
            grid.busy = true;
            cx.notify();
        });
    }
    pub fn publish(
        &mut self,
        page: SqlitePage,
        offset: usize,
        columns: Option<Vec<String>>,
        cx: &mut Context<Self>,
    ) {
        self.grid.update(cx, |state, cx| {
            let grid = state.delegate_mut();
            let changed = columns
                .as_ref()
                .is_some_and(|columns| *columns != grid.columns);
            if let Some(columns) = columns {
                grid.columns = columns;
            }
            grid.rows = page.rows;
            grid.start = page.start_column;
            grid.offset = offset;
            grid.busy = false;
            if changed {
                state.refresh(cx);
            } else {
                cx.notify();
            }
        });
        if let Some((vertical, horizontal)) = self.resume_scroll.take() {
            self.grid.update(cx, |state, _| {
                state
                    .vertical_scroll_handle
                    .0
                    .borrow()
                    .base_handle
                    .set_offset(vertical);
                state.horizontal_scroll_handle.set_offset(horizontal);
            });
        }
    }
    pub fn suspend(&mut self, remember_scroll: bool, cx: &mut Context<Self>) {
        if remember_scroll && self.resume_scroll.is_none() {
            let grid = self.grid.read(cx);
            self.resume_scroll = Some((
                grid.vertical_scroll_handle.0.borrow().base_handle.offset(),
                grid.horizontal_scroll_handle.offset(),
            ));
        }
        self.grid.update(cx, |state, cx| {
            let grid = state.delegate_mut();
            grid.rows = Default::default();
            grid.busy = false;
            cx.notify();
        });
    }
    fn last_page(&self) -> Option<usize> {
        self.status
            .total
            .map(|n| n.saturating_sub(1) as usize / self.policy.page_rows)
    }
}
impl Render for SqliteView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tr = self.host.translate;
        let table_filter = self.table_search.read(cx).value().to_lowercase();
        let tables = self.tables.clone();
        let busy = self.status.busy;
        let last = self.last_page();
        let rows = self.grid.read(cx).delegate().rows.len();
        let mut main = v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .child(
                h_flex().gap_2().p_2().child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&self.search).small()),
                ),
            )
            .when(self.status.error.is_some() || self.status.count_error, |el| {
                let details = self.status.error.clone().unwrap_or_else(||
                    tr("Row count timed out. Retry with more time; available pages can still be browsed."));
                el.child(
                    v_flex().gap_2().px_2().pb_2()
                        .child(h_flex().gap_2()
                            .child(Button::new("sqlite-error-details").small()
                                .label(tr(if self.status.error.is_some() { "Preview unavailable" } else { "Row count unavailable" }))
                                .tooltip(tr("Show error details"))
                                .selected(self.error_details)
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.error_details = !view.error_details;
                                    cx.notify();
                                })))
                            .child(Button::new("sqlite-retry").small()
                                .label(tr(if busy { "Retrying…" } else { "Retry" }))
                                .disabled(busy)
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.error_details = false;
                                    cx.emit(Request::Retry);
                                }))))
                        .when(self.error_details, |el| el.child(
                            div().text_sm().text_color(cx.theme().muted_foreground).child(details))),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(DataTable::new(&self.grid).stripe(true).small()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .p_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("first")
                            .label("|‹")
                            .disabled(busy || self.status.page == 0)
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(Request::Navigate(Navigation::First));
                            })),
                    )
                    .child(
                        Button::new("previous")
                            .label("‹")
                            .disabled(busy || self.status.page == 0)
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(Request::Navigate(Navigation::Previous));
                            })),
                    )
                    .child(div().w(px(80.)).child(Input::new(&self.page_input).small()))
                    .child(div().child(format!(
                            "{} / {}",
                            self.status.page + 1,
                            last.map(|n| (n + 1).to_string())
                                .unwrap_or_else(|| "…".into())
                        )))
                    .child(
                        Button::new("next")
                            .label("›")
                            .disabled(busy || !self.status.more)
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(Request::Navigate(Navigation::Next));
                            })),
                    )
                    .child(
                        Button::new("last")
                            .label("›|")
                            .disabled(busy || last.is_none() || last == Some(self.status.page))
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(Request::Navigate(Navigation::Last));
                            })),
                    )
                    .child(div().flex_1().text_sm().child(format!(
                            "{}: {} · {}–{}",
                            tr("Rows"),
                            self.status.total
                                .map(|n| n.to_string())
                                .unwrap_or_else(|| "…".into()),
                            if rows == 0 {
                                0
                            } else {
                                self.status.page.saturating_mul(self.policy.page_rows) + 1
                            },
                            self.status.page.saturating_mul(self.policy.page_rows) + rows
                        ))),
            );
        if let Some(detail) = self.detail.as_mut() {
            let (row, column, offset, more) =
                (detail.row, detail.column, detail.offset, detail.more);
            let editor = detail.content.clone().into_any_element();
            main = main.child(
                v_flex()
                    .h(px(260.))
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Button::new("value-close").label(tr("Close")).on_click(
                                cx.listener(|_, _, _, cx| {
                                    cx.emit(Request::CloseDetail);
                                }),
                            ))
                            .child(
                                Button::new("value-prev")
                                    .label(tr("Previous page"))
                                    .disabled(offset == 0)
                                    .on_click(cx.listener(move |v, _, _, cx| {
                                        cx.emit(Request::Detail {
                                            row,
                                            column,
                                            offset: offset.saturating_sub(v.policy.text_page_bytes),
                                        });
                                    })),
                            )
                            .child(
                                Button::new("value-next")
                                    .label(tr("Next page"))
                                    .disabled(!more)
                                    .on_click(cx.listener(move |v, _, _, cx| {
                                        cx.emit(Request::Detail {
                                            row,
                                            column,
                                            offset: offset.saturating_add(v.policy.text_page_bytes),
                                        });
                                    })),
                            ),
                    )
                    .child(div().flex_1().min_h_0().child(editor)),
            );
        }
        h_flex()
            .on_action(
                cx.listener(|view, _: &gpui_kit::component::input::Copy, _, cx| {
                    let state = view.grid.read(cx);
                    let grid = state.delegate();
                    let value = match state.selection() {
                        TableSelection::Cell(row, column) => Some(grid.cell_text(row, column, cx)),
                        TableSelection::Row(row) => grid.rows.get(row).map(|cells| {
                            cells
                                .iter()
                                .map(|cell| {
                                    if cell.contains(['\t', '\n', '\r', '"']) {
                                        format!("\"{}\"", cell.replace('"', "\"\""))
                                    } else {
                                        cell.clone()
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("\t")
                        }),
                        _ => None,
                    };
                    if let Some(value) = value {
                        cx.emit(Request::Copy(value));
                    } else {
                        cx.propagate();
                    }
                }),
            )
            .size_full()
            .min_h_0()
            .child(
                v_flex()
                    .w(px(210.))
                    .h_full()
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .child(div().p_2().child(Input::new(&self.table_search).small()))
                    .child(
                        div()
                            .id("sqlite-tables")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .children(
                                tables
                                    .iter()
                                    .filter(|name| name.to_lowercase().contains(&table_filter))
                                    .cloned()
                                    .enumerate()
                                    .map(|(index, name)| {
                                        let selected = name == self.status.table;
                                        div()
                                            .id(("sqlite-table", index))
                                            .px_3()
                                            .py_1()
                                            .cursor_pointer()
                                            .when(selected, |el| el.bg(cx.theme().secondary))
                                            .child(name.clone())
                                            .on_click(cx.listener(move |v, _, _, cx| {
                                                if v.status.table != name {
                                                    cx.emit(Request::SelectTable(name.clone()));
                                                }
                                            }))
                                    }),
                            ),
                    ),
            )
            .child(main)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ColumnSort, InputEvent, Navigation, Policy, ReaderHost, Request, SqlitePage, SqliteView,
        TableDelegate, TableEvent,
    };
    use glim_core::resources::{ResourcePool, TextRowsBuilder};
    use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext, WindowOptions, point, px};
    use std::{cell::RefCell, rc::Rc};

    fn open(cx: &mut TestAppContext, pool: &ResourcePool) -> (AnyWindowHandle, Entity<SqliteView>) {
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    let mut view = SqliteView::new(
                        ReaderHost {
                            translate: str::to_owned,
                            action: |_, _, _| {},
                            resources: pool.clone(),
                        },
                        Policy {
                            page_rows: 100,
                            column_window: 2,
                            text_page_bytes: 1024,
                        },
                        window,
                        cx,
                    );
                    view.bind_window(window, cx);
                    view
                })
            })
            .unwrap()
        })
    }
    fn page(pool: &ResourcePool) -> SqlitePage {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../assets/reader-contracts/table.json"
        ))
        .unwrap();
        let mut rows = TextRowsBuilder::new(pool);
        for row in fixture["rows"].as_array().unwrap() {
            rows.try_push(row.as_array().unwrap().iter().map(|v| v.as_str().unwrap()))
                .unwrap();
        }
        SqlitePage {
            rows: rows.finish(),
            more: true,
            start_column: 4,
        }
    }

    #[gpui_kit::test]
    fn fixed_page_handoff_and_suspension_preserve_cells_positions_and_admission(
        cx: &mut TestAppContext,
    ) {
        let pool = ResourcePool::with_budgets(0, 16 * 1024);
        let (_, view) = open(cx, &pool);
        let first = page(&pool);
        let pointer = first.rows[1][2].as_ptr();
        let bytes = pool.snapshot().working_bytes;
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                view.status.table = "中文 table".into();
                view.status.page = 2;
                view.publish(
                    first,
                    200,
                    Some((0..8).map(|n| format!("column {n}")).collect()),
                    cx,
                );
                let grid = view.grid.read(cx).delegate();
                assert_eq!(grid.rows[1][2].as_ptr(), pointer);
                assert_eq!(grid.cell_text(1, 7, cx), "line 1\nline 2 \"yes\"");
                assert_eq!(grid.cell_text(1, 0, cx), "202");
                assert_eq!(grid.cell_text(0, 1, cx), "");
                assert_eq!(pool.snapshot().working_bytes, bytes);
                let vertical = point(px(0.), px(-64.));
                let horizontal = point(px(-120.), px(0.));
                view.grid.update(cx, |grid, _| {
                    grid.vertical_scroll_handle
                        .0
                        .borrow()
                        .base_handle
                        .set_offset(vertical);
                    grid.horizontal_scroll_handle.set_offset(horizontal);
                });
                view.suspend(true, cx);
                view.suspend(false, cx);
                assert_eq!(pool.snapshot().working_bytes, 0);
                assert_eq!((&*view.status.table, view.status.page), ("中文 table", 2));
                view.begin_page(cx);
                view.set_loading(false, cx);
                assert_eq!(view.resume_scroll, Some((vertical, horizontal)));
                view.publish(page(&pool), 200, None, cx);
                let grid = view.grid.read(cx);
                assert_eq!(
                    grid.vertical_scroll_handle.0.borrow().base_handle.offset(),
                    vertical
                );
                assert_eq!(grid.horizontal_scroll_handle.offset(), horizontal);
                view.suspend(false, cx);
                assert_eq!(pool.snapshot().working_bytes, 0);
            })
        });
    }

    #[gpui_kit::test]
    fn controls_emit_search_sort_column_and_detail_requests_without_a_database(
        cx: &mut TestAppContext,
    ) {
        let pool = ResourcePool::with_budgets(0, 16 * 1024);
        let (handle, view) = open(cx, &pool);
        let requests = Rc::new(RefCell::new(Vec::new()));
        let received = requests.clone();
        let _subscription = cx.update(|cx| {
            cx.subscribe(&view, move |_, event: &Request, _| {
                received.borrow_mut().push(event.clone());
            })
        });
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.search.update(cx, |input, cx| {
                    input.set_value("中文 filter", window, cx);
                    cx.emit(InputEvent::Change);
                });
                view.page_input.update(cx, |input, cx| {
                    input.set_value("3", window, cx);
                    cx.emit(InputEvent::PressEnter {
                        secondary: false,
                        shift: false,
                    });
                });
                view.publish(
                    page(&pool),
                    200,
                    Some((0..8).map(|n| format!("column {n}")).collect()),
                    cx,
                );
                view.grid.update(cx, |grid, cx| {
                    grid.delegate_mut()
                        .perform_sort(7, ColumnSort::Descending, window, cx);
                    grid.delegate_mut()
                        .visible_columns_changed(7..8, window, cx);
                    cx.emit(TableEvent::DoubleClickedCell(1, 7));
                });
            });
        })
        .unwrap();
        cx.run_until_parked();
        let requests = requests.borrow();
        assert!(
            requests
                .iter()
                .any(|r| matches!(r, Request::SearchChanged(value) if value == "中文 filter"))
        );
        assert!(
            requests
                .iter()
                .any(|r| matches!(r, Request::Navigate(Navigation::Jump(2))))
        );
        assert!(
            requests
                .iter()
                .any(|r| matches!(r, Request::Sort(Some((6, true)))))
        );
        assert!(requests.iter().any(|r| matches!(r, Request::Columns(7))));
        assert!(requests.iter().any(|r| matches!(
            r,
            Request::Detail {
                row: 1,
                column: 6,
                offset: 0
            }
        )));
    }
}
