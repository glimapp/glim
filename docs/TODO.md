# Current coverage and remaining work

**English** · [简体中文](TODO.zh-CN.md) · [Documentation](README.md)

Glim covers [tasks](topics.md) across office work, reading, media, data and development. This page identifies gaps so you can decide whether it fits your work. Candidate work does not promise a release date.

## Available now

- Mixed files in workspaces, multiple workspaces and separate file windows.
- Documents, ebooks, mind maps, images, media playback, structured data and SQLite inspection.
- Basic UTF-8 editing with automatic or explicit saving; ZIP browsing and extraction.
- Normal-startup window, workspace and file-tab recovery; ebook bookmarks and progress; untitled-note checkpoints.
- In-app updates, application backups and one-time update-session recovery in configured releases.

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

LSP and plugins are not part of the current product scope. Candidate work is not a release-date promise.

See [formats](file-format-support.md), [architecture](architecture.md), [updates](auto-update-design.md) and [release process](releasing.md).
