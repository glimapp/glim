# Usage statistics technical reference

[简体中文](telemetry.zh-CN.md)

For the switch, upload scope and local records, read [privacy and network access](privacy.md). This reference is for maintainers checking event fields, metric semantics and offline analysis. Installation counts, downloads and people are separate measures.

Telemetry follows the existing Settings switch. Enabled installations report feature events and sanitized errors to the configured Aptabase endpoint. Disabling stops delivery immediately and clears queued events. Installation identity is a random 128-bit token, stored in the settings table of Glim's application database, created only by the enabled background worker. It is unrelated to hardware, account, paths or document contents. It persists across launches and opt-out/re-enable; a missing identity record creates a new identity. An unwritable identity store omits the token. Count distinct `installation_id` values to estimate reach; this measures installations, not people. Multiple devices, application-data resets, opted-out users and undelivered events affect that estimate. Aptabase's built-in session count must not be relabeled as unique users. Cross-session identity is a custom property; distinct installation counts require export analysis or a compatible custom query.

## Development and release routing

Aptabase `systemProps.isDebug` follows build provenance. Local `cargo run`, local optimized builds and local `.app`/DMG packaging report debug telemetry. Only the official repository's pushed `v*` tag workflow, with `GLIM_DISTRIBUTION=github-release` and the optimized Cargo profile, embeds release routing. Manual workflows, ordinary branches and forks remain debug. The choice is compiled into the binary; moving a bundle or changing runtime environment variables does not change it. CI verifies the executable before packaging. `/Applications/Glim.app/Contents/MacOS/glim --build-info` prints its channel and version without starting the app, reading installation identity or sending telemetry.

Events also carry `telemetry_environment=debug|release`. Historical `system_profile.debug_build` values describe compiler debug assertions, independently of the telemetry channel; current builds no longer emit that event. Earlier local optimized builds were classified as release; this change does not relabel historical data. Exclude known development installation IDs when analyzing those exports.

Normal upgrades, replacement of the app bundle, opt-out/re-enable and switching distribution channels reuse the existing `telemetry-installation-id` in the application database. Neither version/build nor channel is part of that key. Clearing application data or moving to another device can create a new installation identity; sessions rotate independently. Identity remains an installation count, without account-based cross-device merging. Regression cases cover schema upgrades/restarts, concurrent first starts and read-only/invalid stores without rotating an existing identity.

## Format ranking

`file_open` is the authoritative new-open attempt/result, with `file_extension`, `file_family`, `result` and `duration_ms`. Reusing an existing tab emits `tab_activated`, not another open. Background reader refreshes can emit `reader_opened`; do not count them as user opens. Known extensionless config names use fixed categories; unrecognized suffixes become `other`, never a user-defined name. Opening a folder emits `workspace_open` separately. Native document preparation occurs after initial routing: inspect `native_load` results as well as `file_open`. Preparation success does not guarantee successful display, media playback or Office conversion inside AppKit.

For a fixed time window and version, rank formats by distinct installations attempting an open, then open attempts. Count attempted reach even when opening fails; otherwise broken formats disappear from demand data. Include open failures, native-load failures, P50/P95 latency and foreground duration. The initial open duration measures routing/loading, not time until every pixel or native document is ready. The current safe extension inventory does not identify every supported format; `other` requires expanding the allowlist, never transmitting arbitrary suffixes.

Export Aptabase events as JSON array, JSONL or CSV (columns `eventName`/`event_name`, `sessionId`/`session_id`, and `props`/`properties`, with nested JSON properties). Run:

```sh
python3 scripts/telemetry-format-report.py events.json --output formats.csv
```

Both report scripts accept ClickHouse JSONEachRow/CSV exports with string_props/numeric_props and snake_case system columns, including .jsonl.gz. JSONL is read line by line. Use --app-id to select the production application and --exclude-installations-file with a private UTF-8 list (one installation ID per line, # comments allowed); --exclude-installation-id may also be repeated. Keep personal installation lists outside the repository. Events without identity remain unidentified, even after exclusions.

The report adds successful_installations, repeat_open_installations_utc8 (successful opens on multiple UTC+8 dates within the export), and source_fallbacks. A file-only successful open can represent source fallback rather than a rendered mind map; inspect reader_fallback as well. Repeated-open counts are observations, not cohort retention rates. The report sorts by unique installations, reports missing identity separately, deduplicates sequenced events, and uses all identifiable installations present in the supplied export as the reach denominator. Export all events for the same interval for population reach; a file-only export yields reach within file-opening installations. Filter version/time in the export first. The script does not fetch production data or invent historical metrics; older `app_started`-only data cannot recover format history.

## Feature event map

| Area | Events and interpretation |
| --- | --- |
| Lifecycle | `app_started`, `telemetry_enabled`, `app_exited`, `window_opened`, `window_closed`; session duration and open tab/folder counts |
| Reading | `file_open_requested`, `file_open`, `workspace_open`, `reader_opened`, `tab_activated`, `tab_closed`; reader kind, preview/edit capabilities, availability |
| Engagement | `foreground_segment`; page, format, mode and milliseconds while window is active, excluding known prompts/loading/pickers; 30-second flush, transition/deactivation/close/quit flush |
| Editing | `draft_created`, `editing_started`, `file_save_requested`, `file_save`, `draft_save`, save-dialog decisions, rejected formats, close decisions, `draft_recovered`, `move_to_trash` |
| Navigation | `workspace_folder_added`, `workspace_search_completed`, preview and wrap changes, toolbar `reader_action`, Markdown task completion |
| Data and books | CSV/SQLite page requests and load results, SQLite table selections, paged mode/load, book load/page/chapter/bookmark |
| Diagrams | `diagram_render`, safe diagram kind and result; cache completion rather than per-frame paint count |
| Native controls | `native_reader_action`: image copy/OCR/zoom/paging, PDF fit/layout/presentation/navigation, Word mode/zoom, workbook paging/sheet, font selection, media playback, presentation/external-open actions; numeric bridge only |
| Preferences and updates | settings/shortcut openings, theme/language changes, integration add/remove outcomes, update actions, updater phase transitions when updater is compiled in |
| Remote workspaces | SSH test/save/connect/delete outcomes; remote workspace and file opens; local/SSH reader capability and foreground context |
| Find and embedded images | Source/preview/native find entry, single/all replacement, image detail opening and first zoom |
| Diagnostics | `operation_failed`, `error_logged`, `app_panicked`, `native_reader_error`, `error_repeated`, `telemetry_health` |

Page/load events include initialization and refresh; treat them as workload, not necessarily distinct intentional actions. Toolbar intent and actual state-change events describe different stages of one action: do not add them together. Native actions report invocation, not guaranteed completion. The shared native reader controls retain their existing targets and dispatch paths.

## Product decisions

Compare feature sessions against sessions with that capability, using `reader_opened.can_preview`, `can_edit`, format flags; capability does not prove a hidden toolbar was seen. High reach plus failures or slow loads supports urgent reliability work. High reach plus repeated preview switching, search or paging supports examining reader usability. Low reach with long foreground duration may describe a valuable specialist workflow. Foreground occupancy includes idle time and does not establish attention or satisfaction. Removal candidates need low opportunity-adjusted reach across several releases, low engagement, maintenance cost and user feedback; telemetry alone cannot establish that removal is safe. No automatic feature removal is implemented.

## Error and delivery semantics

Network payloads exclude document text, filenames, full paths, search queries, database/table/column names, URLs and raw log/error/panic text. Error properties contain classifications, application source basename/line, component/phase, safe fingerprints, OS error codes and up to three application/GPUI panic symbols. Recent context contains event names only. Full diagnostics remain local. Forced termination and crashes may prevent panic/shutdown delivery.

Structured operation outcomes are the failure-rate source. `operation_failed` and `error_logged` can describe the same failure; do not sum diagnostic events for operation failure rate. Repeated error signatures are suppressed within a minute and emitted as `error_repeated.occurrences` on a later flush or normal shutdown; bounded fingerprint eviction can lose repeat detail. `telemetry_health.errors_suppressed` describes transport compression and must not be added to occurrence totals.

The queue holds at most 256 events, batches at most 25, flushes approximately every two seconds, and uses cancellation, bounded shutdown flush and exponential upload backoff. No persistent outbox or raw-log upload exists. Network failure, contention and queue eviction can lose events; health counters disclose dropped events and upload failures when a later upload succeeds. A long-running process rotates Aptabase session identity daily; installation identity remains stable. Transport errors do not recursively enter diagnostic reporting.

## Feature collection (schema 3)

Ordinary events carry `schema_version=3`, architecture and macOS build; Aptabase system properties include OS/app versions, app build, locale and SDK. Routine telemetry does not collect hardware profiles, CPU/memory/GPU samples, cache counters or per-job scheduling timings. `reader_job`, `resource_sample` and `system_profile` are no longer emitted. Local resource admission, cache reclamation, cancellation and performance measurement tools remain independent of telemetry.

Use successful `file_open` installations for explicit file-opening reach and `reader_opened` for available reader capabilities, including restored tabs. `reader_opened.trigger=open|restore` separates newly opened and restored readers; `storage=local|ssh` distinguishes local and SSH content, and remote readers report `can_edit=false`. SSH `file_open` events include format, outcome and duration for explicit opens; successful automatic and manual refreshes do not count as new file opens. `workspace_open` with `storage=ssh` reports opening or restoration of a remote workspace, distinguished by `trigger`; the restoration path also serves recent-workspace and external opens. Foreground segments also carry storage type. App residency and event counts do not measure attention; document foreground duration measures occupancy and can include idle time.

`ssh_connection` reports test, save, connect and delete outcomes; `integration_configured` reports client category and add/remove outcomes only for explicit configuration actions, excluding discovery and automatic migration. `reader_feature.find_opened` covers source and text-preview find surfaces; native `find_opened` covers PDF, Word and HTML find surfaces, including direct keyboard entry. Replacement actions report only the single/all flag. Embedded image detail records `open_detail` and at most one `zoom` per open, including pinch gestures. No query, replacement text, hostname, user, key path, connection name or configuration path is uploaded.

`reader_scroll` groups continuous input after about 700 ms idle, with segments up to roughly 10 seconds. It records event count, duration and absolute horizontal/vertical travel. GPUI records points, lines×100 and positive/negative direction counts separately; native records delta×100 with `delta_unit` (points or lines). These are input deltas, not actual content displacement, scroll depth or reading completion. Native directional signs are not retained. GPUI reuses document event capture; native reuses the existing context router and deduplicates overlapping routers by event identity. No independent right-click monitor is added. Precision modes are separated rather than converted; do not directly combine unlike units or assume identical coverage. Native final segments rely on live timers and can be lost during termination.

`shortcut_used` reports resolved static GPUI command names, modifiers, allowlisted command-key tokens and active format. It includes app commands, book navigation, tree navigation and resolved editor copy/cut/paste, undo/redo, find and cursor commands. Text contents and arbitrary key strings are excluded; unknown binding keys become `other_bound_key`. Tokens distinguish alternate bindings such as Space and PageDown. Events consumed before GPUI resolution are not observed. `native_shortcut_used` covers recognized native copy/select-all, zoom/fit/actual size, preview copy, OCR, external-open and page shortcuts, with modifier flags and physical `key_code` for recognized commands only. Physical key codes do not describe characters produced by the user's keyboard layout. Internal Quick Look, WebKit, system-menu and AVPlayerView shortcuts bypassing these routes are not guaranteed. Native `input_source` is pointer/keyboard/menu/unknown; asynchronous outcomes and observed states do not infer a user source.

### Concrete operations

| Reader | Detail |
| --- | --- |
| CSV/Geo | Row/column previous/next, cell detail, offsets, table format category, load outcome/time |
| SQLite | First/last/previous/next/jump, filter-presence flags, sort direction, table selection, cell detail, extended timeout; separate open/columns/count/page/value outcomes |
| Books | Page/chapter, contents, font size/family, line spacing, page width, paper, bookmarks, links |
| XMind | Sheet/topic selection, node/all folding, expand all, reset, zoom buttons, copy |
| OPML/data tree | Outline expand/collapse; node folding, value detail and parse outcome/time |
| Markdown | Local/web/email link category, task toggles, preview and toolbar operations; no URLs or anchor text |
| Paged text/hex | Previous/next/expand page, bytes/report mode, copy page and load outcome/time |
| Native images | Image/selection copy, completion/failure, OCR observations/time, button/end-of-gesture zoom, fit/actual size, paging and menus |
| Word/HTML/workbook | Load completion/failure, reading mode/zoom/links/copy, workbook sheet/more-sheets, separate row/column paging |
| PDF/presentation/font | Zoom/fit/layout/presentation entry/exit/navigation/external-open/font selection |
| Media | Play/pause invocation, playlist navigation, failure, observed playback/waiting/pause/mute/volume, periodic position/duration |
| Workspace/explorer | Actual reader-fullscreen transitions, contextual toolbar calls, menu new/play/shuffle/open/reveal/copy-path/trash/remove-folder categories; no paths |

`reader_fallback` reports a successful fallback to complete paged XMind content, with a fixed reason (container/content_entry/decompression/json/xml/structure/unknown) and preview_budget flag; it does not mean interactive-map rendering succeeded. operation_failed includes reader_stage for XMind failures, and error signatures distinguish these phases without document text. telemetry_health now carries installation_id when available, schema, architecture, OS build and its own event_sequence; error_repeated aggregates also receive a fresh sequence. Older missing IDs cannot be reconstructed from these changes.

`reader_feature` describes attempts, not universally completed work. Some operations identify a reader family without an exact suffix (Word can combine doc/docx, books can combine epub/fb2); never invent that suffix. Exact format reach remains based on `file_open`. Native `semantic=invocation/outcome/observation` separates calls, outcomes and observed state. OCR outcomes include observation count and elapsed milliseconds, never recognized text; an empty result can be successful. Empty copy results use static error code 2. A playhead discrepancy over two seconds emits `playhead_jump_observed`; buffering/timing can trigger it, so it is not an exact seek-intent counter. Volume changes coalesce for 400 ms. Playback position is sampled about every 30 seconds while the native media host is visible. Disabling telemetry stops native collection and changes aggregation generation, so re-enabling cannot flush previous scroll/playback segments. Delivery remains bounded and best effort. Static integration and compilation do not establish interaction coverage; macOS interaction verification remains manual.

### Detail exports

```sh
python3 scripts/telemetry-detail-report.py events.json --output-dir report
```

By default, `timeline.csv` and `actions.csv` exclude legacy `reader_job`, `resource_sample` and `system_profile` events. Pass `--include-performance` to include them when investigating old exports. Historical samples still populate `resources.csv` and `resource-peaks.csv`; current builds produce no new samples.

Produces `timeline.csv` (events with safe properties), `actions.csv` (event/format/reader/action/source/result groups), `shortcuts.csv` (commands and chords), and `resources.csv` (main-process sample count, P50/P95/max by active format). Input is the same JSON/JSONL/CSV schema as the format report. Sequenced duplicate export rows count once. The detail script also writes activity.csv (per installation and UTC+8 day: file opens, successful opens, sessions with opens and document foreground seconds) and resource-peaks.csv (per installation/metric: value, timestamp, app version and window/tab context). Unknown identities remain blank; resource peaks describe the main process, not exclusive memory use by the selected reader. These separate active use from long-running background samples. Timeline retains export order; inspect session+sequence for ordering, not a guaranteed global causal order. Filter version/time in the export before comparison. Missing installation identities never become invented people counts.

`process_session_id` retains the enabled telemetry session identity when Aptabase rotates `sessionId` after 24 hours; `session_uptime_s` records monotonic elapsed time at event collection. Both reset when telemetry restarts, including opt-out/re-enable. They help group same-process continued use, but are not a person identity or proof of active return. `foreground_segment` and active-reader context include `untitled` to distinguish draft occupancy from file-reading occupancy. Draft creation/edit/save events still must be included when counting productive installations; successful `file_open` alone misses note-only use.

`tab_closed.reason=preview_replaced` distinguishes browsing replacement from explicit closure. `update_phase_changed` includes a fixed `error_stage`, fixed `error_kind` and signed numeric `error_code`; no native error message, URL or path is sent. Error codes are diagnostic values, not a complete failure cause.
