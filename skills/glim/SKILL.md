---
name: glim
description: Use Glim on macOS when asked to open a local file or workspace in Glim, preview Markdown in the browser, export Markdown, or inspect archives, SQLite databases, CSV/TSV tables and binary files with Glim.
---

Run `glim PATH` for a local file or directory. Use `glim preview INPUT.md` for a browser preview, or `glim html INPUT.md OUTPUT.html` and `glim pdf INPUT.md OUTPUT.pdf` for exports. Quote paths containing spaces. The output directory must exist and the output name must be new.

On successful export, stdout contains the generated path. Check stderr for missing image or diagram warnings. Use `glim formats` for the format catalog as JSON. If the agent shell has not inherited the updated PATH, invoke `"$HOME/.local/bin/glim"` directly.

Read archive contents with `glim archive FILE` or extract into a new folder with `glim extract FILE DIRECTORY`. Use `glim tables DB` and `glim columns DB TABLE` for SQLite schema discovery. For CSV/TSV, `glim rows FILE [ROW [COL]]` uses zero-based offsets. Continue with the returned `next_row` until null. Use `glim hex FILE` for a bounded byte preview.
