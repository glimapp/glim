# Reader module contracts

**English** · [简体中文](reader-modules.zh-CN.md) · [Architecture](architecture.md)

Glim's extension boundary is an internal, statically linked reader module. Modules share document identity, editing, task admission and workspace actions. They can reuse a presentation without creating a new renderer for every extension. Scenario pages compose these capabilities for users; scenarios do not introduce a second dispatch system.

## Where changes belong

| Responsibility | Owner | Extension rule |
| --- | --- | --- |
| Pure values and state transitions | `glim-core` | No I/O or UI dependencies |
| Detection, parsing, preparation and source access | `glim-services` | No GPUI entities or window state |
| Ordered built-in routing | `services/documents/registry.rs` | Stable, unique provider ID; preserve detection precedence |
| Text presentation classification | `services/documents/presentation.rs` | Classify markup, table, JSON dialect and structured controls once |
| Document identity and editing | `app/document_session.rs`, `app/edit_session.rs` | Reuse the live session across source and preview views |
| Prepared-content construction | `views/reader/construction.rs` | Assemble retained entities here; no format dispatch in Workspace |
| Read-only presentation lifecycle | `views/reader/surface.rs`, concrete reader module | Implement required rendering/kind and only supported optional operations |
| Text/source/preview behavior | `views/reader/text.rs`, `editing.rs`, `projection.rs` | Share saves and editor state; projections do not own another editor |
| Navigation, tabs, saves and restoration | `views/workspace` | Coordinate readers through their operations |
| Toolbar and content menus | `workspace/reader_actions.rs`, `native/ReaderActions.swift` | Reuse live actions and the existing native menu bridge |

`ReaderSurface` currently includes format-specific optional operations such as archive extraction and playlist state. It is an app-internal lifecycle boundary, not a stable public SDK. The source, markup and paged variants still have explicit composition because their editing and preview transitions are coupled. Do not move those transitions into opaque callbacks merely to remove an enum.

## Adding a format

1. Implement detection and preparation in a service module. Reuse an existing `documents::Content` representation when possible. Container-specific formats must precede the general archive provider; ambiguous extensions need content probes. Keep ordinary text and byte fallback reachable.
2. Register the provider with a unique kebab-case ID. The registry performs ordered routing, not independent plugin discovery. Text formats that reuse existing presentations usually extend `TextPresentation` or an existing provider rather than add a surface.
3. For a new presentation, add its prepared payload and a construction branch, then implement `ReaderSurface` in the concrete view module. Retain GPUI entities and task handles. `render` must not read files, scan directories or run blocking commands.
4. Define actual actions through the shared action path. Optional surface operations have unsupported/default behavior; do not advertise controls whose implementation is absent. Window-bound native surfaces must implement window binding and movement restrictions.
5. For native preparation, use the typed Rust/Swift preview request protocol. Update both decoders and the shared protocol fixtures when the payload changes. Retain prepared resource ownership until the native controller closes.
6. Update support documentation, architecture boundaries and relevant regression coverage. File icons, syntax highlighting and macOS associations describe different domains; an icon match does not prove the file is readable or editable.

## State and publication

`DocumentSession` owns identity, path, content snapshot, revision, draft/recovery metadata and the optional `EditSession`. `EditSession` retains the live editor, saved baseline, dirty flag, edit revision, lock state and change subscription. Source/minimap/preview objects own presentation state. Replacing a presentation must not detach the session's dirty tracking or erase undo history when adopting a draft's first saved path.

Save completion records the snapshot actually written. If typing continued during the write, the newer buffer stays dirty. Read-only images and document surfaces do not acquire an edit session. Text projections operate on editor snapshots; paged previews request a save before reading changed source from disk.

Refresh preparation publishes only when document identity, generation and relevant editor/page revisions still match. Dirty content stays protected; obsolete work must not replace a newer document. Keep the readable old view on refresh errors. A surface can supply in-place refresh when retaining its entity is necessary, as SQLite does.

## Background work and cancellation

Use `app/reader_jobs` for expensive reader preparation. It awaits `services/work/scheduler` admission before spawning blocking work, and holds the permit inside that work until it exits. The lanes are document preparation, projections/pages/queries, image decoding, native preparation and diagram batches. Their concurrency budgets live in `assets/reader-limits.json`.

Choose interactive priority for user-triggered work and background priority for inactive refresh/count preparation. Each priority is FIFO; occupied priorities alternate to avoid starvation. Priority does not preempt running work. Admit one operation or bounded batch, never the entire life of a tab or animation. Do not acquire another permit while holding one and synchronously waiting for it; same-lane nesting can deadlock, and cyclic cross-lane dependencies have the same risk.

Retain the owning task and cancellation guard. Dropping a queued admission removes its waiter. Cancellation tokens are cooperative: changing the flag alone does not wake an admission future; dropping its owning task removes it immediately, while a later poll observes the flag. Running loops check cancellation between chunks. Some SDK/parser calls finish before cancellation is observed; retain their permit until they return and discard obsolete output. `reader_job` telemetry reports queue and worker time under the existing consent policy.

Directory browsing, timers, metadata-only checks and persistence writes have separate ownership and are outside these preparation lanes. The scheduler limits concurrent work, not aggregate retained memory. Continue using per-reader streaming, paging and working-buffer budgets. Do not add total-file-size rejection limits. Regenerate policy inventories with `python3 scripts/generate-reader-limits.py --docs` after budget changes.

## Boundary checks and remaining work

Run `python3 scripts/check-architecture.py` to check workspace dependency direction, known UI dependencies in non-UI crates, provider IDs and common prepared-content dispatch regressions in Workspace. Its source scan is a lightweight guard, not a Rust semantic checker; it cannot prove that core code performs no I/O or that every operation is cancellable. Native Cargo check/clippy remain necessary. An all-targets compilation checks test code but does not execute it; runtime regression tests and macOS interactions are separate validation.

The current architecture leaves three concrete follow-up areas:

- Admission does not account for retained views/caches by bytes or prioritize individual visible blocks. Add a global memory policy and viewport scheduling only with measured workloads and eviction/reconstruction rules.
- Routing, syntax metadata, icons and system associations remain separate inventories. Cross-check shared claims before attempting a single catalog; preserve intentional differences between recognition, preview and editing.
- The app-internal surface contract and prepared-content factory still change with the application. Independent module packages would need narrower capability contracts and compatibility fixtures before extracting crates. External dynamic plugins additionally require an ABI/protocol, permissions, crash isolation and version negotiation; none is currently implemented.
