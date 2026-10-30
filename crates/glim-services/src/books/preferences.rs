use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadingState {
    pub chapter: usize,
    pub fraction: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub page_width: f32,
    pub serif: bool,
    pub paper: bool,
    pub bookmarks: Vec<Bookmark>,
}
impl Default for ReadingState {
    fn default() -> Self {
        Self {
            chapter: 0,
            fraction: 0.,
            font_size: 19.,
            line_height: 1.7,
            page_width: 720.,
            serif: true,
            paper: true,
            bookmarks: Vec::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub chapter: usize,
    pub fraction: f32,
    pub label: String,
}
fn validate_identity(fingerprint: &str) -> Result<()> {
    let id = fingerprint.strip_prefix("ebook:").unwrap_or_default();
    ensure!(
        id.len() == 64 && id.bytes().all(|v| v.is_ascii_hexdigit()),
        "Invalid book identity"
    );
    Ok(())
}
pub fn read(fingerprint: &str) -> Result<ReadingState> {
    validate_identity(fingerprint)?;
    read_from(&crate::storage::open()?, fingerprint)
}
fn read_from(db: &rusqlite::Connection, fingerprint: &str) -> Result<ReadingState> {
    let state = crate::storage::record(db, "book", fingerprint)?
        .map(|v| serde_json::from_str::<ReadingState>(&v))
        .transpose()?
        .unwrap_or_default();
    validate(&state)?;
    Ok(state)
}
pub(crate) fn validate(state: &ReadingState) -> Result<()> {
    ensure!(
        state.fraction.is_finite()
            && (0. ..=1.).contains(&state.fraction)
            && (14. ..=32.).contains(&state.font_size)
            && (1.3..=2.2).contains(&state.line_height)
            && (480. ..=1000.).contains(&state.page_width)
            && state
                .bookmarks
                .iter()
                .all(|b| b.fraction.is_finite() && (0. ..=1.).contains(&b.fraction)),
        "Reading preferences are outside supported limits"
    );
    Ok(())
}
pub fn save(fingerprint: &str, state: &ReadingState) -> Result<()> {
    validate_identity(fingerprint)?;
    save_to(&crate::storage::open()?, fingerprint, state)
}
fn save_to(db: &rusqlite::Connection, fingerprint: &str, state: &ReadingState) -> Result<()> {
    validate(state)?;
    crate::storage::set_record(db, "book", fingerprint, &serde_json::to_string(state)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reading_position_typography_and_bookmarks_roundtrip_without_touching_book_files() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::storage::open_at(dir.path()).unwrap();
        let fingerprint = format!("ebook:{}", "a".repeat(64));
        let mut state = ReadingState {
            chapter: 3,
            fraction: 0.625,
            font_size: 23.,
            page_width: 880.,
            ..Default::default()
        };
        state.bookmarks.push(Bookmark {
            chapter: 1,
            fraction: 0.25,
            label: "Chapter one".into(),
        });
        save_to(&db, &fingerprint, &state).unwrap();
        let restored = read_from(&db, &fingerprint).unwrap();
        assert_eq!(restored.chapter, 3);
        assert_eq!(restored.fraction, 0.625);
        assert_eq!(restored.font_size, 23.);
        assert_eq!(restored.bookmarks[0].fraction, 0.25);
        state.fraction = f32::NAN;
        assert!(save_to(&db, &fingerprint, &state).is_err());
        assert_eq!(read_from(&db, &fingerprint).unwrap().fraction, 0.625);
    }
}
