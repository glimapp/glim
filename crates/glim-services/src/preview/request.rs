//! Versioned Rust/Swift contract; each preview kind requires its own payload.
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct NativeRequest {
    version: u32,
    localizations: serde_json::Value,
    content: NativeContent,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeContent {
    OpenApi {
        source: PathBuf,
        database: PathBuf,
    },
    Pdf {
        source: PathBuf,
    },
    Image {
        source: PathBuf,
        prepared: Option<PathBuf>,
        size: Option<ImageSize>,
    },
    Font {
        source: PathBuf,
    },
    Office {
        source: PathBuf,
        reading: Option<PathBuf>,
        image_pages: bool,
    },
    Workbook {
        source: PathBuf,
        database: PathBuf,
        cache: PathBuf,
        helper: PathBuf,
        information: String,
    },
    Html {
        source: PathBuf,
        text: Option<String>,
    },
    Media {
        files: Vec<PathBuf>,
        shuffle: bool,
        autoplay: bool,
        preparation: Option<MediaPreparation>,
    },
    Associations {
        bundle: PathBuf,
    },
}

#[derive(Serialize)]
pub struct ImageSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize)]
pub struct MediaPreparation {
    pub files: Vec<String>,
    pub helper: PathBuf,
    pub directory: PathBuf,
}

impl NativeRequest {
    pub fn new(content: NativeContent) -> Self {
        Self {
            version: 1,
            localizations: crate::locale::translations(),
            content,
        }
    }
    pub fn encode(&self) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_encoding_matches_the_swift_contract_fixtures() -> anyhow::Result<()> {
        let cases = [
            NativeContent::Pdf {
                source: "/documents/read me.pdf".into(),
            },
            NativeContent::Image {
                source: "/documents/图片.png".into(),
                prepared: Some("/cache/image.png".into()),
                size: Some(ImageSize {
                    width: 640,
                    height: 480,
                }),
            },
            NativeContent::Font {
                source: "/documents/font.otf".into(),
            },
            NativeContent::Office {
                source: "/documents/report.docx".into(),
                reading: Some("/cache/reading.html".into()),
                image_pages: true,
            },
            NativeContent::Workbook {
                source: "/documents/table.xlsx".into(),
                database: "/cache/index.sqlite".into(),
                cache: "/cache/sheets".into(),
                helper: "/Applications/Glim.app/Contents/MacOS/glim".into(),
                information: "two sheets".into(),
            },
            NativeContent::Html {
                source: "/documents/page.html".into(),
                text: Some("<p>中文</p>".into()),
            },
            NativeContent::OpenApi {
                source: "/documents/openapi.json".into(),
                database: "/cache/api/index.sqlite".into(),
            },
            NativeContent::Media {
                files: vec!["/documents/movie.mkv".into()],
                shuffle: false,
                autoplay: false,
                preparation: Some(MediaPreparation {
                    files: vec!["/documents/movie.mkv".into()],
                    helper: "/Applications/Glim.app/Contents/MacOS/glim".into(),
                    directory: "/cache/media".into(),
                }),
            },
            NativeContent::Associations {
                bundle: "/Applications/Glim.app".into(),
            },
        ];
        let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(
            "../../../../native/tests/fixtures/preview-requests.json"
        ))?;
        assert_eq!(cases.len(), fixtures.len());
        for (content, expected) in cases.into_iter().zip(fixtures) {
            let mut request = NativeRequest::new(content);
            request.localizations = serde_json::json!({});
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&request.encode()?)?,
                expected
            );
        }
        Ok(())
    }
}
