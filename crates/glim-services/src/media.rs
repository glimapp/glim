//! Bounded image decoding, kept off the UI executor.
use crate::limits as reader_limits;
use anyhow::{Context, Result, ensure};
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

const GIF_COMPOSITING_BUFFERS: u64 = 5;

pub struct ImageDocument {
    pub path: PathBuf,
    pub png: Vec<u8>,
    pub fingerprint: String,
    pub gif: bool,
    pub pages: Vec<Vec<u8>>,
}
pub fn supports(path: &Path) -> bool {
    glim_core::formats::FormatSet::Image.matches(path)
}
pub fn read_image(path: &Path) -> Result<ImageDocument> {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"))
    {
        let stamp = std::fs::metadata(path)?;
        GifPlayback::from_file(path.to_path_buf())?;
        return Ok(ImageDocument {
            path: std::path::absolute(path)?,
            png: Vec::new(),
            fingerprint: format!("gif:{}:{:?}", stamp.len(), stamp.modified().ok()),
            gif: true,
            pages: Vec::new(),
        });
    }
    let stamp = std::fs::metadata(path)?;
    let png = crate::inline_images::thumbnail_png(path, reader_limits::IMAGE_PREVIEW_EDGE)?;
    Ok(ImageDocument {
        path: std::path::absolute(path)?,
        png,
        fingerprint: format!("image:{}:{:?}", stamp.len(), stamp.modified().ok()),
        gif: false,
        pages: Vec::new(),
    })
}

/// Sequential, original-resolution GIF decoding with bounded compositing state.
/// The UI decodes small batches on its background executor. Display previews
/// are bounded independently of the original-resolution compositing canvas.
#[derive(Clone)]
pub enum GifInput {
    Bytes(std::sync::Arc<[u8]>),
    File(PathBuf),
    Temporary(std::sync::Arc<tempfile::NamedTempFile>),
}
impl GifInput {
    fn open(&self) -> Result<Box<dyn Read + Send>> {
        Ok(match self {
            Self::Bytes(bytes) => Box::new(Cursor::new(bytes.clone())),
            Self::File(path) => Box::new(std::io::BufReader::new(std::fs::File::open(path)?)),
            Self::Temporary(file) => Box::new(std::io::BufReader::new(file.reopen()?)),
        })
    }
}
pub struct GifPlayback {
    memory: glim_core::resources::ResourceLease,
    input: GifInput,
    cancel: crate::work::Cancellation,
    decoder: gif::Decoder<Box<dyn Read + Send>>,
    logical: (u32, u32),
    canvas: image::RgbaImage,
    restore: Option<image::RgbaImage>,
    disposal: gif::DisposalMethod,
    rectangle: (u32, u32, u32, u32),
    frames: usize,
    loops: u32,
    finished: bool,
}
impl GifPlayback {
    pub fn new(bytes: std::sync::Arc<[u8]>) -> Result<Self> {
        Self::from_input(GifInput::Bytes(bytes))
    }
    pub fn from_file(path: PathBuf) -> Result<Self> {
        Self::from_input(GifInput::File(path))
    }
    pub fn from_input(input: GifInput) -> Result<Self> {
        Self::from_input_cancellable(input, crate::work::Cancellation::default())
    }
    pub fn from_input_cancellable(
        input: GifInput,
        cancel: crate::work::Cancellation,
    ) -> Result<Self> {
        let decoder = Self::decoder(&input, &cancel)?;
        let logical = (u32::from(decoder.width()), u32::from(decoder.height()));
        ensure!(logical.0 > 0 && logical.1 > 0, "GIF has an empty canvas");
        let budget_pixels =
            (reader_limits::GIF_CANVAS_BYTES / (4 * GIF_COMPOSITING_BUFFERS)).max(1);
        let scale = (budget_pixels as f64 / (f64::from(logical.0) * f64::from(logical.1)))
            .sqrt()
            .min(1.);
        let width = ((f64::from(logical.0) * scale).floor() as u32).max(1);
        let height = ((f64::from(logical.1) * scale).floor() as u32).max(1);
        Ok(Self {
            memory: crate::resources::pool().track(
                u64::from(width) * u64::from(height) * 4,
                glim_core::resources::ResourceClass::Working,
            ),
            input,
            cancel,
            decoder,
            logical,
            canvas: image::RgbaImage::new(width, height),
            restore: None,
            disposal: gif::DisposalMethod::Keep,
            rectangle: (0, 0, 0, 0),
            frames: 0,
            loops: 0,
            finished: false,
        })
    }
    fn decoder(
        input: &GifInput,
        cancel: &crate::work::Cancellation,
    ) -> Result<gif::Decoder<Box<dyn Read + Send>>> {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        options.check_frame_consistency(true);
        Ok(options.read_info(Box::new(crate::work::Reader {
            inner: input.open()?,
            cancel: cancel.clone(),
        }) as Box<dyn Read + Send>)?)
    }

    pub fn preview_frame_bytes(&self) -> usize {
        let (width, height) = self.preview_dimensions();
        width as usize * height as usize * 4
    }

    fn preview_dimensions(&self) -> (u32, u32) {
        let (width, height) = self.canvas.dimensions();
        let edge = width.max(height);
        if edge <= reader_limits::GIF_PREVIEW_EDGE {
            (width, height)
        } else {
            (
                ((u64::from(width) * u64::from(reader_limits::GIF_PREVIEW_EDGE) / u64::from(edge))
                    as u32)
                    .max(1),
                ((u64::from(height) * u64::from(reader_limits::GIF_PREVIEW_EDGE) / u64::from(edge))
                    as u32)
                    .max(1),
            )
        }
    }

    pub fn next_frame(&mut self) -> Result<Option<(image::Frame, std::time::Duration)>> {
        self.decode_frame(false)
    }

    pub fn next_preview_frame(&mut self) -> Result<Option<(image::Frame, std::time::Duration)>> {
        self.decode_frame(true)
    }

    fn decode_frame(
        &mut self,
        preview: bool,
    ) -> Result<Option<(image::Frame, std::time::Duration)>> {
        if self.finished {
            return Ok(None);
        }
        match self.disposal {
            gif::DisposalMethod::Background => {
                let (left, top, width, height) = self.rectangle;
                let stride = self.canvas.width() as usize * 4;
                let pixels: &mut [u8] = self.canvas.as_mut();
                for y in top..top + height {
                    let start = y as usize * stride + left as usize * 4;
                    pixels[start..start + width as usize * 4].fill(0);
                }
            }
            gif::DisposalMethod::Previous => {
                if let Some(restore) = self.restore.take() {
                    self.canvas = restore;
                }
            }
            _ => {}
        }
        if self.decoder.next_frame_info()?.is_none() {
            ensure!(self.frames > 0, "GIF contains no frames");
            self.loops = self.loops.saturating_add(1);
            if let gif::Repeat::Finite(repeats) = self.decoder.repeat()
                && self.loops > u32::from(repeats)
            {
                self.finished = true;
                return Ok(None);
            }
            self.decoder = Self::decoder(&self.input, &self.cancel)?;
            self.canvas.fill(0);
            self.restore = None;
            self.frames = 0;
            ensure!(
                self.decoder.next_frame_info()?.is_some(),
                "GIF contains no frames"
            );
        }
        let frame = self
            .decoder
            .current_frame_info()
            .context("GIF frame is missing")?
            .clone();
        self.frames += 1;
        let source_left = u32::from(frame.left);
        let source_top = u32::from(frame.top);
        let width = u32::from(frame.width);
        let height = u32::from(frame.height);
        let (cw, ch) = self.canvas.dimensions();
        let scaled = |value: u32, dest: u32, source: u32| {
            (u64::from(value) * u64::from(dest)).div_ceil(u64::from(source)) as u32
        };
        let left = scaled(source_left, cw, self.logical.0);
        let right = scaled(source_left + width, cw, self.logical.0);
        let top = scaled(source_top, ch, self.logical.1);
        let bottom = scaled(source_top + height, ch, self.logical.1);
        self.restore =
            (frame.dispose == gif::DisposalMethod::Previous).then(|| self.canvas.clone());
        // GIF's row width is a format-bounded u16. Decode one row at a time,
        // including interlace passes, instead of allocating a complete frame.
        self.memory
            .resize(self.canvas.len() as u64 + self.restore.as_ref().map_or(0, |v| v.len() as u64));
        let mut row = vec![0; width as usize * 4];
        let _row_memory = crate::resources::pool().track(
            row.len() as u64,
            glim_core::resources::ResourceClass::Working,
        );
        let passes: &[(u32, u32)] = if frame.interlaced {
            &[(0, 8), (4, 8), (2, 4), (1, 2)]
        } else {
            &[(0, 1)]
        };
        for &(first, step) in passes {
            for y in (first..height).step_by(step as usize) {
                self.cancel.check()?;
                if row.is_empty() {
                    continue;
                }
                ensure!(self.decoder.fill_buffer(&mut row)?, "Truncated GIF frame");
                let source_y = source_top + y;
                let dy = scaled(source_y, ch, self.logical.1);
                if dy >= ch
                    || u64::from(dy) * u64::from(self.logical.1) / u64::from(ch)
                        != u64::from(source_y)
                {
                    continue;
                }
                for dx in left..right {
                    let sx = (u64::from(dx) * u64::from(self.logical.0) / u64::from(cw)) as u32
                        - source_left;
                    let pixel = &row[sx as usize * 4..sx as usize * 4 + 4];
                    if pixel[3] != 0 {
                        self.canvas
                            .put_pixel(dx, dy, image::Rgba(pixel.try_into()?));
                    }
                }
            }
        }
        self.disposal = frame.dispose;
        self.rectangle = (left, top, right - left, bottom - top);
        // Match browser handling of zero/one-centisecond delays to avoid busy loops.
        let delay_ms = if frame.delay <= 1 {
            100
        } else {
            u32::from(frame.delay) * 10
        };
        Ok(Some((
            image::Frame::new(
                if preview
                    && self.canvas.width().max(self.canvas.height())
                        > reader_limits::GIF_PREVIEW_EDGE
                {
                    let (width, height) = self.preview_dimensions();
                    image::imageops::resize(
                        &self.canvas,
                        width,
                        height,
                        image::imageops::FilterType::Nearest,
                    )
                } else {
                    self.canvas.clone()
                },
            ),
            std::time::Duration::from_millis(u64::from(delay_ms)),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gif_bytes(
        frames: &[(u16, Vec<u8>, gif::DisposalMethod)],
        repeat: gif::Repeat,
    ) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        {
            let mut encoder = gif::Encoder::new(
                &mut bytes,
                3,
                1,
                &[255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0],
            )?;
            encoder.set_repeat(repeat)?;
            for (left, pixels, disposal) in frames {
                let frame = gif::Frame {
                    left: *left,
                    width: pixels.len() as u16,
                    height: 1,
                    delay: 5,
                    dispose: *disposal,
                    transparent: Some(3),
                    buffer: std::borrow::Cow::Borrowed(pixels),
                    ..Default::default()
                };
                encoder.write_frame(&frame)?;
            }
        }
        Ok(bytes)
    }

    #[test]
    fn gif_display_preview_is_bounded_without_changing_source_frames() -> Result<()> {
        let mut bytes = Vec::new();
        {
            let mut encoder = gif::Encoder::new(&mut bytes, 4096, 1, &[255, 0, 0])?;
            encoder.set_repeat(gif::Repeat::Finite(0))?;
            encoder.write_frame(&gif::Frame {
                width: 4096,
                height: 1,
                delay: 5,
                buffer: std::borrow::Cow::Owned(vec![0; 4096]),
                ..Default::default()
            })?;
        }
        let mut source = GifPlayback::new(std::sync::Arc::from(bytes.clone()))?;
        let mut preview = GifPlayback::new(std::sync::Arc::from(bytes))?;
        assert_eq!(
            source.next_frame()?.unwrap().0.buffer().dimensions(),
            (4096, 1)
        );
        assert_eq!(preview.preview_frame_bytes(), 2048 * 4);
        let (frame, delay) = preview.next_preview_frame()?.unwrap();
        assert_eq!(frame.buffer().dimensions(), (2048, 1));
        assert!(
            frame
                .buffer()
                .pixels()
                .all(|pixel| pixel.0 == [255, 0, 0, 255])
        );
        assert_eq!(delay, std::time::Duration::from_millis(50));
        assert!(preview.next_preview_frame()?.is_none());
        Ok(())
    }

    #[test]
    fn gif_playback_composites_disposal_and_respects_finite_loops() -> Result<()> {
        use gif::DisposalMethod::{Background, Keep, Previous};
        let bytes = gif_bytes(
            &[
                (0, vec![0, 0, 0], Keep),
                (1, vec![1], Background),
                (0, vec![2], Previous),
                (2, vec![1], Keep),
            ],
            gif::Repeat::Finite(1),
        )?;
        let mut playback = GifPlayback::new(std::sync::Arc::from(bytes))?;
        let red = [255, 0, 0, 255];
        let green = [0, 255, 0, 255];
        let blue = [0, 0, 255, 255];
        let transparent = [0; 4];
        for _ in 0..2 {
            for expected in [
                [red, red, red],
                [red, green, red],
                [blue, transparent, red],
                [red, transparent, green],
            ] {
                let (frame, delay) = playback.next_frame()?.unwrap();
                assert_eq!(frame.buffer().dimensions(), (3, 1));
                assert_eq!(delay, std::time::Duration::from_millis(50));
                for (pixel, expected) in frame.buffer().pixels().zip(expected) {
                    assert_eq!(pixel.0, expected);
                }
            }
        }
        assert!(playback.next_frame()?.is_none());
        assert!(playback.next_frame()?.is_none());
        Ok(())
    }

    #[test]
    fn gif_playback_has_no_total_frame_limit() -> Result<()> {
        let frames = vec![(0, vec![0, 1, 2], gif::DisposalMethod::Keep); 2101];
        let bytes = gif_bytes(&frames, gif::Repeat::Infinite)?;
        let mut playback = GifPlayback::new(std::sync::Arc::from(bytes))?;
        for _ in 0..4202 {
            assert!(playback.next_frame()?.is_some());
            assert_eq!(playback.canvas.len(), 12);
            assert!(playback.restore.is_none());
        }
        Ok(())
    }

    #[test]
    fn float_images_produce_displayable_png_previews() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let pixels = image::Rgb32FImage::from_pixel(2, 2, image::Rgb([0.25, 0.5, 1.0]));
        for (extension, format) in [
            ("exr", image::ImageFormat::OpenExr),
            ("hdr", image::ImageFormat::Hdr),
        ] {
            let path = dir.path().join(format!("preview.{extension}"));
            image::DynamicImage::ImageRgb32F(pixels.clone()).save_with_format(&path, format)?;
            assert!(supports(&path));
            let preview = read_image(&path)?;
            let decoded = image::load_from_memory(&preview.png)?;
            assert_eq!((decoded.width(), decoded.height()), (2, 2));
            // ImageIO applies the native color profile/tone curve for float input.
            let [red, green, blue] = decoded.to_rgb8().get_pixel(0, 0).0;
            assert!(red > 0 && red < green && green < blue);
            std::fs::write(&path, b"invalid image")?;
            assert!(read_image(&path).is_err());
        }
        Ok(())
    }

    #[test]
    fn opens_project_assets_and_rejects_malformed_images() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for p in [
            "assets/branding/glim.png",
            "assets/branding/glim.svg",
            "assets/macos/Glim.icns",
        ] {
            assert!(read_image(&root.join(p))?.png.starts_with(b"\x89PNG"));
        }
        let temp = tempfile::tempdir()?;
        let malformed = temp.path().join("malformed.icns");
        std::fs::write(&malformed, b"icns\0\0\0\x10bad!\0\0\0\0")?;
        assert!(read_image(&malformed).is_err());
        let path = temp.path().join("large.svg");
        std::fs::write(
            &path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="50000"><rect width="100000" height="50000" fill="red"/></svg>"#,
        )?;
        let preview = image::load_from_memory(&read_image(&path)?.png)?;
        assert_eq!((preview.width(), preview.height()), (4000, 2000));
        assert_eq!(preview.to_rgb8().get_pixel(2000, 1000).0, [255, 0, 0]);
        Ok(())
    }
}
