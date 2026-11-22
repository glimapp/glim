use crate::{svg_preview, work::Cancellation};
use anyhow::{Context, Result, ensure};
use std::path::Path;
use windows::{
    Graphics::Imaging::{
        BitmapAlphaMode, BitmapDecoder, BitmapFrame, BitmapInterpolationMode, BitmapPixelFormat,
        BitmapTransform, ColorManagementMode, ExifOrientationMode,
    },
    Storage::{FileAccessMode, StorageFile},
    core::HSTRING,
};

pub(super) fn write(
    path: &Path,
    output: &Path,
    max_edge: u32,
    cancel: &Cancellation,
) -> Result<()> {
    if let Some(size) = svg_preview::dimensions(path, cancel).ok().flatten() {
        return svg(path, output, size, max_edge, cancel);
    }
    let frame = decoder(path)?.GetFrameAsync(0)?.join()?;
    encode_frame(&frame, std::fs::File::create(output)?, max_edge, cancel)
}

fn decoder(path: &Path) -> Result<BitmapDecoder> {
    let absolute = crate::files::windows_shell_path(path)?;
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(absolute.as_os_str()))?.join()?;
    let stream = file.OpenAsync(FileAccessMode::Read)?.join()?;
    Ok(BitmapDecoder::CreateAsync(&stream)?.join()?)
}

/// Decode only the requested directory of the TIFF, at display resolution.
pub fn tiff_page(path: &Path, index: u32, cancel: &Cancellation) -> Result<(u32, Vec<u8>)> {
    cancel.check()?;
    let decoder = decoder(path)?;
    let count = decoder.FrameCount()?;
    ensure!(index < count, "TIFF page is no longer available");
    let frame = decoder.GetFrameAsync(index)?.join()?;
    let mut bytes = Vec::new();
    encode_frame(
        &frame,
        &mut bytes,
        crate::limits::IMAGE_PREVIEW_EDGE,
        cancel,
    )?;
    Ok((count, bytes))
}

fn encode_frame(
    frame: &BitmapFrame,
    output: impl std::io::Write,
    max_edge: u32,
    cancel: &Cancellation,
) -> Result<()> {
    use image::ImageEncoder;
    let (width, height) = (frame.PixelWidth()?, frame.PixelHeight()?);
    let longest = u64::from(width.max(height));
    ensure!(width > 0 && height > 0, "Invalid image dimensions");
    let edge = longest.min(max_edge.into());
    let scale = |dimension: u32| (u64::from(dimension) * edge / longest).max(1) as u32;
    let transform = BitmapTransform::new()?;
    transform.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
    transform.SetScaledWidth(scale(width))?;
    transform.SetScaledHeight(scale(height))?;
    cancel.check()?;
    let pixels = frame
        .GetPixelDataTransformedAsync(
            BitmapPixelFormat::Rgba8,
            BitmapAlphaMode::Straight,
            &transform,
            ExifOrientationMode::RespectExifOrientation,
            ColorManagementMode::ColorManageToSRgb,
        )?
        .join()?
        .DetachPixelData()?;
    cancel.check()?;
    image::codecs::png::PngEncoder::new(output).write_image(
        &pixels,
        scale(frame.OrientedPixelWidth()?),
        scale(frame.OrientedPixelHeight()?),
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(())
}

fn svg(
    path: &Path,
    output: &Path,
    size: (f64, f64),
    max_edge: u32,
    cancel: &Cancellation,
) -> Result<()> {
    use resvg::{tiny_skia, usvg};
    let prepared = svg_preview::prepare_thumbnail(path, size, max_edge, cancel)?;
    let options = usvg::Options {
        fontdb: crate::markdown_preview::diagram_fonts(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_data(&std::fs::read(prepared.path())?, &options)?;
    let longest = size.0.max(size.1);
    let edge = longest.ceil().clamp(1., max_edge.into());
    let dimension = |value: f64| (value * edge / longest).ceil().max(1.) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(dimension(size.0), dimension(size.1))
        .context("Invalid SVG viewport")?;
    let ratio = edge as f32 / max_edge as f32;
    cancel.check()?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(ratio, ratio),
        &mut pixmap.as_mut(),
    );
    cancel.check()?;
    pixmap.save_png(output)?;
    Ok(())
}
