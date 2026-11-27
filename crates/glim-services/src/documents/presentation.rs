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
    pub fn for_path(path: &Path) -> Option<Self> {
        [
            (FormatSet::TableCsv, Self::Csv),
            (FormatSet::TableTsv, Self::Tsv),
            (FormatSet::TableGpx, Self::Gpx),
            (FormatSet::TableKml, Self::Kml),
        ]
        .into_iter()
        .find_map(|(set, format)| set.matches(path).then_some(format))
    }

    pub fn delimiter(self) -> Option<u8> {
        match self {
            Self::Csv => Some(b','),
            Self::Tsv => Some(b'\t'),
            Self::Gpx | Self::Kml => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Tsv => "TSV",
            Self::Gpx => "GPX",
            Self::Kml => "KML",
        }
    }

    pub fn geo_name(self) -> Option<&'static str> {
        (!self.is_delimited()).then(|| self.name())
    }

    pub fn is_delimited(self) -> bool {
        self.delimiter().is_some()
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
            table: TableFormat::for_path(path),
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
    fn only_delimited_table_formats_expose_a_delimiter() {
        for (path, format, delimiter) in [
            ("data.CSV", Some(TableFormat::Csv), Some(b',')),
            ("data.tsv", Some(TableFormat::Tsv), Some(b'\t')),
            ("route.gpx", Some(TableFormat::Gpx), None),
            ("route.kml", Some(TableFormat::Kml), None),
            ("other.txt", None, None),
        ] {
            let detected = TableFormat::for_path(Path::new(path));
            assert_eq!(detected, format);
            assert_eq!(detected.and_then(TableFormat::delimiter), delimiter);
        }
    }

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
