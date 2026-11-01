# Start with your own files

**English** · [简体中文](user-guide.zh-CN.md) · [Documentation](README.md)

Glim is a file-viewing workspace for Mac. Use it to read a contract, study reference material, or inspect data and code. Open files you already have. Browse [topics](topics.md) to find tasks that fit your work.

## Open a file or a folder

See [installation](../README.md#download) first.

- ⌘O opens a file; ⌘⇧O opens a folder as a workspace. Use the sidebar to open related files.
- Add Folder to Workspace appends material to the current workspace. The sidebar can also hold several workspaces, each retaining its own tabs when switched.
- Home offers file, workspace and new-file actions, plus recent workspaces. Renaming a workspace changes its display name, not the directory on disk.
- Title-bar search finds workspace files by name or path, not by their contents. Use source-editor search within the current text.

## Keep related files open

Opening a file leaves a tab so you can return after opening another. Reopening the same file activates its existing tab or window. Back and Forward revisit previously opened files.

A tab's context menu can reveal it in Finder, copy its path or move it to a separate window. You can also drag a tab out. Collapse the sidebar for more reading space; the folder icon in the narrow rail expands it again.

⌘W closes the current tab or an empty window. ⌘⇧W closes the current window. Closing the last window leaves Glim running; ⌘Q quits. Unsaved work goes through save confirmation.

## Choose how to view the content

| Content | Common actions |
| --- | --- |
| Contracts, reports and presentations | Page, select text and zoom in PDFs; switch Word Reading mode / Original layout; show supported documents fullscreen |
| Spreadsheets and data | Switch Excel worksheets and pages; read CSV/TSV tables; expand JSON trees |
| Notes and technical documents | Switch Markdown Preview / Source; read formulas and diagrams; check and save tasks in ordinary task lists |
| Images and fonts | Zoom and copy images; extract text from supported raster previews; enter font samples |
| Ebooks and comics | Contents, chapters or comic pages, typography, bookmarks and saved position |
| Audio and video | Playback and ordered or shuffled folder playlists |
| Mind maps and structured data | Expand or fold XMind topics; switch supported data to an interactive graph |

Actions appear in toolbars or content context menus. Some floating toolbars appear when you pause the pointer near the bottom of the content. File fullscreen expands the current reader; Escape returns to the workspace. Keyboard help is in Settings → Keyboard Shortcuts.

Actions differ by format. See [topics](topics.md) and [format compatibility](file-format-support.md).

## Make a small text edit

Ordinary UTF-8 text supports typing, selection, undo/redo and ⌘S. Existing files save automatically after a short typing pause, directly to the original. Keep this in mind with configuration and shared files. If another application changes the file before saving, Glim keeps your unsaved editor content and reports a conflict.

⌘N creates an untitled note. Its first ⌘S asks for a location and name; later edits save automatically. Changing the extension does not convert text into a PDF or Office document.

Large or long-line text uses editable pages; save pending page edits before navigating when prompted. Tables, trees, databases, images and Office previews are read-only; edit supported text through Source. Closing unsaved content offers Save, Don't Save or Cancel. A failed save leaves it open.

## Inspect a database or unpack a handoff

Open the main SQLite file, choose a table or view, filter, sort and page. Double-click a cell for its full value. Glim creates a separate read-only snapshot with available WAL/journal data; it does not modify or checkpoint the source. Encrypted databases and arbitrary write SQL are unsupported.

Browse ZIP entries and choose extraction to create a separate directory, then open it as a new workspace. Existing directories are not overwritten, and extraction can be cancelled. TAR currently offers listings only. GZ/BZ2/XZ support on-demand decompressed source reading.

## Continue next time

Normal startup can restore saved windows, workspaces, ordered file tabs and supported preview modes. Open recent workspaces in the current or a new window. Moved or deleted files remain visible as unavailable tabs.

General scroll and cursor positions and media time are not fully restored. Restored media stays paused. Ebooks separately save reading progress and bookmarks.

Untitled notes use local recovery checkpoints. Changes not yet checkpointed can still be lost; save important notes as files. Application updates have a separate one-time restart checkpoint; see [updates and recovery](updates.md). Session records and application backups do not replace backups of your files.

## Settings and help

Settings includes appearance, language, default file associations, update checks, shortcuts and usage-statistics controls. Making Glim the default for a file type can cause Finder to open those files in Glim.

See [privacy](privacy.md) for network and local-storage behavior. For opening, preview or saving problems, check [common questions](faq.md), then [report an issue](https://github.com/glimapp/glim/issues/new?template=bug_report.yml) with your version, Mac type, steps and a sanitized sample. Logs may contain local paths; review them before sharing.
