//! Text projections shared by full-buffer and paged readers.
use glim_core::formats::FormatSet;
use glim_core::{DocumentKind, language_for_path};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableFormat {
    Csv,
    Tsv,
    Gpx,
    Kml,
}
impl TableFormat {
    pub fn delimiter(self) -> u8 {
        if self == Self::Tsv { b'\t' } else { b',' }
    }
    pub fn geo_name(self) -> Option<&'static str> {
        match self {
            Self::Gpx => Some("GPX"),
            Self::Kml => Some("KML"),
            _ => None,
        }
    }
    pub fn is_delimited(self) -> bool {
        matches!(self, Self::Csv | Self::Tsv)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkupFormat {
    Markdown,
    Html,
    Converted,
}

pub struct TextPresentation {
    pub markup: Option<MarkupFormat>,
    pub structured: bool,
    pub json_dialect: Option<String>,
    pub table: Option<TableFormat>,
}
impl TextPresentation {
    pub fn for_path(path: &Path) -> Self {
        let extension = path
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let html = FormatSet::Html.matches(path);
        let projected = crate::text_formats::supports(path);
        let table = [
            (FormatSet::TableCsv, TableFormat::Csv),
            (FormatSet::TableTsv, TableFormat::Tsv),
            (FormatSet::TableGpx, TableFormat::Gpx),
            (FormatSet::TableKml, TableFormat::Kml),
        ]
        .into_iter()
        .find_map(|(set, format)| set.matches(path).then_some(format));
        let json_dialect = (language_for_path(path) == "json" && !projected).then(|| {
            if extension == "code-workspace" {
                "jsonc".into()
            } else if extension.is_empty() {
                "json".into()
            } else {
                extension
            }
        });
        Self {
            structured: crate::graph::supports(path),
            table,
            json_dialect,
            markup: if html {
                Some(MarkupFormat::Html)
            } else if projected {
                Some(MarkupFormat::Converted)
            } else if DocumentKind::from_path(path) == DocumentKind::Markdown {
                Some(MarkupFormat::Markdown)
            } else {
                None
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projections_cover_named_configs_and_case_insensitive_formats() {
        assert_eq!(
            TextPresentation::for_path(Path::new(".prettierrc"))
                .json_dialect
                .as_deref(),
            Some("json")
        );
        assert_eq!(
            TextPresentation::for_path(Path::new("project.code-workspace"))
                .json_dialect
                .as_deref(),
            Some("jsonc")
        );
        assert_eq!(
            TextPresentation::for_path(Path::new("TABLE.TSV")).table,
            Some(TableFormat::Tsv)
        );
        assert_eq!(
            TextPresentation::for_path(Path::new("track.GPX")).table,
            Some(TableFormat::Gpx)
        );
        assert_eq!(
            TextPresentation::for_path(Path::new("index.HTM")).markup,
            Some(MarkupFormat::Html)
        );
        assert_eq!(
            TextPresentation::for_path(Path::new("README.md")).markup,
            Some(MarkupFormat::Markdown)
        );
    }
}
