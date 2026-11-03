# Reader resources and rendering

**English** · [简体中文](reader-resources.zh-CN.md) · [Architecture](architecture.md)

Glim combines format-specific data representations with shared admission and resource ownership. Text uses pages and syntax ranges, databases use query windows, workbooks use disk indexes, books use chapters, images use bounded previews, and native documents retain system controllers. Scenarios compose these readers; they do not require a universal AST or conversion to Markdown.

## Resource ownership

`glim-core::resources` implements a UI/I/O-free resource pool. `glim-services::resources::pool()` owns the process-wide instance. Resource leases share a charge across clones and release it when the final owner drops. Resizing updates the existing charge. Counters are constant-time snapshots, not heap measurements.

| Class | Integrated owners | Release/reconstruction |
| --- | --- | --- |
| State | Live edit-session text and saved-baseline estimates | Session lifetime; never evicted as cache |
| Working | Inline image/GIF displayed and lookahead pixel buffers; GIF canvas, restore and row buffers | Canceled work drops its buffers; displayed images defer atlas removal until the window is available |
| Cache | Service diagram PNG reuse and view diagram images/errors | Shared budget; background entries first, then least recently accessed entries within each retention class |

`CachedResource` keeps a key that can become empty. A reader treats an empty entry as a miss and reconstructs it from its original content. `ResourceValue` pins a cache value while a caller owns it; diagram elements retain that pin during rendering. Reclamation runs outside the pool lock and skips busy or pinned entries. The current shared cache target is defined only in `assets/reader-limits.json`; existing local encoded/decoded/key budgets still apply. Allocation triggers reclamation, and explicitly calling `trim` can reclaim newly unpinned entries. Pinned values and newly published results may temporarily exceed the target; a new result stays resident through its initial reclamation pass so cache pressure cannot discard it before publication.

This is accounting for integrated owners, not a hard process-memory ceiling. Text capacity/undo history, all retained projections, GPUI asset/atlas caches, ImageIO, WebKit, Quick Look, parser internals and helper-process allocations are not fully represented. Diagram decoded bytes are an estimate. Do not add these counters to OS footprint or Metal counters: they overlap and have different scopes. `resource_sample` includes state, working, cache, cache target and eviction counters under existing telemetry consent.

## Visible work and cancellation

The shared scheduler has document, projection, decode, native and diagram lanes. FIFO queues use a weighted viewport, interactive, viewport, background cycle, skipping empty queues. Admission waits asynchronously before occupying a worker. A permit stays with the actual work until it exits, including after the UI cancels its task. Running SDK/parser calls are not preempted. Do not hold a permit while synchronously waiting for another permit in a dependency cycle.

Markdown's virtualized image subtree records image requests during layout/paint. After each frame, images absent from that subtree deactivate: still-image requests and GIF downloads/playback cancel, decoded frames release, and returning images reconstruct. GIF constructors do not fetch offscreen URLs; downloaded temporary files can be reused while their reader exists. A GIF restarts when reactivated, including finite animations. Toolkit overscan/layout can request nearby images; this is render demand, not exact pixel visibility. Nested alert images and embedded browser/system viewers retain their own loading paths.

Diagram/formula nodes enqueue bounded viewport batches. Updating render resources invalidates layout without reparsing the full document. Service PNG reuse keys include diagram kind, complete source and raster edge; that cache lasts only for the process. Cache eviction does not discard original source. Offline engines remain bundled and load required modules on demand; failures are isolated per result.

## Large-file paths

UTF-8 editing uses pages, bounded streaming copies and atomic saving. XLSX/XLSM builds a disk SQLite cell index; presentation pages preserve cached values, styles and merges without recalculating formulas or executing macros. Books use chapters and on-demand members, with full text/byte member fallbacks. CSV/SQLite cells and geographic coordinates use windows. Archives stream payloads; compressed streams reconstruct their decoder when needed. GIF uses bounded compositing and preview batches; raster images use scaled previews rather than original-resolution tiles. Remote images stream to temporary files. HTML streams through a local-resource worker while WebKit owns DOM/script allocation. Oversized structural projections retain complete source paging.

Source size is not an admission limit. The [generated policy inventory](reader-limits.md) describes working buffers, paging and fallback effects. Library/platform constraints remain: ZIP central-directory allocation, SQLite expression evaluation, native conversion and some indexes can require preparation before the first readable view. Changing numeric policy requires `python3 scripts/generate-reader-limits.py --docs`.

## Remaining engineering

- Extend ownership accounting to retained tables, book chapters, static image pages and native prepared resources; each integration needs a real release/reconstruction contract. Byte accounting alone does not bound aggregate working memory across all active readers.
- Add byte admission or adaptive decode quality if measured workloads exceed the working-memory target; preserve forward progress for a single large operation and protect unsaved state.
- Incremental Markdown parsing/index reuse remains incomplete. A large individual block can still cost substantial layout time.
- Persistent caches need version invalidation, disk reclamation and privacy cleanup before adoption.
- Measure first readable content, complete first viewport, scroll/input stalls and cache hits separately from preparation queue/worker duration. Runtime regressions and macOS/Intel interactions require actual execution; compilation cannot establish them.
