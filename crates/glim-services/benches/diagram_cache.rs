//! Production cache operations on prepared PNG fixtures; excludes engine rendering.
use glim_services::{
    markdown_preview::{Diagram, DiagramImage, cache::ImageCache},
    resources::{CacheActivitySnapshot, ResourcePool, Retention},
};
use serde_json::json;
use std::{io::Cursor, time::Instant};

fn fixture(edge: u32) -> DiagramImage {
    let pixels = image::RgbaImage::from_fn(edge, edge, |x, y| {
        image::Rgba([(x % 251) as u8, (y % 251) as u8, ((x ^ y) % 251) as u8, 255])
    });
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    DiagramImage {
        png: encoded.into_inner(),
        width: edge,
        height: edge,
        baseline: 0.,
    }
}

fn same(actual: &DiagramImage, expected: &DiagramImage) {
    assert_eq!(actual.png, expected.png);
    assert_eq!(
        (actual.width, actual.height, actual.baseline),
        (expected.width, expected.height, expected.baseline)
    );
}

fn main() {
    let key = Diagram {
        kind: "dot".into(),
        source: "digraph { 中文 -> cache }".into(),
    };
    let mut cases = Vec::new();
    for edge in [64, 256, 1024] {
        let image = fixture(edge);
        let mut cold = Vec::new();
        let mut warm = Vec::new();
        for _ in 0..20 {
            let pool = ResourcePool::new(u64::MAX);
            let mut cache = ImageCache::new(pool.clone(), u64::MAX);
            let started = Instant::now();
            assert!(cache.lookup(&key, edge).is_none());
            cache.remember(&key, edge, &image);
            cold.push(started.elapsed().as_nanos() as u64);
            assert_eq!(
                cache.activity(),
                CacheActivitySnapshot {
                    misses: 1,
                    ..Default::default()
                }
            );
            for _ in 0..10 {
                let started = Instant::now();
                let hit = cache.lookup(&key, edge).unwrap();
                warm.push(started.elapsed().as_nanos() as u64);
                same(&hit, &image);
            }
            assert_eq!(
                cache.activity(),
                CacheActivitySnapshot {
                    hits: 10,
                    misses: 1,
                    pending: 0
                }
            );
            // Each part of the production key must independently prevent reuse.
            assert!(cache.lookup(&key, edge + 1).is_none());
            assert!(
                cache
                    .lookup(
                        &Diagram {
                            kind: "mermaid".into(),
                            ..key.clone()
                        },
                        edge
                    )
                    .is_none()
            );
            assert!(
                cache
                    .lookup(
                        &Diagram {
                            source: "digraph { changed }".into(),
                            ..key.clone()
                        },
                        edge
                    )
                    .is_none()
            );
            assert_eq!(
                cache.activity(),
                CacheActivitySnapshot {
                    hits: 10,
                    misses: 4,
                    pending: 0
                }
            );
            drop(cache);
            assert_eq!(pool.snapshot().cache_bytes, 0);
        }
        cold.sort_unstable();
        warm.sort_unstable();
        cases.push(json!({
            "edge": edge, "png_bytes": image.png.len(),
            "cold_lookup_and_publish_ns": cold,
            "warm_lookup_ns": warm,
            "verified_hits": 200, "verified_misses": 80,
            "comparison": "miss plus publication of prepared PNG versus resident lookup; rendering and encoding excluded",
        }));
    }

    let image = fixture(64);
    // Probe the retained charge so pressure cases do not depend on struct layout.
    let probe = ResourcePool::new(u64::MAX);
    let mut cache = ImageCache::new(probe.clone(), u64::MAX);
    cache.remember(&key, 64, &image);
    let charge = probe.snapshot().cache_bytes;
    drop(cache);
    assert_eq!(probe.snapshot().cache_bytes, 0);

    let pool = ResourcePool::new(charge);
    let protected_state = pool.track(128, glim_services::resources::ResourceClass::State);
    let mut cache = ImageCache::new(pool.clone(), u64::MAX);
    assert!(cache.lookup(&key, 64).is_none());
    cache.remember(&key, 64, &image);
    let displayed = cache.lookup(&key, 64).unwrap();
    let pressure = pool.cache(vec![0u8; charge as usize], charge, Retention::Viewport);
    let pressure_user = pressure.get(Retention::Viewport).unwrap();
    pool.trim();
    assert_eq!(pool.snapshot().cache_bytes, charge * 2);
    assert_eq!(pool.snapshot().evictions, 0);
    same(&displayed, &image);
    drop(displayed);
    pool.trim();
    assert_eq!(pool.snapshot().evictions, 1);
    assert!(cache.lookup(&key, 64).is_none());
    assert_eq!(pool.snapshot().state_bytes, 128);
    drop((pressure_user, pressure));
    cache.remember(&key, 64, &image);
    let reconstructed = cache.lookup(&key, 64).unwrap();
    same(&reconstructed, &image);
    assert_eq!(
        cache.activity(),
        CacheActivitySnapshot {
            hits: 2,
            misses: 2,
            pending: 0
        }
    );
    let pressure_activity = cache.activity();
    drop(cache);
    assert_eq!(pool.snapshot().cache_bytes, charge);
    drop(reconstructed);
    assert_eq!(pool.snapshot().cache_bytes, 0);
    drop(protected_state);
    assert_eq!(pool.snapshot().state_bytes, 0);

    // Local retention refuses oversized results without limiting access to source.
    let mut refused = ImageCache::new(pool.clone(), charge - 1);
    refused.remember(&key, 64, &image);
    assert_eq!(refused.activity(), CacheActivitySnapshot::default());
    assert!(refused.lookup(&key, 64).is_none());
    assert_eq!(pool.snapshot().cache_bytes, 0);

    println!("{}", serde_json::to_string_pretty(&json!({
        "schema": 1, "scope": "production prepared-image cache; no WebView, diagram engine or UI frame timing",
        "cases": cases,
        "pressure": { "hits": pressure_activity.hits, "misses": pressure_activity.misses, "evictions": pool.snapshot().evictions, "all_owned_buffers_released": true },
        "counters": "only consumer lookup calls; publication, pinning, residency checks and trimming do not count",
    })).unwrap());
}
