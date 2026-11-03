//! Parser/projection comparison; excludes UI layout and parser-internal heap measurements.
#[path = "support/structured_baseline.rs"]
mod baseline;
use anyhow::{Result, ensure};
use glim_core::resources::{ResourcePool, TextSnapshot, WorkingPressure};
use glim_services::{
    json::parse_format_cancellable,
    work::{Cancellation, Cancelled},
};
use serde_json::json;
use std::{sync::Arc, time::Instant};

fn main() -> Result<()> {
    let mut fixtures = Vec::new();
    for count in [100, 1000, 10_000] {
        let rows: Vec<_> = (0..count)
            .map(|i| {
                json!({
                    "index": i, "title": format!("文档 {i}"), "lines": ["first\n", "second"],
                    "metadata": {"enabled": true, "value": null}
                })
            })
            .collect();
        fixtures.push((
            format!("json-{count}"),
            "json",
            serde_json::to_string(&rows)?,
        ));
    }
    fixtures.extend([
        (
            "jsonc".into(),
            "jsonc",
            "{/*comment*/\"url\":\"https://a/b//c\",\"items\":[1,2,],}".into(),
        ),
        (
            "json5".into(),
            "json5",
            "{name:'中文',hex:0x10,number:.5,items:[true,null,],duplicate:1,duplicate:2}".into(),
        ),
        (
            "jsonl".into(),
            "jsonl",
            "{\"row\":1}\n\n{\"row\":2,\"lines\":[\"a\",\"b\"]}\n".into(),
        ),
        (
            "duplicate-keys".into(),
            "json",
            "{\"z\":1,\"a\":{\"x\":1},\"a\":[2,3]}".into(),
        ),
        (
            "long-values".into(),
            "json",
            serde_json::to_string(
                &json!({"长键".repeat(700): "\n长值".repeat(4000), "cell_type":"markdown"}),
            )?,
        ),
    ]);
    let mut reports = Vec::new();
    for (name, format, source) in fixtures {
        let pool = ResourcePool::with_budgets(0, glim_services::limits::RESOURCES_WORKING_BYTES);
        let start = Instant::now();
        let input = Arc::new(TextSnapshot::from_chunks(
            std::iter::once(source.as_str()),
            &pool,
        )?);
        let snapshot_us = start.elapsed().as_micros();
        let input_bytes = pool.snapshot().working_bytes;
        let mut samples = Vec::new();
        let mut node_count = 0;
        for round in 0..5 {
            let before = || -> Result<_> {
                let start = Instant::now();
                let nodes = baseline::parse_format(input.as_str(), format)?;
                Ok((nodes, start.elapsed().as_micros()))
            };
            let after = || -> Result<_> {
                let start = Instant::now();
                let nodes = parse_format_cancellable(
                    input.as_str(),
                    format,
                    &Cancellation::default(),
                    &pool,
                )?;
                Ok((nodes, start.elapsed().as_micros()))
            };
            let (old, new) = if round % 2 == 0 {
                (before()?, after()?)
            } else {
                let new = after()?;
                (before()?, new)
            };
            ensure!(
                &*new.0 == old.0.as_slice(),
                "Node semantics changed: {name}"
            );
            node_count = new.0.len();
            let retained = pool.snapshot().working_bytes;
            ensure!(
                retained <= glim_services::limits::RESOURCES_WORKING_BYTES,
                "Owned buffers exceeded budget"
            );
            samples.push(json!({"baseline_us":old.1, "admitted_us":new.1, "retained_input_and_node_bytes":retained}));
            drop(new);
            ensure!(
                pool.snapshot().working_bytes == input_bytes,
                "Nodes retained admission after dropping"
            );
        }
        let available = glim_services::limits::RESOURCES_WORKING_BYTES - input_bytes;
        let blocker = pool.reserve_working(available, available)?;
        let denied =
            parse_format_cancellable(input.as_str(), format, &Cancellation::default(), &pool)
                .err()
                .expect("pressure must deny projection");
        ensure!(
            denied.is::<WorkingPressure>(),
            "Unexpected denial: {denied}"
        );
        drop(blocker);
        ensure!(
            pool.snapshot().working_bytes == input_bytes,
            "Denied projection leaked its reservation"
        );
        let retry =
            parse_format_cancellable(input.as_str(), format, &Cancellation::default(), &pool)?;
        ensure!(retry.len() == node_count, "Retry lost nodes");
        drop(retry);
        let (cancel, guard) = Cancellation::task();
        drop(guard);
        let error = parse_format_cancellable(input.as_str(), format, &cancel, &pool)
            .err()
            .expect("cancelled projection");
        ensure!(error.is::<Cancelled>(), "Cancellation did not propagate");
        let worker = input.clone();
        drop(input);
        ensure!(
            pool.snapshot().working_bytes == input_bytes,
            "Worker input was uncharged"
        );
        let start = Instant::now();
        ensure!(
            worker.matches_chunks(std::iter::once(source.as_str())),
            "Unchanged source did not match"
        );
        let snapshot_match_us = start.elapsed().as_micros();
        drop(worker);
        ensure!(
            pool.snapshot().working_bytes == 0,
            "Input reservation leaked"
        );
        reports.push(json!({"fixture":name, "format":format, "source_bytes":source.len(), "nodes":node_count,
            "snapshot_copy_us":snapshot_us, "snapshot_match_us":snapshot_match_us, "samples":samples}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"schema":1,
        "scope":"Input snapshot and structured node buffers; parser Value/JSON5 internals, UI and process peak memory are excluded", "reports":reports}))?
    );
    Ok(())
}
