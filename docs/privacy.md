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
| Usage statistics | Send feature and foreground activity events only when this option is enabled |
| Error diagnostics | Send sanitized failure and panic reports only when this separate option is enabled |

Ebooks do not fetch remote images or execute active content. Offline diagram engines ship with the app. Disabling updates, statistics and diagnostics does not disable HTML or Markdown network content. Inspect unfamiliar HTML cautiously as source code first.

## How do I choose what to share?

Settings → Help improve Glim has independent **Error diagnostics** and **Usage statistics** switches. Both options are enabled by default on installations with no recorded choice; there is no startup prompt. Disable either option in Settings at any time. Error diagnostics help the developer locate failures and fix crashes without collecting feature-use history. Either choice can be changed later; declining does not restrict file viewing or editing.

An explicit old combined-switch choice is preserved for both new switches. An old installation with no saved choice uses the same defaults as a new installation. Unreadable preferences disable both channels. Choices are stored together; a save failure is shown in Settings with a retry action, and the live choices still apply for the current run.

## What do usage statistics send?

When enabled, collection covers feature actions, fixed format categories, success/failure, operation timings and foreground segments. System information includes versions, architecture and locale. Routine collection excludes process resource samples and hardware profiles. Scroll and shortcut metrics do not record typed document text.

A random installation identifier links usage across sessions for an installation. Collection excludes accounts, hardware serial numbers, usernames and device names. It does not upload document content, filenames, full paths, search terms, database/table/column names, URLs, recognized text or raw logs/error messages.

This measures **active installations with usage statistics enabled**, not all active users. One person may have several installations. Opted-out, offline or undelivered activity is unknown; missing reports do not establish churn. Glim sends no hidden activity heartbeat when statistics are disabled. Cross-session linkage means the random identifier is pseudonymous, not unlinkable anonymity. Network requests also expose their source IP to the receiving service, even though Glim does not put an IP address in its telemetry payload.

## What do error diagnostics send?

Diagnostics contain error classifications, application source locations, component/operation, safe fingerprints, error codes, versions and limited application stack symbols. Repeated errors are aggregated. Document contents, filenames, paths and raw error text remain excluded. A panic report is best effort; abrupt termination can prevent delivery.

Diagnostics-only mode sends no usage events, activity reports or persistent installation identifier. Reports use a temporary session ID so related failures can be interpreted within a reporting session. Enabling both options also allows diagnostic events in usage exports to help explain failed operations.

Turning off an option immediately stops its collection and sending. A consent change clears pending data and cancels in-flight telemetry work; re-enabling does not replay activity from the disabled period. With both switches off, the telemetry worker does not run or fetch remote telemetry configuration. This does not disable separately configured updates or document network content, and it does not delete reports already received by the server. See the [telemetry reference](telemetry.md) for fields and interpretation.

## What should I back up?

Text edits save automatically to the original file. Back up important files yourself. Update backups contain the Glim application bundle, not a complete backup of your documents, preferences or drafts.

Share only sanitized samples you can publish. Full diagnostics stay local, and logs can contain paths; inspect them before sharing. Use [private reporting](../SECURITY.md) for vulnerabilities.
