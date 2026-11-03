//! Pixel buffers retain their shared admission through preparation and display.
use crate::{
    resources::{ResourcePool, WorkingReservation},
    work::Cancellation,
};
use anyhow::{Result, ensure};
use std::{io::Cursor, ops::Deref};

pub struct RasterFrame {
    frame: image::Frame,
    memory: WorkingReservation,
}

impl RasterFrame {
    pub(super) fn new(frame: image::Frame, mut memory: WorkingReservation) -> Self {
        memory.shrink_to(frame.buffer().as_raw().capacity() as u64);
        Self { frame, memory }
    }

    /// GPUI consumes BGRA pixels without another image-sized allocation.
    pub fn into_bgra(mut self) -> (image::Frame, WorkingReservation) {
        for pixel in self.frame.buffer_mut().as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        (self.frame, self.memory)
    }
}

impl Deref for RasterFrame {
    type Target = image::Frame;
    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

pub fn decode_png(bytes: &[u8], cancel: &Cancellation) -> Result<RasterFrame> {
    decode_png_with_pool(bytes, cancel, crate::resources::pool())
}

fn decode_png_with_pool(
    bytes: &[u8],
    cancel: &Cancellation,
    pool: &ResourcePool,
) -> Result<RasterFrame> {
    cancel.check()?;
    // Read only the fixed IHDR prefix before admitting any decoder allocation.
    // The PNG decoder validates CRCs, color types and the rest of the stream.
    ensure!(
        bytes.len() >= 24
            && &bytes[..8] == b"\x89PNG\r\n\x1a\n"
            && bytes[8..12] == 13_u32.to_be_bytes()
            && &bytes[12..16] == b"IHDR",
        "Invalid preview PNG header"
    );
    let width = u32::from_be_bytes(bytes[16..20].try_into()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into()?);
    ensure!(width > 0 && height > 0, "Empty preview PNG");
    // A 16-bit RGBA source and the converted 8-bit result can coexist. PNG
    // parser/metadata workspace has its own bounded allowance within the grant.
    let pixels = u64::from(width) * u64::from(height);
    let required = pixels
        .checked_mul(12)
        .and_then(|n| n.checked_add(crate::limits::INLINE_IMAGE_DECODED_BYTES))
        .ok_or_else(|| anyhow::anyhow!("Preview dimensions overflow"))?;
    let memory = pool.reserve_working(required, required)?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(crate::limits::INLINE_IMAGE_DECODED_BYTES);
    let decoder = image::codecs::png::PngDecoder::with_limits(Cursor::new(bytes), limits)?;
    let image = image::DynamicImage::from_decoder(decoder)?.into_rgba8();
    cancel.check()?;
    Ok(RasterFrame::new(image::Frame::new(image), memory))
}

/// Preserve aspect ratio while fitting even a one-pixel-high panorama into the
/// admitted pixel count. Rounding each dimension up to one must not exceed it.
pub(super) fn fitted_dimensions(width: u32, height: u32, pixels: u64) -> (u32, u32) {
    assert!(width > 0 && height > 0 && pixels > 0);
    let scale = (pixels as f64 / (f64::from(width) * f64::from(height)))
        .sqrt()
        .min(1.);
    let mut w = (f64::from(width) * scale).floor().max(1.) as u32;
    let mut h = (f64::from(height) * scale).floor().max(1.) as u32;
    if u64::from(w) * u64::from(h) > pixels {
        if w >= h {
            w = (pixels / u64::from(h)).max(1) as u32;
        } else {
            h = (pixels / u64::from(w)).max(1) as u32;
        }
    }
    (w, h)
}

pub(super) fn sample_nearest(
    image: &image::RgbaImage,
    width: u32,
    height: u32,
    cancel: &Cancellation,
) -> Result<image::RgbaImage> {
    cancel.check()?;
    if image.dimensions() == (width, height) {
        return Ok(image.clone());
    }
    let mut output = image::RgbaImage::new(width, height);
    for y in 0..height {
        cancel.check()?;
        let sy =
            ((2 * u64::from(y) + 1) * u64::from(image.height()) / (2 * u64::from(height))) as u32;
        for x in 0..width {
            let sx =
                ((2 * u64::from(x) + 1) * u64::from(image.width()) / (2 * u64::from(width))) as u32;
            output.put_pixel(x, y, *image.get_pixel(sx, sy));
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_images_fit_the_grant_without_zero_dimensions() {
        for size in [(65535, 1), (1, 65535), (65535, 65535), (3, 2)] {
            for pixels in [1, 2, 3, 17, 100, 10_000] {
                let (w, h) = fitted_dimensions(size.0, size.1, pixels);
                assert!(w > 0 && h > 0 && w <= size.0 && h <= size.1);
                assert!(u64::from(w) * u64::from(h) <= pixels);
            }
        }
    }

    #[test]
    fn direct_sampling_matches_nearest_without_float_image_storage() {
        for (w, h, target_w, target_h) in [(9, 7, 4, 3), (20, 1, 3, 1), (1, 20, 1, 3), (5, 3, 5, 3)]
        {
            let input =
                image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([x as u8, y as u8, 42, 127]));
            assert_eq!(
                sample_nearest(&input, target_w, target_h, &Cancellation::default()).unwrap(),
                image::imageops::resize(
                    &input,
                    target_w,
                    target_h,
                    image::imageops::FilterType::Nearest
                )
            );
        }
    }

    #[test]
    fn png_admission_precedes_decode_and_lives_until_pixel_handoff_drops() {
        let mut bytes = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let pool = ResourcePool::with_budgets(0, 16);
        let error = decode_png_with_pool(bytes.get_ref(), &Cancellation::default(), &pool)
            .err()
            .unwrap();
        assert!(error.is::<crate::resources::WorkingPressure>());
        assert_eq!(pool.snapshot().working_bytes, 0);
        let pool =
            ResourcePool::with_budgets(0, crate::limits::INLINE_IMAGE_DECODED_BYTES + 6 * 12);
        let frame = decode_png_with_pool(bytes.get_ref(), &Cancellation::default(), &pool).unwrap();
        assert_eq!(pool.snapshot().working_bytes, 24);
        let (frame, memory) = frame.into_bgra();
        assert_eq!(frame.buffer().get_pixel(0, 0).0, [3, 2, 1, 255]);
        assert_eq!(pool.snapshot().working_bytes, 24);
        drop(frame);
        drop(memory);
        assert_eq!(pool.snapshot().working_bytes, 0);
    }
}
