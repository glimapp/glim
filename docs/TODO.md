# Backlog and acceptance gaps

**English** · [简体中文](TODO.zh-CN.md) · [Documentation](README.md)

Glim covers [tasks](topics.md) across office work, reading, media, data and development. This page identifies gaps so you can decide whether it fits your work. Candidate work does not promise a release date.

## Architecture and performance backlog

Constraints: retain `app → services → core`; use ordinary reader modules first. Separate crates require concrete versioning, dependency isolation or reuse needs. Formats and scenarios compose existing capabilities. External plugin installation and dynamic loading remain deferred.

| Area | Implemented foundation | Completion criterion |
| --- | --- | --- |
| Reader modularity | Lifecycle/capability contracts, centralized construction, shared edit sessions; OPML, graph and JSON presentation consume pure data and injected host operations | The remaining readers have explicit, verifiable preparation/presentation/host boundaries; a few isolated modules do not establish independent portability for all readers |
| Resource management | Shared pool and work admission; major text/hex, table, tree and PNG/GIF buffers reserve bytes; several views release and reconstruct hidden resources | Controlled working buffers acquire shared admission before allocation, active readers support pressure reclamation, and framework/helper coverage is explicitly measured |
| Incremental work and measurement | Markdown single-line prose edits reuse blocks; input removes redundant copies; parser, raster, tree, source-window and image-cache measurement entries exist; diagram cache layers have separate counters | Real workspaces and reader families have reproducible first-content/interaction/resource measurements and before/after comparisons, with conclusions limited to measured scope |

### Reader modularity

- [ ] **M1: Remaining reader boundaries.** Review CSV, SQLite, books, media, archives, paging, source/markup and native adapters. Separate preparation, retained state, presentation and host operations. Acceptance: presentation performs no file I/O or background orchestration; construction injects required app dependencies and architecture checks cover the actual boundaries.
- [ ] **M2: Lifecycle contract acceptance.** Cover activation/hiding, refresh, cancellation, retry, closing and window transfer. Acceptance: preserve the editing session, undo and unsaved state; stale jobs cannot replace newer revisions, and representative producer/consumer contracts pass at runtime.

### Resource coverage and pressure recovery

- [ ] **R1: Remaining controlled allocations.** Admit parser `Value`/JSON5 workspaces, final graph output, book HTML/metadata, encoded previews/SVG intermediates and controlled SQLite/geographic parsing scratch. Acceptance: allocation and reallocation peaks fit the budget; ownership retains reservations across workers/publication; remove working-buffer `track`/`resize` paths that bypass admission.
- [ ] **R2: Native and helper resources.** Cover controlled preparation, conversion and copy/OCR working sets; distinguish opaque ImageIO, WebKit, Quick Look, GPUI atlas, SQLite engine and helper allocations. Acceptance: actual system-memory observations and pressure degradation/recovery evidence; application accounting is not described as a process-memory hard limit.
- [ ] **R3: Reclaim across active readers.** Unify release/retry/reconstruction of replaceable resources under pressure, including multiple windows and one large operation. Acceptance: progress with bounded working memory, protected unsaved text and necessary interaction state, charges retained until canceled work actually exits, and readable restoration on return.

### Incremental work and performance evidence

- [ ] **P1: Remaining whole-document work.** Measure and reduce minimap copying/geometry reconstruction, Markdown block cloning and large-block layout. Expand structural-edit block reuse when measured gains justify it. Acceptance: full-parser semantic equivalence, correct references/lists/fences/source mapping and safe fallbacks, with document scale, edit location and latency quantiles reported.
- [ ] **P2: End-to-end scenarios.** Extend measurements to workspace save coordination, paging, asynchronous document/data previews, books, diagrams and native readers. Acceptance: distinguish first content, first viewport and scroll/input stalls from queue/preparation time; verify actual content readiness and retain fixtures, machine/toolchain/source provenance and raw before/after samples.
- [ ] **P3: Real cache benefits.** Measure both image and presentation caches during opening, repeat visits, leaving/returning to the viewport and memory pressure. Acceptance: separate hits, misses, pending requests, reclamation and preparation actually avoided. The current cache benchmark covers prepared PNG lookup/publication only, not engine or first-viewport speedups.
- [ ] **P4: Runtime acceptance.** Execute relevant regressions and Apple Silicon/Intel interactions, including multiple windows and pressure. Compilation does not replace this acceptance; unexecuted cases remain open.

## Gaps by task

| Task | Current gap |
| --- | --- |
| Office | Complex layout, charts and embedded objects, formula recalculation, complete OpenDocument reading |
| Reading | Publisher EPUB styling and fixed-layout fidelity |
| Continuing work | General scroll/cursor and media-time recovery; draft-recovery reliability |
| Diagrams | Complete PlantUML Salt/Chen dialects; external resources remain restricted |
| Specialist files | GIS maps and interactive 3D are unavailable; GGUF/ONNX/NPZ readers remain unfinished |
| Installation | Developer ID signing and Apple notarization require credentials |

## Remaining acceptance coverage

- Apple Silicon and Intel GUI interactions on real hardware, supported graphics/drivers and accessibility.
- Real GitHub production upgrades, administrator-required install locations, multi-instance conflicts, canceled authorization and interrupted replacement/recovery.
- Native document providers: password/corrupt documents, office packages, media codecs, font collections and accessibility behavior.
- Active SQLite/WAL sources, source-stability rejection, missing-main sidecars and preservation of the source database.
- Finder cold/warm opening, duplicate/cross-window activation, file-association changes/restoration and native view cleanup.
- Save conflicts and concurrent file operations across windows, watcher refreshes, large readers and platform gestures.

Existing Rust/native/isolated installer checks are useful but do not prove all of these scenarios. Run relevant checks when validating a change and report the actual coverage in its release notes.

## Outside current scope

Non-macOS platforms are outside the product scope. Existing public release assets are retained.

LSP, external plugin installation and dynamic loading are outside current product scope. Internal modularity follows the dependency boundaries above.

See [formats](file-format-support.md), [architecture](architecture.md), [updates](auto-update-design.md) and [release process](releasing.md).
