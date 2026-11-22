---
name: glim
description: Use Glim on macOS when asked to open a local file or workspace in Glim, preview Markdown in the browser, or export Markdown using Glim’s renderer.
---

Run `glim open PATH` for a local file or directory. Use `glim preview INPUT.md` for a browser preview, or `glim html INPUT.md OUTPUT.html` and `glim pdf INPUT.md OUTPUT.pdf` for exports. Quote paths containing spaces. The output directory must exist and the output name must be new.

On successful export, stdout contains the generated path. Check stderr for missing image or diagram warnings. Use `glim formats` for the format catalog as JSON. If the agent shell has not inherited the updated PATH, invoke `"$HOME/.local/bin/glim"` directly.
