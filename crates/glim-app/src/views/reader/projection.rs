//! Optional data views share a source editor and one selected projection.
use super::{csv::CsvTable, json::JsonTree};
use glim_core::resources::{TextSnapshot, WorkingPressure};
use glim_services::documents::presentation::{TableFormat, TextPresentation};
use gpui_kit::*;
use std::path::Path;
use std::sync::Arc;

pub(super) struct Projection {
    view: View,
    visible: bool,
    error: Option<String>,
}
enum View {
    Table {
        view: Entity<CsvTable>,
        format: TableFormat,
    },
    Json(Entity<JsonTree>),
}
impl Projection {
    pub fn new(presentation: &TextPresentation, file: Option<&Path>, cx: &mut App) -> Option<Self> {
        let view = if let Some(format) = presentation.table {
            View::Table {
                view: cx.new(|_| match file {
                    Some(path) => CsvTable::from_file(path.to_path_buf(), format),
                    None => CsvTable::new(),
                }),
                format,
            }
        } else if file.is_none() {
            let dialect = presentation.json_dialect.clone()?;
            View::Json(cx.new(|cx| JsonTree::new(dialect, cx)))
        } else {
            return None;
        };
        Some(Self {
            view,
            visible: false,
            error: None,
        })
    }
    pub fn table_format(&self) -> Option<TableFormat> {
        match self.view {
            View::Table { format, .. } => Some(format),
            View::Json(_) => None,
        }
    }
    pub fn is_json(&self) -> bool {
        matches!(self.view, View::Json(_))
    }
    pub fn visible(&self) -> bool {
        self.visible
    }
    pub fn snapshot<'a>(
        &self,
        chunks: impl Iterator<Item = &'a str> + Clone,
        cx: &App,
    ) -> Result<Arc<TextSnapshot>, WorkingPressure> {
        let current = match &self.view {
            View::Table { view, .. } => view.read(cx).input_snapshot(),
            View::Json(view) => view.read(cx).input_snapshot(),
        };
        if let Some(current) = current
            && current.matches_chunks(chunks.clone())
        {
            return Ok(current.clone());
        }
        TextSnapshot::from_chunks(chunks, glim_services::resources::pool()).map(Arc::new)
    }
    pub fn set_visible(
        &mut self,
        visible: bool,
        text: Option<Result<Arc<TextSnapshot>, WorkingPressure>>,
        cx: &mut App,
    ) {
        self.visible = visible;
        if !visible {
            self.set_active(false, cx);
            return;
        }
        self.error = None;
        let text = match text.transpose() {
            Ok(text) => text,
            Err(error) => {
                self.error = Some(format!("{error}. Switch to Source and retry the preview."));
                self.set_active(false, cx);
                return;
            }
        };
        match &self.view {
            View::Table { view, format } => view.update(cx, |table, cx| match text {
                Some(text) => match format.geo_name() {
                    Some(name) => table.load_geo(text, name, cx),
                    None => table.load(text, format.delimiter(), cx),
                },
                None => table.reload(cx),
            }),
            View::Json(view) => {
                if let Some(text) = text {
                    view.update(cx, |tree, cx| tree.load(text, cx));
                }
            }
        }
    }
    pub fn set_active(&self, active: bool, cx: &mut App) {
        let active = active && self.visible && self.error.is_none();
        match &self.view {
            View::Table { view, .. } => view.update(cx, |table, cx| {
                if active {
                    table.resume(cx);
                } else {
                    table.suspend();
                }
            }),
            View::Json(view) => view.update(cx, |tree, cx| {
                if active {
                    tree.resume(cx);
                } else {
                    tree.suspend(cx);
                }
            }),
        }
    }
    pub fn render(&self) -> Option<AnyElement> {
        self.visible.then(|| {
            if let Some(error) = &self.error {
                return div().p_4().child(error.clone()).into_any_element();
            }
            match &self.view {
                View::Table { view, .. } => view.clone().into_any_element(),
                View::Json(view) => view.clone().into_any_element(),
            }
        })
    }
}
