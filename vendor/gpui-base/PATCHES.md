# Local gpui-base patch

Based on crates.io gpui-base 0.7.1 (Apache-2.0), archive checksum `3cf698677b253118459f552fe15513858c669d79a25837a94e480db2ae330050`.

TextView exposes an optional `on_task_toggle` callback. Markdown task markers use the existing native Checkbox control only when a callback and source span are available; all other TextViews retain their original read-only rendering. The callback provides the list item's original parsed source range and requested checked state. Applications own snapshot validation, edits and persistence. Checkbox pointer activation does not start text selection; keyboard and accessibility behavior come from the existing Checkbox control.

The task-toggle patch touches `src/text/text_view.rs`, `src/text/state.rs` and `src/text/node.rs`. The dependency version remains pinned at 0.7.1.

Additional presentation patches in the current source:

- `src/text/style.rs` and `src/text/document.rs` add scroll-bottom padding inside the virtual list, keeping its geometry shared with the minimap and scrollbar.
- `src/text/text_view.rs` exposes the installed default style so Glim can override only table sizing and end spacing.
- `src/text/node.rs` applies authored image height alongside width; automatic-height measurement is handled by the companion gpui-pre patch.

`src/text/format/markdown/edit.rs` adds replacement-block reuse for safe single-line prose edits, preserving UTF-8 boundaries and rebasing source spans. It verifies the old fragment before reuse and falls back for structural/global-reference changes and opaque custom-node ranges. `state.rs` integrates this into synchronous and background replacements, retains reference context, and reparses complete sources when appended definitions can change earlier links. Its test-support-only measurement entry point drives Glim's `markdown_edit` benchmark; it adds no runtime plugin interface.

Keep the local APIs and behaviors in the text state, node, style, document, text-view and Markdown parser modules when upgrading; the version number alone does not establish compatibility.
