# Privacy and network access

**English** · [简体中文](privacy.zh-CN.md) · [Documentation](README.md)

## Where do my files open?

Documents, images, data and code are processed locally without uploading them to a hosted viewer. SQLite creates a private local snapshot. Document conversion, image decoding and media playback can create temporary resources.

Glim locally saves preferences, recent workspaces, paths, windows and file tabs, draft recovery and ebook progress. Most current records are in `~/Library/Application Support/Glim/glim.sqlite3`; application backups and logs remain separate files. Local records can contain paths and draft text. Excluding content from telemetry does not mean no content is stored locally.

## What can use the network?

| Content or action | Behavior |
| --- | --- |
| Markdown remote images | Downloads images referenced by the document; the destination server may receive requests |
| HTML preview | Can run CSS and JavaScript and load network content, depending on the page |
| External links | Open the destination through the chosen action |
| Application updates | Check release information, download and verify installers according to update mode |
| Usage statistics | Send usage events and sanitized diagnostics only when enabled in Settings |

Ebooks do not fetch remote images or execute active content. Offline diagram engines ship with the app. Disabling updates and statistics does not disable HTML or Markdown network content. Inspect unfamiliar HTML cautiously as source code first.

## What do usage statistics send?

When enabled, collection covers feature actions, fixed format categories, success/failure, operation timings and foreground segments. System information includes versions, architecture and locale. Routine collection excludes process resource samples and hardware profiles. Scroll and shortcut metrics do not record typed document text.

A random installation identifier links usage across sessions for an installation. Collection excludes accounts, hardware serial numbers, usernames and device names. It does not upload document content, filenames, full paths, search terms, database/table/column names, URLs, recognized text or raw logs/error messages.

Turning statistics off stops collection and sending and clears the pending queue. Re-enabling does not replay activity from the disabled period. Turning the switch off does not delete data already sent. See the [telemetry reference](telemetry.md) for fields and interpretation.

## What should I back up?

Text edits save automatically to the original file. Back up important files yourself. Update backups contain the Glim application bundle, not a complete backup of your documents, preferences or drafts.

Share only sanitized samples you can publish. Full diagnostics stay local, and logs can contain paths; inspect them before sharing. Use [private reporting](../SECURITY.md) for vulnerabilities.
