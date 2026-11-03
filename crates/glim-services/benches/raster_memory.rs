//! Measures GIF decode/preview work and admitted buffer residency. Decoder
//! metadata, the encoded fixture, allocator overhead and GPU storage are outside
//! this counter; it must not be presented as process RSS or total peak memory.
use glim_services::{
    media::{GifInput, GifPlayback},
    resources::ResourcePool,
    work::Cancellation,
};
use serde_json::json;
use std::{collections::VecDeque, sync::Arc, time::Instant};

fn fixture(width: u16, height: u16) -> anyhow::Result<Arc<[u8]>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = gif::Encoder::new(
            &mut bytes,
            width,
            height,
            &[255, 0, 0, 0, 255, 0, 0, 0, 255],
        )?;
        encoder.set_repeat(gif::Repeat::Finite(0))?;
        for index in 0..9 {
            encoder.write_frame(&gif::Frame {
                width,
                height,
                delay: 5,
                dispose: if index % 2 == 0 {
                    gif::DisposalMethod::Previous
                } else {
                    gif::DisposalMethod::Keep
                },
                buffer: std::borrow::Cow::Owned(vec![
                    index % 3;
                    usize::from(width) * usize::from(height)
                ]),
                ..Default::default()
            })?;
        }
    }
    Ok(bytes.into())
}

fn main() -> anyhow::Result<()> {
    let mut reports = Vec::new();
    for (width, height) in [(4096, 1024), (65535, 1)] {
        let input = fixture(width, height)?;
        for budget in [1024 * 1024, 8 * 1024 * 1024, 512 * 1024 * 1024] {
            let pool = ResourcePool::with_budgets(0, budget);
            let start = Instant::now();
            let mut playback = GifPlayback::from_input_with_pool(
                GifInput::Bytes(input.clone()),
                Cancellation::default(),
                &pool,
            )?;
            let setup_us = start.elapsed().as_micros();
            let mut resident = VecDeque::new();
            let mut samples = Vec::new();
            let mut max_retained = pool.snapshot().working_bytes;
            let in_flight = 2 * glim_services::limits::GIF_BATCH_FRAMES + 1;
            for index in 0..9 {
                if resident.len() == in_flight {
                    resident.pop_front();
                }
                let start = Instant::now();
                let (frame, _) = playback.next_preview_frame()?.expect("fixture frame");
                let elapsed_us = start.elapsed().as_micros();
                let expected = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]][index % 3];
                anyhow::ensure!(
                    frame.buffer().pixels().all(|p| p.0 == expected),
                    "Preview/disposal pixels differ from the fixture"
                );
                let (w, h) = frame.buffer().dimensions();
                resident.push_back(frame);
                let working = pool.snapshot().working_bytes;
                anyhow::ensure!(working <= budget, "Admission exceeded its grant");
                max_retained = max_retained.max(working);
                samples.push(json!({"decode_us": elapsed_us, "width": w, "height": h, "retained_bytes": working}));
            }
            anyhow::ensure!(
                playback.next_preview_frame()?.is_none(),
                "Animation did not finish while its last frames remained visible"
            );
            drop(playback);
            let surviving_frames = pool.snapshot().working_bytes;
            let expected_bytes: u64 = resident.iter().map(|f| f.buffer().len() as u64).sum();
            anyhow::ensure!(
                surviving_frames == expected_bytes,
                "Frame charges did not follow the remaining pixels"
            );
            drop(resident);
            anyhow::ensure!(
                pool.snapshot().working_bytes == 0,
                "Leaked buffer reservation"
            );
            reports.push(json!({
                "source_width": width, "source_height": height, "budget_bytes": budget,
                "encoded_bytes": input.len(), "setup_us": setup_us,
                "max_retained_bytes_after_calls": max_retained, "surviving_frame_bytes_after_decoder_drop": surviving_frames,
                "samples": samples,
            }));
        }
    }
    let mut png_reports = Vec::new();
    for edge in [32, 2048] {
        let source =
            image::RgbaImage::from_fn(edge, edge, |x, y| image::Rgba([x as u8, y as u8, 127, 255]));
        let mut encoded = std::io::Cursor::new(Vec::new());
        source.write_to(&mut encoded, image::ImageFormat::Png)?;
        let pool = glim_services::resources::pool();
        let start = Instant::now();
        let frame = glim_services::media::decode_png(encoded.get_ref(), &Cancellation::default())?;
        let decode_us = start.elapsed().as_micros();
        anyhow::ensure!(
            frame.buffer() == &source,
            "PNG pixels changed during admitted decode"
        );
        let retained = pool.snapshot().working_bytes;
        let (frame, memory) = frame.into_bgra();
        anyhow::ensure!(
            pool.snapshot().working_bytes == retained,
            "PNG handoff released admission too early"
        );
        drop(frame);
        drop(memory);
        anyhow::ensure!(
            pool.snapshot().working_bytes == 0,
            "PNG admission did not release"
        );
        png_reports.push(json!({"edge": edge, "encoded_bytes": encoded.get_ref().len(), "decode_us": decode_us, "retained_bytes": retained}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema": 1, "measurement": "Raster decode time and admitted application pixel/row buffers, not process RSS or transient peak", "gif_reports": reports, "png_reports": png_reports,
        }))?
    );
    Ok(())
}
