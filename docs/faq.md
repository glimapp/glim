# Common questions

**English** · [简体中文](faq.zh-CN.md) · [Documentation](README.md)

## What would I use Glim for?

Use a workspace for mixed reference material, or when switching repeatedly between documents, images, data and code. [Topics](topics.md) cover office work, study, content creation and development. Apple Silicon and Intel Macs are supported.

## Do I need to import files or create an account?

Local viewing requires no account or upload to a hosted viewer. ⌘O opens a file; ⌘⇧O opens a folder. Workspaces save organization and session information; original files stay where they are.

## Can I edit Word, Excel, PDF or images?

These previews focus on viewing, without document-layout, spreadsheet-cell or image editing. Ordinary UTF-8 text supports basic editing, including notes, code and configuration. Supported text formats such as CSV can be changed through Source. Existing text files save automatically to the original.

## Why does an Office preview differ from its original application?

Complex layout, charts, embedded objects and system-provider support affect the result. Excel uses saved formula results without recalculation. Try Original layout for Word; use the appropriate application for precise layout and specialist editing. See [format compatibility](file-format-support.md).

## Can I extract text from a scan or screenshot?

Supported raster images, the current PDF page and selected Office/iWork embedded images offer local text recognition. Results are plain text and can contain mistakes; check them before copying. SVG and legacy binary Office embedded images are outside this recognition path.

## Why doesn't search find a sentence inside a file?

Title-bar search matches workspace filenames and paths. Use ⌘F or the reader's magnifier to search the current document in a compact overlay at the upper right. Source text and rendered Markdown support case-sensitive, whole-word and regular-expression matching; editable source also supports replace and replace all. Enter/Shift+Enter and ⌘G/⇧⌘G move between matches; Esc closes the overlay.

HTML, PDF and Word reading mode support text search with a case-sensitive option. PDF searches selectable text, so scanned pages require OCR first. Search in paged text editing covers the current page, as the input indicates. Structured tables/trees, book previews and Quick Look do not currently provide this document search; use source mode where available. Native previews do not offer regex or replacement; WebKit reports whether a match exists without a total count.

## Where are the reading controls?

Some floating toolbars appear when you pause the pointer near the bottom of the content. Available actions depend on the file. Content context menus also offer supported reading commands. Settings includes shortcut help.

## Can I continue after restarting?

Saved windows, workspaces and file tabs can return. General scroll and cursor positions and media time are not fully restored. Ebooks save their own reading progress. Save important notes as files; draft recovery does not replace backups.

## Why do some videos, maps and 3D files show only part of the content?

Media depends on system codecs, and MKV supports a specific codec path. GIS currently offers data trees or coordinate tables; 3D offers static mesh projections. Topic names describe use cases; see [topic limits](topics.md) for actual actions.

## Does ZIP extraction change the original?

It keeps the archive and writes to a separate new directory without overwriting an existing one. After success, that directory can open as a new workspace. Extraction can be cancelled; failures clean up temporary output. TAR currently offers listings only.

## What happens with large files?

Supported readers use paging, streaming or on-demand decoding. Complex content may fall back to source or byte pages; some indexes take time to prepare. Opening a file does not mean all content has been loaded into memory or laid out at once.

## Are files uploaded? Can I avoid network access?

Local viewing does not require uploading files. HTML can execute scripts and use the network; Markdown can load remote images. Updates, enabled usage statistics and independently enabled error diagnostics also use the network. Turning off statistics, diagnostics and updates does not block a document's own network content. See [privacy and network access](privacy.md).

## What if macOS blocks installation or a file won't open?

Check that the installer came from [project releases](https://github.com/glimapp/glim/releases/latest), then follow [first-launch help](../README.md#macos). Retry a failed preview or use the default application. Report the version, Mac type, format, steps and a sanitized sample. Do not publish private files, passwords or unchecked logs.
