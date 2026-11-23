# Privacy and network access

**English** · [简体中文](privacy.zh-CN.md) · [Documentation](README.md)

## Where do my files open?

Documents, images, data and code are processed locally without uploading them to a hosted viewer. SQLite creates a private local snapshot. Document conversion, image decoding and media playback can create temporary resources.

Glim saves preferences, recent workspaces, paths, windows, file tabs, recovery drafts and ebook progress locally. Most records are in `~/Library/Application Support/Glim/glim.sqlite3`. Application backups and logs are stored separately. Local records can contain file paths and draft text.

## What can use the network?

| Content or action | Behavior |
| --- | --- |
| Markdown remote images | Downloads images referenced by the document; the destination server may receive requests |
| HTML preview | Can run CSS and JavaScript and load network content, depending on the page |
| External links | Open the destination through the chosen action |
| Application updates | Check release information, download and verify installers according to update mode |
| Usage statistics | Report feature usage and foreground activity |
| Error diagnostics | Report sanitized failures and crashes |

Ebooks do not fetch remote images or execute active content. Offline diagram engines ship with the app. Inspect unfamiliar HTML cautiously as source code first.

## What do usage statistics send?

Usage reports include feature actions, format categories, operation results and timings, and time spent in the foreground. System information includes versions, architecture and locale. Routine collection excludes process resource samples and hardware profiles. Scroll and shortcut metrics do not record typed document text.

A random installation identifier links usage across sessions for an installation. Collection excludes accounts, hardware serial numbers, usernames and device names. It does not upload document content, filenames, full paths, search terms, database/table/column names, URLs, recognized text or raw logs/error messages.

The installation identifier is pseudonymous and measures activity per installation. One person may use several installations. The receiving service can see the source IP of network requests. Glim excludes IP addresses from report contents.

## What do error diagnostics send?

Diagnostics contain error classifications, application source locations, component/operation, safe fingerprints, error codes, versions and limited application stack symbols. Repeated errors are aggregated. Document contents, filenames, paths and raw error text remain excluded. A panic report is best effort; abrupt termination can prevent delivery.

Diagnostic reports use a temporary session ID to group related failures. See the [telemetry reference](telemetry.md) for report fields.

## What should I back up?

Text edits save automatically to the original file. Back up important files yourself. Update backups contain the Glim application bundle, not a complete backup of your documents, preferences or drafts.

Share only sanitized samples you can publish. Full diagnostics stay local, and logs can contain paths; inspect them before sharing. Use [private reporting](../SECURITY.md) for vulnerabilities.
