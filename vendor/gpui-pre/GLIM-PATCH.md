# Local GPUI recovery patch

Source: crates.io `gpui-pre` 0.3.8 (checksum `d7f0bc3ebba070f8981b3f8e905a12f47d27794a30d029e59cdb9edf7a4b3360`), used by pinned GPUI Kit 0.7.1. Apache-2.0 license retained. Examples and integration-test targets/assets are omitted; library sources and resources are retained.

Changes are limited to `src/app.rs` and `src/window.rs`:

- Mark windows closed at application removal or native close, and reject queued platform input before dispatching into a removed window. Other dispatch errors remain reported.
- Return entity leases and balance nested app updates before resuming an unwind.
- Return the Taffy layout engine before resuming a measurement unwind.
- Add opt-in `Window::set_draw_panic_handler`. Catch a frame unwind before the native callback boundary. Resume unrelated panics. For a file failure acknowledged by the app, discard the partial frame, clear element/view state and drawing stacks, and force a fresh frame while keeping the last presented frame. The app quarantines the file before acknowledging recovery.

This does not catch aborting panics, OOM, stack overflow, native exceptions or process termination. No new platform backend or platform dependency is introduced.
