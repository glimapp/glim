//! Application adapter for SQLite snapshots, queries and retained presentation.
mod view;
use self::view::{Detail, Navigation, Policy, Request, SqliteView, Status};
use crate::app::reader_jobs::{self, JobClass, Priority};
use glim_core::table::SqlitePage;
use glim_services::{
    limits,
    sqlite::browser::{self, Database, Query},
    work::{Cancellation, scheduler::Scheduler},
};
use gpui_kit::*;
use std::{path::PathBuf, sync::Arc};

struct ValueEditor(super::source::SourceReader);
impl Render for ValueEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.render(cx)
    }
}

pub(super) struct SqliteReader {
    path: PathBuf,
    active: bool,
    refresh_pending: bool,
    database: Option<Arc<Database>>,
    query: Query,
    search: String,
    scheduler: Arc<Scheduler>,
    view: Entity<SqliteView>,
    total: Option<u64>,
    more: bool,
    error: Option<String>,
    count_error: bool,
    task: Option<Task<()>>,
    count_task: Option<Task<()>>,
    refresh_task: Option<Task<()>>,
    debounce: Option<Task<()>>,
    detail_task: Option<Task<()>>,
    detail: Option<Entity<ValueEditor>>,
    request_subscription: Option<Subscription>,
}
impl SqliteReader {
    pub fn new(path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_scheduler(path, Scheduler::global().clone(), window, cx)
    }
    fn with_scheduler(
        path: PathBuf,
        scheduler: Arc<Scheduler>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = cx.new(|cx| {
            SqliteView::new(
                super::host(),
                Policy {
                    page_rows: limits::SQLITE_PAGE_ROWS,
                    column_window: limits::SQLITE_COLUMN_WINDOW,
                    text_page_bytes: limits::TEXT_PAGE_BYTES as usize,
                },
                window,
                cx,
            )
        });
        let mut reader = Self {
            path,
            active: true,
            refresh_pending: false,
            database: None,
            query: Query::default(),
            search: String::new(),
            scheduler,
            view,
            total: None,
            more: false,
            error: None,
            count_error: false,
            task: None,
            count_task: None,
            refresh_task: None,
            debounce: None,
            detail_task: None,
            detail: None,
            request_subscription: None,
        };
        reader.bind_window(window, cx);
        reader.open_database(cx);
        reader
    }
    fn sync(&self, cx: &mut Context<Self>) {
        let status = Status {
            table: self.query.table.clone(),
            page: self.query.page,
            total: self.total,
            more: self.more,
            error: self.error.clone(),
            count_error: self.count_error,
            busy: self.task.is_some() || self.refresh_task.is_some(),
        };
        self.view.update(cx, |view, cx| view.set_status(status, cx));
        cx.notify();
    }
    fn clear_detail(&mut self, cx: &mut App) {
        self.detail = None;
        self.view.update(cx, |view, _| view.clear_detail());
    }
    fn apply_search(&mut self, cx: &mut App) {
        if self.query.search == self.search {
            return;
        }
        self.query.search = self.search.clone();
        self.query.page = 0;
        self.total = None;
        self.count_error = false;
        self.clear_detail(cx);
        self.view.update(cx, |view, _| view.reset_position());
    }
    fn request(&mut self, request: Request, window: &mut Window, cx: &mut Context<Self>) {
        if !self.active && !matches!(request, Request::SearchChanged(_)) {
            return;
        }
        match request {
            Request::SearchChanged(search) => {
                self.search = search;
                if !self.active {
                    // Input events queued before hiding still update the retained query.
                    self.apply_search(cx);
                    self.sync(cx);
                    return;
                }
                self.debounce = Some(cx.spawn(async |view, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(250))
                        .await;
                    let _ = view.update(cx, |view, cx| {
                        view.debounce = None;
                        if !view.active {
                            return;
                        }
                        view.apply_search(cx);
                        glim_services::telemetry::reader_feature(
                            "sqlite",
                            "filter_applied",
                            [(
                                "global_filter",
                                glim_services::telemetry::flag(!view.query.search.is_empty()),
                            )],
                        );
                        view.query.page = 0;
                        view.reload(cx);
                    });
                }));
            }
            Request::SelectTable(table) => {
                if self.query.table != table
                    && self
                        .database
                        .as_ref()
                        .is_some_and(|db| db.tables.contains(&table))
                {
                    self.query.table = table;
                    self.select_table(cx);
                }
            }
            Request::Sort(sort) => {
                if sort.is_some_and(|(column, _)| column >= self.query.columns.len()) {
                    return;
                }
                glim_services::telemetry::reader_feature(
                    "sqlite",
                    "sort",
                    [(
                        "direction",
                        glim_services::telemetry::text(match sort {
                            None => "default",
                            Some((_, false)) => "ascending",
                            Some((_, true)) => "descending",
                        }),
                    )],
                );
                self.query.sort = sort;
                self.query.page = 0;
                self.reload(cx);
            }
            Request::Columns(start) => {
                if start != self.query.start_column && start < self.query.columns.len() {
                    self.query.start_column = start;
                    self.load_page(cx);
                }
            }
            Request::Navigate(navigation) => {
                if self.task.is_some() || self.refresh_task.is_some() {
                    return;
                }
                let action = match navigation {
                    Navigation::First => {
                        self.query.page = 0;
                        "first_page"
                    }
                    Navigation::Previous => {
                        self.query.page = self.query.page.saturating_sub(1);
                        "previous_page"
                    }
                    Navigation::Next if self.more => {
                        self.query.page = self.query.page.saturating_add(1);
                        "next_page"
                    }
                    Navigation::Next => return,
                    Navigation::Last => {
                        self.query.page = self.last_page().unwrap_or(self.query.page);
                        "last_page"
                    }
                    Navigation::Jump(page) => {
                        self.query.page = page.min(
                            self.last_page()
                                .unwrap_or(i64::MAX as usize / limits::SQLITE_PAGE_ROWS),
                        );
                        "jump_page"
                    }
                };
                glim_services::telemetry::reader_feature("sqlite", action, []);
                self.load_page(cx);
            }
            Request::Retry => {
                glim_services::telemetry::reader_feature("sqlite", "retry_preview", []);
                self.query.seconds = self
                    .query
                    .seconds
                    .max(limits::SQLITE_QUERY_SECONDS)
                    .saturating_mul(3);
                if self.error.is_some() || self.database.is_none() {
                    self.refresh(self.path.clone(), cx);
                } else {
                    self.reload(cx);
                }
            }
            Request::Detail {
                row,
                column,
                offset,
            } => self.load_value(row, column, offset, window, cx),
            Request::CloseDetail => {
                self.detail_task = None;
                self.clear_detail(cx);
                self.sync(cx);
            }
            Request::Copy(value) => cx.write_to_clipboard(ClipboardItem::new_string(value)),
        }
    }
    fn open_database(&mut self, cx: &mut Context<Self>) {
        self.refresh_pending = false;
        self.error = None;
        self.view.update(cx, |view, cx| view.set_loading(true, cx));
        let (cancel, guard) = Cancellation::task();
        let telemetry_started = std::time::Instant::now();
        let source = self.path.clone();
        let work = reader_jobs::run_with_scheduler(
            cx.background_executor().clone(),
            self.scheduler.clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |cancel| browser::open(&source, cancel),
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result("sqlite_open", None, telemetry_started, &result);
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                match result {
                    Ok(database) => {
                        view.query.table = database.tables.first().cloned().unwrap_or_default();
                        view.view
                            .update(cx, |table, _| table.set_tables(database.tables.clone()));
                        view.database = Some(database);
                        view.select_table(cx);
                    }
                    Err(error) => {
                        view.error = Some(error.to_string());
                        view.view
                            .update(cx, |table, cx| table.set_loading(false, cx));
                    }
                }
                view.sync(cx);
            });
        }));
        self.sync(cx);
    }
    pub(super) fn source_stamp(&self) -> Option<glim_services::sqlite::SourceStamp> {
        self.database.as_ref().and_then(|db| db.source_stamp())
    }
    pub(super) fn refresh(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.active {
            self.path = path;
            self.refresh_pending = true;
            return;
        }
        if self.refresh_task.is_some() {
            return;
        }
        self.refresh_pending = false;
        self.path = path.clone();
        let previous = self.query.clone();
        let mut query = previous.clone();
        let source = path.clone();
        let (cancel, guard) = Cancellation::task();
        let work = reader_jobs::run_with_scheduler(
            cx.background_executor().clone(),
            self.scheduler.clone(),
            JobClass::Projection,
            Priority::Background,
            cancel,
            move |cancel| {
                let database = browser::open(&source, cancel)?;
                if !database.tables.contains(&query.table) {
                    query.table = database.tables.first().cloned().unwrap_or_default();
                    query.page = 0;
                }
                if query.table.is_empty() {
                    query.columns.clear();
                    query.order.clear();
                    query.sort = None;
                    query.start_column = 0;
                    return Ok((database, query, None, None));
                }
                let columns = database.columns(&query.table, cancel)?;
                if columns.names != query.columns {
                    query.sort = None;
                    query.start_column = 0;
                }
                query.columns = columns.names;
                query.order = columns.order;
                let total = database.count(&query, cancel).ok();
                if let Some(total) = total {
                    query.page = query
                        .page
                        .min(total.saturating_sub(1) as usize / limits::SQLITE_PAGE_ROWS);
                }
                let page = database.page(&query, cancel)?;
                Ok::<_, anyhow::Error>((database, query, Some(page), total))
            },
        );
        self.refresh_task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                view.refresh_task = None;
                if view.query != previous {
                    view.refresh(path, cx);
                    return;
                }
                match result {
                    Ok((database, query, page, total)) => {
                        if view.query.table != query.table || view.query.columns != query.columns {
                            view.clear_detail(cx);
                        }
                        // Old snapshot consumers stop before the replacement is published.
                        view.task = None;
                        view.count_task = None;
                        view.detail_task = None;
                        view.error = None;
                        view.count_error = total.is_none() && !query.table.is_empty();
                        view.total = total;
                        view.more = page.as_ref().is_some_and(|p| p.more);
                        let page = page.unwrap_or(SqlitePage {
                            rows: Default::default(),
                            start_column: 0,
                            more: false,
                        });
                        view.view.update(cx, |table, cx| {
                            table.set_tables(database.tables.clone());
                            table.publish(
                                page,
                                query.page.saturating_mul(limits::SQLITE_PAGE_ROWS),
                                Some(query.columns.clone()),
                                cx,
                            );
                        });
                        view.query = query;
                        view.database = Some(database);
                    }
                    Err(error) => {
                        if !view.view.read(cx).has_rows(cx)
                            && !view.query.columns.is_empty()
                            && view.database.is_some()
                        {
                            view.fetch_page(cx);
                        }
                        view.error = Some(error.to_string());
                    }
                }
                view.sync(cx);
            });
        }));
        self.sync(cx);
    }
    pub(super) fn can_move_to_window(&self) -> bool {
        self.detail_task.is_none() && self.refresh_task.is_none()
    }
    pub(super) fn bind_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.view
            .update(cx, |view, cx| view.bind_window(window, cx));
        self.request_subscription = Some(cx.subscribe_in(
            &self.view,
            window,
            |view, _, request: &Request, window, cx| view.request(request.clone(), window, cx),
        ));
    }
    fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        if let Some(detail) = &self.detail {
            detail.update(cx, |editor, cx| {
                editor
                    .0
                    .minimap
                    .update(cx, |map, cx| map.set_active(active, cx))
            });
        }
        if !active {
            self.view
                .update(cx, |view, cx| view.suspend(self.task.is_none(), cx));
            self.refresh_pending |= self.refresh_task.is_some();
            self.task = None;
            self.count_task = None;
            self.refresh_task = None;
            self.debounce = None;
            self.detail_task = None;
            self.apply_search(cx);
        } else if self.database.is_none() {
            self.open_database(cx);
        } else if self.refresh_pending {
            self.refresh(self.path.clone(), cx);
        } else if self.query.columns.is_empty() {
            self.select_table(cx);
        } else {
            self.fetch_page(cx);
            if self.total.is_none() && !self.count_error {
                self.load_count(cx);
            }
        }
        self.sync(cx);
    }
    fn last_page(&self) -> Option<usize> {
        self.total
            .map(|n| n.saturating_sub(1) as usize / limits::SQLITE_PAGE_ROWS)
    }
    fn select_table(&mut self, cx: &mut Context<Self>) {
        glim_services::telemetry::track("sqlite_table_selected", []);
        self.task = None;
        self.count_task = None;
        self.clear_detail(cx);
        self.detail_task = None;
        self.query.page = 0;
        self.query.start_column = 0;
        self.query.sort = None;
        self.query.columns.clear();
        self.total = None;
        self.more = false;
        self.error = None;
        self.count_error = false;
        self.view.update(cx, |view, cx| {
            view.reset_position();
            view.set_columns(Vec::new(), cx);
            view.set_loading(true, cx);
        });
        if !self.active {
            self.sync(cx);
            return;
        }
        let Some(db) = self.database.clone() else {
            self.sync(cx);
            return;
        };
        if self.query.table.is_empty() {
            self.view.update(cx, |view, cx| view.set_loading(false, cx));
            self.sync(cx);
            return;
        }
        let table = self.query.table.clone();
        self.debounce = None;
        self.query.search = self.search.clone();
        let (cancel, guard) = Cancellation::task();
        let telemetry_started = std::time::Instant::now();
        let work = reader_jobs::run_with_scheduler(
            cx.background_executor().clone(),
            self.scheduler.clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |cancel| db.columns(&table, cancel),
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result("sqlite_columns", None, telemetry_started, &result);
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                match result {
                    Ok(columns) => {
                        view.query.order = columns.order;
                        view.query.columns = columns.names.clone();
                        view.view
                            .update(cx, |table, cx| table.set_columns(columns.names, cx));
                        view.reload(cx);
                    }
                    Err(error) => {
                        view.error = Some(error.to_string());
                        view.view
                            .update(cx, |table, cx| table.set_loading(false, cx));
                    }
                }
                view.sync(cx);
            });
        }));
        self.sync(cx);
    }
    fn reload(&mut self, cx: &mut Context<Self>) {
        // A filter can change before column metadata arrives.
        if self.query.columns.is_empty() {
            return;
        }
        self.count_task = None;
        self.total = None;
        self.count_error = false;
        self.clear_detail(cx);
        self.detail_task = None;
        self.load_page(cx);
        self.load_count(cx);
    }
    fn load_count(&mut self, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        let Some(db) = self.database.clone() else {
            return;
        };
        let query = self.query.clone();
        let (cancel, guard) = Cancellation::task();
        let telemetry_started = std::time::Instant::now();
        let work = reader_jobs::run_with_scheduler(
            cx.background_executor().clone(),
            self.scheduler.clone(),
            JobClass::Projection,
            Priority::Background,
            cancel,
            move |cancel| db.count(&query, cancel),
        );
        self.count_task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result("sqlite_count", None, telemetry_started, &result);
            let _ = view.update(cx, |view, cx| {
                view.count_task = None;
                match result {
                    Ok(total) => view.total = Some(total),
                    Err(_) => view.count_error = true,
                }
                view.sync(cx);
            });
        }));
    }
    fn load_page(&mut self, cx: &mut Context<Self>) {
        self.view.update(cx, |view, _| view.reset_position());
        self.clear_detail(cx);
        self.fetch_page(cx);
    }
    fn fetch_page(&mut self, cx: &mut Context<Self>) {
        if !self.active {
            return;
        }
        glim_services::telemetry::track(
            "sqlite_page_requested",
            [
                (
                    "page_index",
                    glim_services::telemetry::number(self.query.page as u64),
                ),
                (
                    "column_offset",
                    glim_services::telemetry::number(self.query.start_column as u64),
                ),
                (
                    "has_search",
                    glim_services::telemetry::flag(!self.query.search.is_empty()),
                ),
                (
                    "has_sort",
                    glim_services::telemetry::flag(self.query.sort.is_some()),
                ),
            ],
        );
        self.task = None;
        self.error = None;
        self.detail_task = None;
        let Some(db) = self.database.clone() else {
            return;
        };
        if self.query.columns.is_empty() {
            return;
        }
        self.view.update(cx, |view, cx| view.begin_page(cx));
        let query = self.query.clone();
        let offset = query.page.saturating_mul(limits::SQLITE_PAGE_ROWS);
        let (cancel, guard) = Cancellation::task();
        let telemetry_started = std::time::Instant::now();
        let work = reader_jobs::run_with_scheduler(
            cx.background_executor().clone(),
            self.scheduler.clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |cancel| db.page(&query, cancel),
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result("sqlite_page_load", None, telemetry_started, &result);
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                match result {
                    Ok(page) => {
                        view.more = page.more;
                        view.view
                            .update(cx, |table, cx| table.publish(page, offset, None, cx));
                    }
                    Err(error) => {
                        glim_services::telemetry::operation_failed("sqlite_load", None, &error);
                        view.error = Some(error.to_string());
                        view.view
                            .update(cx, |table, cx| table.set_loading(false, cx));
                    }
                }
                view.sync(cx);
            });
        }));
        self.sync(cx);
    }
    fn load_value(
        &mut self,
        row: usize,
        column: usize,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.active {
            return;
        }
        self.detail_task = None;
        let Some(db) = self.database.clone() else {
            return;
        };
        glim_services::telemetry::reader_feature("sqlite", "cell_detail", []);
        let query = self.query.clone();
        let (cancel, guard) = Cancellation::task();
        let telemetry_started = std::time::Instant::now();
        let work = reader_jobs::run_with_scheduler(
            cx.background_executor().clone(),
            self.scheduler.clone(),
            JobClass::Projection,
            Priority::Interactive,
            cancel,
            move |cancel| db.value(&query, row, column, offset, cancel),
        );
        self.detail_task = Some(cx.spawn_in(window, async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            glim_services::telemetry::result("sqlite_value_load", None, telemetry_started, &result);
            let _ = view.update_in(cx, |view, window, cx| {
                view.detail_task = None;
                match result {
                    Ok((text, more)) => {
                        let editor = cx.new(|cx| {
                            ValueEditor(super::source::SourceReader::new(
                                &text,
                                "text",
                                true,
                                ScrollHandle::new(),
                                window,
                                cx,
                            ))
                        });
                        view.detail = Some(editor.clone());
                        view.view.update(cx, |table, _| {
                            table.publish_detail(Detail {
                                row,
                                column,
                                offset,
                                more,
                                content: editor.into(),
                            })
                        });
                    }
                    Err(error) => {
                        glim_services::telemetry::operation_failed("sqlite_load", None, &error);
                        view.error = Some(error.to_string());
                    }
                }
                view.sync(cx);
            });
        }));
    }
}
impl Render for SqliteReader {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.view.clone()
    }
}

impl crate::views::reader::contract::ReaderView for super::surface::SurfaceEntity<SqliteReader> {
    fn kind(&self) -> &'static str {
        "sqlite"
    }
    fn render(&mut self, _cx: &mut App) -> AnyElement {
        self.0.clone().into_any_element()
    }
    fn can_move_to_window(&self, cx: &App) -> bool {
        self.read(cx).can_move_to_window()
    }
    fn bind_window(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |view, cx| view.bind_window(window, cx));
    }
    fn set_active(&self, active: bool, cx: &mut App) {
        self.update(cx, |view, cx| view.set_active(active, cx));
    }
}
impl super::surface::ReaderSurface for super::surface::SurfaceEntity<SqliteReader> {
    fn refresh_request(
        &self,
        path: &std::path::Path,
        force: bool,
        cx: &App,
    ) -> Option<glim_services::documents::refresh::RefreshRequest> {
        Some(glim_services::documents::refresh::RefreshRequest::Sqlite {
            path: path.to_path_buf(),
            stamp: if force {
                None
            } else {
                self.read(cx).source_stamp()
            },
        })
    }
    fn refresh_in_place(
        &self,
        content: &glim_services::documents::Content,
        _on_failure: super::surface::RefreshFailureHandler,
        cx: &mut App,
    ) -> bool {
        let glim_services::documents::Content::Sqlite(path) = content else {
            return false;
        };
        self.update(cx, |view, cx| view.refresh(path.clone(), cx));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Navigation, Request, Scheduler, SqliteReader, limits};
    use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext, WindowOptions};

    fn settle(cx: &mut TestAppContext, reader: &Entity<SqliteReader>) {
        cx.run_until_parked();
        cx.update(|cx| {
            let reader = reader.read(cx);
            assert!(
                reader.task.is_none()
                    && reader.count_task.is_none()
                    && reader.refresh_task.is_none()
                    && reader.detail_task.is_none(),
                "SQLite tasks did not settle"
            );
        });
    }
    fn create_database(path: &std::path::Path) {
        // macOS ships sqlite3; only fixture setup uses the command-line tool.
        let result = std::process::Command::new("/usr/bin/sqlite3")
            .arg(path)
            .arg(include_str!(
                "../../../../../assets/reader-contracts/sqlite.sql"
            ))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fn open(
        cx: &mut TestAppContext,
        path: &std::path::Path,
    ) -> (AnyWindowHandle, Entity<SqliteReader>) {
        cx.update(gpui_kit::init);
        let (handle, reader) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    SqliteReader::with_scheduler(
                        path.to_path_buf(),
                        Scheduler::from_policy(),
                        window,
                        cx,
                    )
                })
            })
            .unwrap()
        });
        settle(cx, &reader);
        (handle, reader)
    }
    fn emit(cx: &mut TestAppContext, reader: &Entity<SqliteReader>, request: Request) {
        cx.update(|cx| {
            let view = reader.read(cx).view.clone();
            view.update(cx, |_, cx| cx.emit(request));
        });
    }
    fn cells(cx: &mut TestAppContext, reader: &Entity<SqliteReader>) -> Vec<Vec<String>> {
        cx.update(|cx| {
            let reader = reader.read(cx);
            let cells = reader.view.read(cx).visible_cells(cx);
            assert!(
                !cells.is_empty(),
                "table={} error={:?} page_task={} count_task={} refresh_task={} active={}",
                reader.query.table,
                reader.error,
                reader.task.is_some(),
                reader.count_task.is_some(),
                reader.refresh_task.is_some(),
                reader.active
            );
            cells
        })
    }

    #[gpui_kit::test]
    fn page_sort_filter_and_detail_survive_cancellation_and_reactivation(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.sqlite");
        create_database(&path);
        let original = std::fs::read(&path).unwrap();
        let (handle, reader) = open(cx, &path);
        assert_eq!(cells(cx, &reader).len(), limits::SQLITE_PAGE_ROWS);
        assert_eq!(cells(cx, &reader)[0][2], "line 1\nline 2 \"yes\"");
        emit(cx, &reader, Request::Navigate(Navigation::Next));
        settle(cx, &reader);
        assert_eq!(cells(cx, &reader).len(), 520 - limits::SQLITE_PAGE_ROWS);
        assert_eq!(
            cells(cx, &reader)[0][0],
            (limits::SQLITE_PAGE_ROWS + 1).to_string()
        );

        emit(cx, &reader, Request::Sort(Some((0, true))));
        emit(cx, &reader, Request::SelectTable("beta".into()));
        emit(cx, &reader, Request::SelectTable("alpha".into()));
        settle(cx, &reader);
        assert_eq!(cells(cx, &reader)[0][0], "1");
        emit(cx, &reader, Request::Sort(Some((0, true))));
        settle(cx, &reader);
        assert_eq!(cells(cx, &reader)[0][0], "520");

        cx.update_window(handle, |_, window, cx| {
            let input = reader.read(cx).view.read(cx).search_input();
            input.update(cx, |input, cx| {
                input.set_value("record 520", window, cx);
                cx.emit(gpui_kit::component::input::InputEvent::Change);
            });
            reader.update(cx, |reader, cx| {
                reader.set_active(false, cx);
                reader.set_active(false, cx);
                assert!(
                    reader.task.is_none()
                        && reader.count_task.is_none()
                        && reader.debounce.is_none()
                );
                assert!(!reader.view.read(cx).has_rows(cx));
            })
        })
        .unwrap();
        settle(cx, &reader);
        cx.update(|cx| {
            reader.update(cx, |reader, cx| {
                assert_eq!(reader.query.search, "record 520");
                assert!(!reader.view.read(cx).has_rows(cx));
                reader.set_active(true, cx);
            })
        });
        settle(cx, &reader);
        assert_eq!(cells(cx, &reader), [vec!["520", "中文 520", "record 520"]]);
        cx.update(|cx| assert_eq!(reader.read(cx).total, Some(1)));

        emit(
            cx,
            &reader,
            Request::Detail {
                row: 0,
                column: 2,
                offset: 0,
            },
        );
        settle(cx, &reader);
        let detail = cx.update(|cx| reader.read(cx).detail.clone().expect("loaded detail"));
        cx.update(|cx| {
            assert_eq!(
                detail.read(cx).0.edit.editor().read(cx).value().as_ref(),
                "record 520"
            );
            reader.update(cx, |reader, cx| {
                reader.set_active(false, cx);
                reader.set_active(true, cx);
                assert_eq!(
                    reader.detail.as_ref().unwrap().entity_id(),
                    detail.entity_id()
                );
            });
        });
        settle(cx, &reader);
        emit(
            cx,
            &reader,
            Request::Detail {
                row: 0,
                column: 1,
                offset: 0,
            },
        );
        emit(cx, &reader, Request::CloseDetail);
        settle(cx, &reader);
        cx.update(|cx| assert!(reader.read(cx).detail.is_none()));
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[gpui_kit::test]
    fn failed_hidden_refresh_restores_old_rows_and_stale_refresh_keeps_new_selection(
        cx: &mut TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.sqlite");
        create_database(&path);
        let (_, reader) = open(cx, &path);
        cx.update(|cx| {
            reader.update(cx, |reader, cx| {
                reader.set_active(false, cx);
                reader.refresh(directory.path().join("missing.sqlite"), cx);
                reader.set_active(true, cx);
            })
        });
        settle(cx, &reader);
        assert_eq!(cells(cx, &reader)[0][0], "1");
        cx.update(|cx| {
            let reader = reader.read(cx);
            assert!(reader.error.is_some());
            assert!(reader.task.is_none() && reader.refresh_task.is_none());
        });
        cx.update(|cx| reader.update(cx, |reader, cx| reader.refresh(path.clone(), cx)));
        emit(cx, &reader, Request::SelectTable("beta".into()));
        settle(cx, &reader);
        assert_eq!(cells(cx, &reader), [vec!["retained beta"]]);
        cx.update(|cx| {
            let reader = reader.read(cx);
            assert_eq!(reader.query.table, "beta");
            assert!(reader.error.is_none());
            assert!(reader.refresh_task.is_none());
            assert_eq!(reader.total, Some(1));
        });
    }

    #[gpui_kit::test]
    fn retry_recovers_a_missing_database_without_replacing_presentation(cx: &mut TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("later.sqlite");
        let (_, reader) = open(cx, &path);
        let presentation = cx.update(|cx| {
            let reader = reader.read(cx);
            assert!(reader.error.is_some());
            reader.view.entity_id()
        });
        create_database(&path);
        emit(cx, &reader, Request::Retry);
        settle(cx, &reader);
        assert_eq!(cells(cx, &reader)[0][0], "1");
        cx.update(|cx| {
            let reader = reader.read(cx);
            assert!(reader.error.is_none());
            assert_eq!(reader.view.entity_id(), presentation);
            assert_eq!(reader.total, Some(520));
        });
    }
}
