//! Bounded raster previews for inline document images. Call from a background executor.
use crate::limits as reader_limits;
use anyhow::{Context, Result, ensure};
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::Mutex,
};

const MAX_EDGE: u32 = reader_limits::INLINE_IMAGE_PREVIEW_EDGE;
// Avoid simultaneous full-size native decode work for a document containing many images.
static PREPARATION: Mutex<()> = Mutex::new(());

pub fn from_file_cancellable(
    path: &Path,
    cancel: &crate::work::Cancellation,
) -> Result<crate::media::RasterFrame> {
    cancel.check()?;
    let _guard = PREPARATION
        .lock()
        .map_err(|_| anyhow::anyhow!("Image preparation was interrupted"))?;
    cancel.check()?;
    native_thumbnail(path, cancel)
}

fn native_thumbnail(
    path: &Path,
    cancel: &crate::work::Cancellation,
) -> Result<crate::media::RasterFrame> {
    let png = thumbnail_png_cancellable(path, MAX_EDGE, cancel)?;
    crate::media::decode_png(&png, cancel)
}

pub(crate) fn thumbnail_png(path: &Path, max_edge: u32) -> Result<Vec<u8>> {
    thumbnail_png_cancellable(path, max_edge, &crate::work::Cancellation::default())
}
pub(crate) fn thumbnail_png_cancellable(
    path: &Path,
    max_edge: u32,
    cancel: &crate::work::Cancellation,
) -> Result<Vec<u8>> {
    cancel.check()?;
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("preview.png");
    let mut signature = [0; 4];
    use std::io::Read;
    let qoi = std::fs::File::open(path)?.read(&mut signature)? == 4 && &signature == b"qoif";
    let converted = if qoi {
        Some(crate::qoi::prepare(path, cancel)?)
    } else {
        None
    };
    let path = converted.as_ref().map_or(path, |file| file.path());
    let dimensions = image::ImageReader::open(path)
        .ok()
        .and_then(|reader| reader.with_guessed_format().ok())
        .and_then(|reader| reader.into_dimensions().ok());
    let svg_size = if dimensions.is_none() {
        crate::svg_preview::dimensions(path, cancel).ok().flatten()
    } else {
        None
    };
    if let Some(size) = svg_size {
        let svg = crate::svg_preview::prepare_thumbnail(path, size, max_edge, cancel)?;
        let side = size.0.max(size.1);
        let edge = side.ceil().clamp(1., f64::from(max_edge)) as u32;
        let mut command = Command::new("/usr/bin/qlmanage");
        command
            .args(["-t", "-s", &edge.to_string(), "-o"])
            .arg(directory.path())
            .arg(svg.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        ensure!(
            crate::work::run(&mut command, cancel)?.success(),
            "Cannot prepare SVG thumbnail"
        );
        let thumbnail = directory
            .path()
            .join(svg.path().file_name().context("Missing SVG filename")?)
            .with_extension("svg.png");
        let (width, height) = image::ImageReader::open(&thumbnail)?.into_dimensions()?;
        ensure!(
            width > 0 && height > 0 && width <= max_edge && height <= max_edge,
            "Invalid SVG thumbnail dimensions"
        );
        let width = (f64::from(width) * size.0 / side).ceil().max(1.) as u32;
        let height = (f64::from(height) * size.1 / side).ceil().max(1.) as u32;
        let mut reader = image::ImageReader::open(thumbnail)?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(max_edge);
        limits.max_image_height = Some(max_edge);
        limits.max_alloc = Some(reader_limits::INLINE_IMAGE_DECODED_BYTES);
        reader.limits(limits);
        cancel.check()?;
        let image = reader.decode()?;
        cancel.check()?;
        image
            .crop_imm(0, 0, width, height)
            .save_with_format(&output, image::ImageFormat::Png)?;
    } else {
        ensure!(
            sips_thumbnail(path, &output, max_edge, dimensions, cancel)?,
            "Cannot prepare inline image thumbnail"
        );
    }
    ensure!(
        std::fs::metadata(&output)?.len()
            <= u64::from(max_edge) * u64::from(max_edge) * 4
                + reader_limits::INLINE_IMAGE_OUTPUT_OVERHEAD_BYTES,
        "Image thumbnail exceeds output budget"
    );
    let (width, height) = image::ImageReader::open(&output)?.into_dimensions()?;
    ensure!(
        width > 0 && height > 0 && width <= max_edge && height <= max_edge,
        "Invalid thumbnail dimensions"
    );
    Ok(std::fs::read(output)?)
}

fn sips_thumbnail(
    path: &Path,
    output: &Path,
    max_edge: u32,
    dimensions: Option<(u32, u32)>,
    cancel: &crate::work::Cancellation,
) -> Result<bool> {
    // Specify both dimensions for thin images: sips' longest-edge option can
    // round the short edge to zero even though the source is valid.
    let mut command = Command::new("/usr/bin/sips");
    if let Some((width, height)) = dimensions {
        let longest = u64::from(width.max(height)).max(1);
        let edge = u64::from(max_edge).min(longest);
        let width = (u64::from(width) * edge / longest).max(1);
        let height = (u64::from(height) * edge / longest).max(1);
        command.args([
            "--resampleHeightWidth",
            &height.to_string(),
            &width.to_string(),
        ]);
    } else {
        command.args(["--resampleHeightWidthMax", &max_edge.to_string()]);
    }
    command
        .args(["--setProperty", "format", "png", "--out"])
        .arg(output)
        .arg(std::path::absolute(path)?)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Ok(crate::work::run(&mut command, cancel)
        .context("Cannot prepare inline image thumbnail")?
        .success())
}
