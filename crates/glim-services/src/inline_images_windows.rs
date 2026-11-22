use crate::{svg_preview, work::Cancellation};
use anyhow::{Context, Result, ensure};
use std::path::Path;
use windows::{
    Graphics::Imaging::{
        BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat,
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
    let absolute = std::path::absolute(path)?;
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(absolute.as_os_str()))?.join()?;
    let stream = file.OpenAsync(FileAccessMode::Read)?.join()?;
    let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
    let (width, height) = (decoder.PixelWidth()?, decoder.PixelHeight()?);
    let longest = u64::from(width.max(height));
    ensure!(width > 0 && height > 0, "Invalid image dimensions");
    let edge = longest.min(max_edge.into());
    let scale = |dimension: u32| (u64::from(dimension) * edge / longest).max(1) as u32;
    let transform = BitmapTransform::new()?;
    transform.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
    transform.SetScaledWidth(scale(width))?;
    transform.SetScaledHeight(scale(height))?;
    cancel.check()?;
    let pixels = decoder
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
    image::save_buffer_with_format(
        output,
        &pixels,
        scale(decoder.OrientedPixelWidth()?),
        scale(decoder.OrientedPixelHeight()?),
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
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
