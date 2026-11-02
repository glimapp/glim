//! Text projections shared by full-buffer and paged readers.
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

pub struct TextPresentation {
    pub markup: bool,
    pub html: bool,
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
        let html = matches!(extension.as_str(), "html" | "htm");
        let projected = crate::text_formats::supports(path);
        let table = match extension.as_str() {
            "csv" => Some(TableFormat::Csv),
            "tsv" => Some(TableFormat::Tsv),
            "gpx" => Some(TableFormat::Gpx),
            "kml" => Some(TableFormat::Kml),
            _ => None,
        };
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
            html,
            table,
            json_dialect,
            markup: html || projected || DocumentKind::from_path(path) == DocumentKind::Markdown,
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
        assert!(TextPresentation::for_path(Path::new("index.HTM")).html);
        assert!(TextPresentation::for_path(Path::new("README.md")).markup);
    }
}
