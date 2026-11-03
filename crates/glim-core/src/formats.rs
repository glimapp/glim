//! Compile-time declarations. Content probes and parser validity stay in services.
use std::path::Path;

include!("formats_data.rs");

struct FormatRule {
    suffixes: &'static [&'static str],
    tails: &'static [&'static str],
    filename_suffix: bool,
}

impl FormatSet {
    pub fn matches(self, path: &Path) -> bool {
        let rule = self.rule();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let extension = path.extension().and_then(|ext| ext.to_str());
        rule.suffixes.iter().any(|suffix| {
            if rule.filename_suffix || suffix.contains('.') {
                name.len() > suffix.len()
                    && name.as_bytes()[name.len() - suffix.len() - 1] == b'.'
                    && name
                        .get(name.len() - suffix.len()..)
                        .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
            } else {
                extension.is_some_and(|ext| ext.eq_ignore_ascii_case(suffix))
            }
        }) || rule.tails.iter().any(|tail| {
            name.get(name.len().saturating_sub(tail.len())..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(tail))
        })
    }
}

pub fn language_for_extension(extension: &str) -> &'static str {
    let extension = extension.to_ascii_lowercase();
    LANGUAGES
        .binary_search_by_key(&extension.as_str(), |(suffix, _)| *suffix)
        .map(|index| LANGUAGES[index].1)
        .unwrap_or("plain")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compound_formats_and_sidecars_preserve_detection_boundaries() {
        for path in ["book.FB2.ZIP", ".fb2.zip", "book.epub"] {
            assert!(FormatSet::Book.matches(Path::new(path)));
        }
        assert!(!FormatSet::Book.matches(Path::new("book.fb2.zip.bak")));
        assert!(FormatSet::ConvertedMarkup.matches(Path::new("plot.VEGA.JSON")));
        assert!(!FormatSet::ConvertedMarkup.matches(Path::new("plain.json")));
        assert!(FormatSet::Sqlite.matches(Path::new("database-journal")));
        assert!(FormatSet::Sqlite.matches(Path::new("unknown.DB-WAL")));
        assert!(!FormatSet::Image.matches(Path::new(".png")));
        assert!(!FormatSet::Image.matches(Path::new("photo.png.backup")));
        assert!(!FormatSet::Image.matches(Path::new("🦀")));
        assert_eq!(language_for_extension("RS"), "rust");
        assert_eq!(language_for_extension("unknown"), "plain");
    }
}
