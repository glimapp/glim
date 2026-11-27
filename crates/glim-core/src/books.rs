//! Book catalog values and reading state, independent of storage and presentation.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Epub,
    FictionBook,
    Comic,
}
impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Self::Epub => "EPUB",
            Self::FictionBook => "FB2",
            Self::Comic => "CBZ",
        }
    }
}
#[derive(Clone)]
pub struct Chapter {
    pub href: String,
    pub title: String,
    pub html: Option<String>,
}
#[derive(Clone)]
pub struct TocEntry {
    pub title: String,
    pub href: String,
    pub depth: usize,
}
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
impl ReadingState {
    pub const MIN_FONT_SIZE: f32 = 14.;
    pub const MAX_FONT_SIZE: f32 = 32.;
    const FONT_SIZE_STEP: f32 = 2.;
    const DEFAULT_LINE_HEIGHT: f32 = 1.7;
    const DEFAULT_PAGE_WIDTH: f32 = 720.;

    pub fn is_valid(&self) -> bool {
        (0. ..=1.).contains(&self.fraction)
            && (Self::MIN_FONT_SIZE..=Self::MAX_FONT_SIZE).contains(&self.font_size)
            && (1.3..=2.2).contains(&self.line_height)
            && (480. ..=1000.).contains(&self.page_width)
            && self
                .bookmarks
                .iter()
                .all(|b| (0. ..=1.).contains(&b.fraction))
    }

    pub fn decrease_font_size(&mut self) {
        self.font_size = (self.font_size - Self::FONT_SIZE_STEP).max(Self::MIN_FONT_SIZE);
    }

    pub fn increase_font_size(&mut self) {
        self.font_size = (self.font_size + Self::FONT_SIZE_STEP).min(Self::MAX_FONT_SIZE);
    }

    pub fn cycle_line_height(&mut self) {
        self.line_height = if self.line_height < 1.6 {
            Self::DEFAULT_LINE_HEIGHT
        } else if self.line_height < 1.9 {
            2.0
        } else {
            1.5
        };
    }

    pub fn cycle_page_width(&mut self) {
        self.page_width = if self.page_width < 650. {
            Self::DEFAULT_PAGE_WIDTH
        } else if self.page_width < 800. {
            880.
        } else {
            560.
        };
    }
}
impl Default for ReadingState {
    fn default() -> Self {
        Self {
            chapter: 0,
            fraction: 0.,
            font_size: 19.,
            line_height: Self::DEFAULT_LINE_HEIGHT,
            page_width: Self::DEFAULT_PAGE_WIDTH,
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

#[cfg(test)]
mod tests {
    use super::ReadingState;

    #[test]
    fn reading_controls_stay_within_the_persisted_limits() {
        let mut state = ReadingState::default();
        assert!(state.is_valid());
        for _ in 0..12 {
            state.decrease_font_size();
            assert!(state.is_valid());
        }
        assert_eq!(state.font_size, ReadingState::MIN_FONT_SIZE);
        for _ in 0..12 {
            state.increase_font_size();
            assert!(state.is_valid());
        }
        assert_eq!(state.font_size, ReadingState::MAX_FONT_SIZE);
        for (width, height) in [(880., 2.), (560., 1.5), (720., 1.7)] {
            state.cycle_page_width();
            state.cycle_line_height();
            assert_eq!((state.page_width, state.line_height), (width, height));
            assert!(state.is_valid());
        }
    }
}
