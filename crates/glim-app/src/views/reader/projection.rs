//! Optional data views share a source editor and one selected projection.
use super::{csv::CsvTable, json::JsonTree};
use glim_services::documents::presentation::{TableFormat, TextPresentation};
use gpui_kit::*;
use std::path::Path;

pub(super) struct Projection {
    view: View,
    visible: bool,
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
            View::Json(cx.new(|_| JsonTree::new(dialect)))
        } else {
            return None;
        };
        Some(Self {
            view,
            visible: false,
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
    pub fn set_visible(&mut self, visible: bool, text: Option<String>, cx: &mut App) {
        self.visible = visible;
        if !visible {
            return;
        }
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
    pub fn render(&self) -> Option<AnyElement> {
        self.visible.then(|| match &self.view {
            View::Table { view, .. } => view.clone().into_any_element(),
            View::Json(view) => view.clone().into_any_element(),
        })
    }
}
