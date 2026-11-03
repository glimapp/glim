# Reader module contracts

**English** · [简体中文](reader-modules.zh-CN.md) · [Architecture](architecture.md)

Glim keeps three crates: app, services and core. Reader modularity is enforced through ownership and dependency boundaries inside them. A new format can reuse an existing view; scenarios compose document, data, media and editing capabilities without another dispatcher. External plugin installation/loading is deferred.

```mermaid
flowchart TD
  Workspace[Workspace: navigation and document sessions] --> Factory[reader/construction]
  Factory --> Contract[reader/contract: lifecycle and optional capabilities]
  Factory --> Views[Reader modules: retained presentation state]
  Services[Services: source access and preparation] --> Core[Core: pure document values]
  Views --> Core
  Views --> Contract
  Factory --> Services
```

## Interfaces and ownership

`ReaderView` requires kind and rendering, with activation/window hooks. Workspace visibility is authoritative for activation, including keyboard tab switches and returning from settings/loading. `Reader` delivers only state transitions; a newly constructed hidden reader still receives its first deactivation. `ReaderOperations` borrows only implemented image-copy, extraction, playback-state and persistence traits. `ReaderSurface` adds app-owned refresh policy; native/SQLite implementations can retain in-place state. `SurfaceEntity` adapts existing GPUI entities. Source/markup/paged composition stays explicit because it owns shared editing transitions.

`ReaderHost` supplies translation, typed numeric action metadata and the shared resource pool. OPML and graph presentations use this host and pure `glim-core::outline`/`graph` payloads, without importing service implementations or application globals. The graph presentation serves XMind and structured-data visualization. Both have source/payload fixtures in `assets/reader-contracts`, shared by producer and presentation regression cases. Fixtures cover hierarchy/order, Unicode, URLs and fold behavior. They are internal contracts, not a serialized external plugin protocol.

Prepared table results cross the service/view boundary as immutable `glim-core::resources::TextRows`, retaining their working admission with the cells. CSV/TSV, geographic and SQLite views do not reconstruct a second resource charge after publication. Text/hex pages similarly retain text, row ranges, widest-row metadata and their reservation in the prepared page.

Raster results cross that boundary through `glim-services::media::RasterFrame`, which owns an `image::Frame` and its working reservation without GPUI types. PNG preparation and GIF playback acquire admission before allocating pixels; the app converts channels in place and transfers both pixels and reservation into its display owner. GIF playback also accepts an explicit resource pool for independent preparation and measurement. This isolates buffer ownership; media views still use application scheduling and service entry points.

Workspace owns navigation and session coordination. `DocumentSession` owns identity, path, saved snapshot, revision and optional `EditSession`; source and preview share the editor, baseline, dirty/locked state and subscriptions. First save/window transfer preserves undo and recovery identity. Save completion confirms the snapshot actually written; newer typing remains dirty. Refresh publication checks identity/generation and relevant edit/page revisions, protecting dirty content and preserving readable views on failure.

## Adding a format

1. Update suffix sets, provider claims and relevant association/icon expectations in `assets/format-catalog.json`, then run `python3 scripts/generate-format-catalog.py --write`. Filename language rules and content probes remain code. Catalog declarations do not grant editability.
2. Implement detection/preparation in services, preserving ordered container routing, cancellation and complete-source fallbacks. Reuse `documents::Content`/`TextPresentation` where suitable.
3. Compose retained entities in `reader/construction`. Views consume prepared values; render/layout do not read files or run blocking commands. Add a capability only when there is a concrete caller and implementation.
4. Route actions through `workspace/reader_actions` or native `ReaderActions`, reusing live actions for content menus. Keep task/resource ownership explicit; follow [resource lifecycle](reader-resources.md).
5. When crossing a module boundary, add representative producer/consumer fixtures. Native requests retain their versioned Rust/Swift protocol and `native/tests/fixtures`. Update architecture checks and current behavior documentation.

## When to split a crate

A separate crate requires an actual independent-version, dependency-isolation or reuse requirement. Interface size, number of supported extensions and a desire for a tidier directory do not establish that requirement. Before extracting, show that the module can operate through prepared values and host operations, identify who will maintain/version it, and justify the added build and adapter overhead. Current reader modules stay in the app crate.

`scripts/check-architecture.py` checks the three-crate direction, known UI dependencies in core/services, provider/catalog consistency and workspace dispatch regressions. It also rejects direct service/app-global/filesystem imports in the contract, controls, OPML and graph modules. These source guards supplement native Cargo compilation; they do not prove arbitrary Rust is I/O-free or cancellable. All-targets compilation checks test code without executing it. Runtime and macOS interaction verification remain separate.
