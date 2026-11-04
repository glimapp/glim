//! Compare the previous full-copy/full-geometry path with production row updates.
// Cargo checks this no-harness target with cfg(test), without executing its embedded tests.
#[allow(dead_code, unused_imports)]
#[path = "../src/views/minimap/geometry.rs"]
mod geometry;
use geometry::Geometry;
use glim_core::resources::ResourcePool;
use gpui_kit::component::input::{Rope, RopeExt};
use serde_json::json;
use std::{sync::Arc, time::Instant};

fn previous(text: &str) -> Vec<(u8, u8)> {
    text.split('\n')
        .map(|line| {
            let indent = line
                .chars()
                .take_while(|ch| ch.is_whitespace())
                .count()
                .min(100);
            let width = line.chars().take(120).count().saturating_sub(indent);
            (indent as u8, width as u8)
        })
        .collect()
}
fn main() {
    let mut reports = Vec::new();
    for count in [1000, 10_000, 50_000] {
        let source = (0..count).map(|row| format!("  Line {row:06}: 中文内容, plain text, punctuation and reproducible minimap geometry.\n")).collect::<String>();
        let original = Rope::from_str(&source);
        let pool = ResourcePool::with_budgets(0, 16 * 1024 * 1024);
        let started = Instant::now();
        let mut baseline = Arc::new(previous(&source));
        let baseline_initial_ms = started.elapsed().as_secs_f64() * 1000.;
        let started = Instant::now();
        let mut candidate = Geometry::new(&original, pool.clone());
        let candidate_initial_ms = started.elapsed().as_secs_f64() * 1000.;
        let mut samples = Vec::new();
        for repeat in 0..12 {
            for (location, row) in [("start", 0), ("middle", count / 2), ("end", count - 1)] {
                for (kind, inserted) in [("text", "新😀"), ("newline", "\n  inserted\n")] {
                    let mut edited = original.clone();
                    edited.insert(edited.line_start_offset(row) + 2, inserted);
                    for (operation, revision) in [("insert", &edited), ("remove", &original)] {
                        let old_baseline = baseline.clone();
                        let painted = candidate.snapshot();
                        let mut baseline_ms = 0.;
                        let mut candidate_ms = 0.;
                        let mut recomputed = 0;
                        for variant in if repeat % 2 == 0 { [0, 1] } else { [1, 0] } {
                            let started = Instant::now();
                            if variant == 0 {
                                let text = revision.to_string();
                                baseline = Arc::new(previous(&text));
                                drop(text);
                                baseline_ms = started.elapsed().as_secs_f64() * 1000.;
                            } else {
                                recomputed = candidate
                                    .update(revision)
                                    .expect("bounded fixture admission");
                                candidate_ms = started.elapsed().as_secs_f64() * 1000.;
                            }
                        }
                        let snapshot = candidate.snapshot().unwrap();
                        assert_eq!(candidate.len(), baseline.len());
                        for (row, expected) in baseline.iter().enumerate() {
                            assert_eq!(snapshot.get(row), Some(*expected));
                        }
                        let peak_retained_bytes = pool.snapshot().working_bytes;
                        drop((old_baseline, painted, snapshot));
                        samples.push(json!({"repeat":repeat,"location":location,"kind":kind,"operation":operation,
                            "baseline_ms":baseline_ms,"candidate_ms":candidate_ms,"recomputed_rows":recomputed,
                            "working_bytes_with_previous_frame":peak_retained_bytes}));
                    }
                }
            }
        }
        drop(candidate);
        assert_eq!(pool.snapshot().working_bytes, 0);
        reports.push(
            json!({"lines":count,"bytes":source.len(),"baseline_initial_ms":baseline_initial_ms,
            "candidate_initial_ms":candidate_initial_ms,"samples":samples}),
        );
    }
    println!("{}",serde_json::to_string_pretty(&json!({"schema":1,
        "measurement":"minimap update wall time including allocation, previous frame pinned; no UI or I/O",
        "reports":reports})).unwrap());
}
