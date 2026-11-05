# Backlog and acceptance gaps

**English** · [简体中文](TODO.zh-CN.md) · [Documentation](README.md)

This page distinguishes implemented foundations, remaining engineering and runtime acceptance using implementation, test and measurement entry points. An unchecked item has unmet completion criteria; its implemented parts do not need rebuilding. A test or measurement script does not establish acceptance on real hardware. Candidate capabilities do not promise a release date.

## Implemented foundations

| Capability | Current implementation and entry points |
| --- | --- |
| Format registration and crate direction | A format catalog generates consumers, with ordered providers and centralized reader construction; retain `app → services → core`. [Catalog](../assets/format-catalog.json), [registry](../crates/glim-services/src/documents/registry.rs), [construction](../crates/glim-app/src/views/reader/construction.rs), [architecture check](../scripts/check-architecture.py). |
| Selected reader presentation boundaries | OPML, interactive graphs, JSON, CSV/geographic tables, SQLite and books have pure data/host interfaces. Table, SQLite and book views emit semantic requests; adapters own preparation and persistence. [Module contracts](reader-modules.md), [table](../crates/glim-app/src/views/reader/csv/view.rs), [SQLite](../crates/glim-app/src/views/reader/sqlite/view.rs), [books](../crates/glim-app/src/views/reader/books/view.rs). |
| Editing and session ownership | Editing sessions and saved baselines are separate from presentation; source/preview share editing state. Workspaces persist active selection, books persist chapter/position, and drafts have background checkpoints. [Editing](../crates/glim-app/src/app/edit_session.rs), [document sessions](../crates/glim-app/src/app/document_session.rs), [drafts](../crates/glim-services/src/drafts.rs). |
| Shared resources and work admission | Text/hex pages, table cells, structured trees, final structured-graph output/traversal storage, minimap geometry and major PNG/GIF pixel buffers acquire admission before allocation. Several readers release hidden resources. [Pool](../crates/glim-core/src/resources.rs), [graph projection](../crates/glim-services/src/graph/projection.rs), [raster decoding](../crates/glim-services/src/media/raster.rs), [coverage boundaries](reader-resources.md). |
| Selected incremental processing | The source minimap shares Rope text and unchanged geometry blocks, recomputing changed rows. Markdown locally reparses safe prose edits and retains a full-parser fallback. [Minimap](../crates/glim-app/src/views/minimap/geometry.rs), [Markdown](../vendor/gpui-base/src/text/format/markdown/edit.rs). |
| Diagram caching | Presentation and services provide in-process reuse; successful results have a disk backing cache with source/engine/environment invalidation, integrity checks, capacity eviction, atomic writes and cancellation. Filenames omit source text, but PNGs contain rendered document content; resource documentation describes cleanup. [Presentation](../crates/glim-app/src/views/reader/diagrams.rs), [disk cache](../crates/glim-services/src/markdown_preview/disk_cache.rs), [cleanup and limits](reader-resources.md#diagram-caches-and-hit-rates). |

## Architecture and performance backlog

Constraints: prefer ordinary reader modules. Separate crates require actual independent versioning, dependency isolation or reuse needs; module and extension counts do not justify extraction. New formats and scenarios compose existing capabilities, retaining shared actions, editing sessions, resource ownership and cancellation.

### Reader modularity

- [ ] **M1: Remaining reader boundaries.** Media, archives, paging, source/markup and native adapters still directly consume service payloads, application state or scheduler entry points; some combine presentation and preparation in one module. Separate actual coupling incrementally, retaining background orchestration in adapters and injecting required data/host operations into presentation. Avoid mechanical interface or crate proliferation.
  - Completion: the corresponding presentation performs no file I/O, persistence or background orchestration; representative prepared data and an injected host can construct it independently, and architecture checks cover each added boundary.
  - Entry points: [media](../crates/glim-app/src/views/reader/media.rs), [archives](../crates/glim-app/src/views/reader/archive.rs), [paging](../crates/glim-app/src/views/reader/paged.rs), [source](../crates/glim-app/src/views/reader/source.rs), [markup](../crates/glim-app/src/views/reader/markup.rs), [native](../crates/glim-app/src/views/reader/native.rs).
- [ ] **M2: Lifecycle contracts across readers.** Tables have handoff/suspension tests. SQLite covers cancellation, filtering/sorting, hidden refresh, stale results and retry. Books cover interrupted preference restoration, selection, chapter replacement, pending-state draining, writes after disposal and window rebinding. Other readers and complete workspace transfers still lack consistent representative coverage.
  - Completion: cover activation/hiding, refresh, cancellation, retry, closing and window transfer; preserve editing sessions, undo, unsaved contents and necessary navigation state, and reject stale results. Distinguish automated contract tests from physical-input acceptance.
  - Entry points: [lifecycle contract](../crates/glim-app/src/views/reader/contract.rs), [table tests](../crates/glim-app/src/views/reader/csv/view.rs), [SQLite tests](../crates/glim-app/src/views/reader/sqlite.rs), [book tests](../crates/glim-app/src/views/reader/books.rs).

### Resource coverage and pressure recovery

- [ ] **R1: Remaining controlled allocations.** JSON/JSON5 parser value trees, intermediate inputs for structured-data graph conversion, graph layout, book HTML/metadata, encoded preview/SVG intermediates and SQLite/geographic parsing scratch are not fully admitted. Admission for final tree/graph results does not establish coverage of preparation peaks.
  - Completion: controlled working buffers acquire admission before allocation/growth, including overlap between old and new buffers; owners retain reservations across tasks and publication. Replace working-buffer accounting that bypasses admission while preserving necessary state accounting and complete-source fallback. Do not reject supported files based on file size.
  - Entry points: [JSON](../crates/glim-services/src/json.rs), [graph preparation](../crates/glim-services/src/graph.rs), [books](../crates/glim-services/src/books/mod.rs), [SVG](../crates/glim-services/src/svg_preview.rs), [SQLite](../crates/glim-services/src/sqlite/browser.rs), [geographic tables](../crates/glim-services/src/geo.rs).
- [ ] **R2: Native and helper resources.** Native images, OCR and copying share ownership and accounting leases, but the Rust bridge still uses `track` without byte-budget admission. Preparation/conversion intermediates, separate helpers, ImageIO, WebKit, Quick Look, GPUI atlases and SQLite engine allocations have limited coverage.
  - Completion: admit controlled native working sets and support cancellable handoff; retain charges until running consumers finish. Add system-memory/helper observations and explicit framework boundaries; do not call application accounting a hard process-memory limit.
  - Entry points: [native ownership](../native/ReaderResources.swift), [Rust bridge](../crates/glim-app/src/views/reader/native_bridge.rs), [native resource tests](../native/tests/ReaderResourcesTests.swift).
- [ ] **R3: Pressure recovery across active readers.** The pool evicts caches and some readers release working data when hidden. Insufficient capacity makes `reserve_working` return a retryable error; it does not yet coordinate active readers to release reconstructible working buffers and resume requests.
  - Completion: multiple windows and one large operation can make progress within bounded working memory. Protect unsaved contents and necessary interaction state; unify release, retry and reconstruction, returning charges only when canceled work actually exits.
  - Entry points: [admission and cache reclamation](../crates/glim-core/src/resources.rs), [scheduler](../crates/glim-services/src/work/scheduler.rs), [SQLite suspension](../crates/glim-app/src/views/reader/sqlite.rs), [book suspension](../crates/glim-app/src/views/reader/books.rs).

### Incremental processing and performance evidence

- [ ] **P1: Remaining full scans, cloning and layout.** Incremental minimap updates are implemented. Markdown local reparsing still clones the block collection, copies source text and rebases subsequent source ranges; structural edits retain full parsing. Archive navigation still traverses every entry and rebuilds the list on the UI side. Large individual layout blocks also need measurement.
  - Completion: measure before reducing repeated work, adding indexes or background preparation where needed. Compare expanded incremental paths with full parsing, preserving references, lists, fences, extension nodes and source mapping. Report scale, edit location and latency quantiles.
  - Entry points: [Markdown edits](../vendor/gpui-base/src/text/format/markdown/edit.rs), [layout](../vendor/gpui-base/src/text/text_view.rs), [archive listing](../crates/glim-app/src/views/reader/archive.rs), [Markdown benchmark](../crates/glim-app/benches/markdown_edit.rs), [minimap benchmark](../crates/glim-app/benches/minimap_edit.rs).
- [ ] **P2: End-to-end reader measurements.** Parser, raster and tree microbenchmarks and a real-window source input/scroll runner exist. They do not cover complete paths for workspace save coordination, paging, asynchronous document/data previews, books, diagrams or native readers.
  - Completion: separately record first readable content, first viewport, input/scroll stalls and queue/preparation time, checking real content readiness. Retain fixtures, machine/toolchain/source provenance and raw before/after samples outside the repository.
  - Entry points: [real-window runner](../scripts/measure-source-reader.py), [measurement composition](../crates/glim-app/src/app/reader_measurement.rs), [measurement scope](reader-resources.md).
- [ ] **P3: Real cache benefits.** In-process and disk caching are implemented. The existing `diagram_cache` benchmark measures prepared PNG lookup/publication; disk tests cover reopening, invalidation, corruption, eviction and cancellation. Complete comparisons including actual rendering engines and first-viewport readiness remain missing.
  - Completion: cover first open, tab reopening, app restart, leaving/returning to the viewport, invalidation and pressure. Separate presentation/service/disk hits, misses, pending work, eviction, preparation actually avoided and first-viewport latency, including initial environment-fingerprint computation.
  - Entry points: [cache benchmark](../crates/glim-services/benches/diagram_cache.rs), [disk cache/tests](../crates/glim-services/src/markdown_preview/disk_cache.rs), [presentation cache](../crates/glim-app/src/views/reader/diagrams.rs).
- [ ] **P4: Acceptance on real hardware.** Release CI runs workspace tests and native updater checks on Apple Silicon and builds/verifies packages for both architectures. Those checks do not establish Intel hardware interaction, comprehensive UI, multiple-window or memory-pressure acceptance.
  - Completion: execute relevant automated regressions and Apple Silicon/Intel interactions for the scenarios below, recording actual coverage. Retain unexecuted or failing scenarios.
  - Entry points: [release CI](../.github/workflows/ci.yml), [native preview checks](../scripts/test-native-preview.sh), [isolated installer regression](../scripts/test-updater-install.py).

Suggested order: complete **M1 + M2** reader by reader, then **R1 + R3**; address **R2** alongside native paths. Add focused **P1–P3** measurements to each module and finish with **P4** acceptance. Schedule new formats and scenario capabilities separately.

## Candidate scenario capabilities

These product gaps are not prerequisites for internal modularity. See [format support](file-format-support.md) for specific limitations.

| Task | Existing capability and remaining gap | Entry points |
| --- | --- | --- |
| Office | Paged tables, cached values and a Word reading view exist; complex layout, charts/embedded objects, formula recalculation and complete OpenDocument reading remain limited. | [Workbook](../native/WorkbookPreview.swift), [table preparation](../crates/glim-services/src/spreadsheet.rs), [Word](../native/WordPreview.swift) |
| Reading | Chapters/contents, illustrations, bookmarks and positions exist; full EPUB publisher CSS and fixed-layout fidelity remain unsupported. | [Book parsing](../crates/glim-services/src/books/mod.rs), [reading state](../crates/glim-core/src/books.rs) |
| Continuing work | Workspaces/tabs and book positions persist; drafts have checkpoints and recovery. General scroll/cursor/media-position restoration is not unified, and crash/multiple-instance reliability still needs acceptance. | [Session model](../crates/glim-core/src/session.rs), [storage schema](../crates/glim-services/src/storage/schema.sql), [drafts](../crates/glim-services/src/drafts.rs) |
| Diagrams | Salt/Chen support common syntax; complete dialects, preprocessing and external-resource compatibility remain limited. | [Salt](../scripts/markdown/salt.js), [Chen](../scripts/markdown/chen.js) |
| Specialist files | GPX/KML coordinate tables and static OBJ/STL projections exist; GIS maps, interactive 3D and dedicated GGUF/ONNX/NPZ readers remain unavailable. | [Geographic tables](../crates/glim-services/src/geo.rs), [static projections](../crates/glim-services/src/vector_formats.rs), [format catalog](../assets/format-catalog.json) |
| Installation | Ad-hoc signing and Sparkle-signed feeds exist; Developer ID distribution and Apple notarization still require credentials and integration. | [App signing](../scripts/sign-macos.py), [update feeds](../scripts/create-update-feeds.py), [release process](releasing.md) |

## Remaining real-world acceptance

- Apple Silicon/Intel GUI interactions, graphics environments and accessibility, multiple windows and resource pressure.
- Real GitHub production upgrades, administrator-required install locations, multiple-instance conflicts, canceled authorization and interrupted replacement/recovery; isolated installer regressions do not replace these scenarios.
- Native providers with password/corrupt documents, office packages, media codecs, font collections and accessibility.
- Active SQLite/WAL sources, source-stability rejection, missing-main sidecars and preservation of the source database.
- Finder cold/warm opening, duplicate/cross-window activation, association changes/restoration and native view cleanup.
- Save conflicts, concurrent file operations, watcher refreshes, large readers, draft recovery and macOS gestures across windows.

## Outside current scope

Non-macOS platforms, LSP, external plugin installation and dynamic loading are outside current product scope. Existing public release assets are retained. Internal modularity follows the three-layer dependencies and [module contracts](reader-modules.md).

See [architecture](architecture.md), [resources and measurement](reader-resources.md), [updates](auto-update-design.md) and [release process](releasing.md).
