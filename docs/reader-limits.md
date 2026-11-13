# Shared reader policy

All application reader boundaries take their numeric values from [assets/reader-limits.json](../assets/reader-limits.json). Edit a category’s `value`, preserving its `type`, `unit` and `effect`, then rebuild and install. Null values retire total-size/count gates. Finite values govern a specific feature, page or resource budget; see the behavior description.

Build scripts use `scripts/generate-reader-limits.py` to generate Rust constants, Swift ReaderLimits and embedded JSON in Cargo OUT_DIR. The diagram WebView receives that same JSON. Rendering does not read the policy from disk. Format validity, codec support and UI geometry remain responsibilities of their implementations.

This page is generated from the same policy. After editing it, run `python3 scripts/generate-reader-limits.py --docs` to update both language inventories. Ordinary builds write generated files only to OUT_DIR.

## openapi

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `openapi.field_bytes` | 4 KiB | bytes | page | Decoded label/value prefix per indexed JSON node. Complete original tokens remain available through raw-value byte pages. |
| `openapi.page_nodes` | 60 | items | page | Endpoint or child-node window. Navigation replaces the current window; collapsed content releases its DOM. |
| `openapi.depth` | 96 | levels | fallback | Active streaming-index parser stack depth. Deeper JSON retains the ordinary source reader. |

## text

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `text.editable_bytes` | None | bytes | unlimited | Read complete UTF-8 text when memory permits; no size, line-length or line-count switch to pages. Memory pressure retains editable source-page fallback. |
| `text.editable_newlines` | None | items | unlimited | Read complete UTF-8 text when memory permits; no size, line-length or line-count switch to pages. Memory pressure retains editable source-page fallback. |
| `text.line_bytes` | None | bytes | unlimited | Read complete UTF-8 text when memory permits; no size, line-length or line-count switch to pages. Memory pressure retains editable source-page fallback. |
| `text.page_bytes` | 1 MiB | bytes | budget | Bytes per normal text page; following pages preserve the rest of the file. |
| `text.full_bytes` | None | bytes | unlimited | Read complete UTF-8 text when memory permits; no size, line-length or line-count switch to pages. Memory pressure retains editable source-page fallback. |
| `text.full_newlines` | None | items | unlimited | Read complete UTF-8 text when memory permits; no size, line-length or line-count switch to pages. Memory pressure retains editable source-page fallback. |
| `text.layout_run_bytes` | 1 KiB | bytes | budget | Maximum text run per virtual read-only row; long logical lines wrap into further rows without losing source bytes. |

## binary

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `binary.preview_bytes` | 256 KiB | bytes | page | Bytes per hex page; previous/next controls expose the complete file. |

## image

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `image.input_bytes` | None | bytes | unlimited | File-backed native image loading has no application input-byte or total-pixel gate; File bytes preserves complete source access. |
| `image.pixels` | None | pixels | unlimited | File-backed native image loading has no application input-byte or total-pixel gate; File bytes preserves complete source access. |
| `image.decoded_bytes` | 512 MiB | bytes | budget | Active native/QOI preview raster working memory. QOI samples pixels while streaming instead of rejecting wide source scanlines; actual-size mode may use a scaled raster and complete original file bytes remain accessible. |
| `image.preview_edge` | 4,000 | pixels | budget | Initial ImageIO thumbnail resolution. Actual-size mode may increase the raster within image.decoded_bytes; it never rejects a file based on original dimensions. |

## gif

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `gif.preview_edge` | 2,048 | pixels | budget | Displayed animation texture edge, independent of source size and frame count. |
| `gif.canvas_bytes` | 512 MiB | bytes | budget | Compositing working memory; oversized source canvases are sampled while decoding rows instead of rejecting the animation. |
| `gif.batch_bytes` | 16 MiB | bytes | budget | Per-batch animation prefetch budget; consumed frames are released and later frames remain available. |
| `gif.batch_frames` | 3 | items | budget | Per-batch animation prefetch budget; consumed frames are released and later frames remain available. |

## tiff

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `tiff.decode_buffer_bytes` | None | bytes | unlimited | ImageIO loads the requested TIFF page on demand, with no application page-count or aggregate-preview total gate. |
| `tiff.pages` | None | items | unlimited | ImageIO loads the requested TIFF page on demand, with no application page-count or aggregate-preview total gate. |
| `tiff.preview_decoded_bytes` | None | bytes | unlimited | ImageIO loads the requested TIFF page on demand, with no application page-count or aggregate-preview total gate. |
| `tiff.preview_encoded_bytes` | None | bytes | unlimited | ImageIO loads the requested TIFF page on demand, with no application page-count or aggregate-preview total gate. |

## inline_image

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `inline_image.preview_edge` | 2,048 | pixels | budget | Inline thumbnail resolution/decoded/output working budget; the original remains available through its standalone reader or source URL. |
| `inline_image.encoded_bytes` | None | bytes | unlimited | File-backed or streamed image preparation has no application encoded-file-size gate. |
| `inline_image.decoded_bytes` | 64 MiB | bytes | budget | Inline thumbnail resolution/decoded/output working budget; the original remains available through its standalone reader or source URL. |
| `inline_image.output_overhead_bytes` | 1 MiB | bytes | budget | Inline thumbnail resolution/decoded/output working budget; the original remains available through its standalone reader or source URL. |

## font

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `font.input_bytes` | None | bytes | unlimited | System file-backed font metadata retains all faces, resources and sample text without application total gates. |
| `font.faces` | None | items | unlimited | System file-backed font metadata retains all faces, resources and sample text without application total gates. |
| `font.resource_types` | None | items | unlimited | System file-backed font metadata retains all faces, resources and sample text without application total gates. |
| `font.sample_chars` | None | characters | unlimited | System file-backed font metadata retains all faces, resources and sample text without application total gates. |

## playlist

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `playlist.files` | None | items | unlimited | Virtualized playlist rows retain every eligible file; no application file-count gate. |

## mkv

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `mkv.playlist_bytes` | None | bytes | unlimited | Read frames from disk, index timestamps on disk and write fragmented MP4 for the selected playlist item in a cancellable helper; no total size/count/duration gate. |
| `mkv.input_bytes` | None | bytes | unlimited | Read frames from disk, index timestamps on disk and write fragmented MP4 for the selected playlist item in a cancellable helper; no total size/count/duration gate. |
| `mkv.frame_bytes` | 16 MiB | bytes | budget | One active compressed-frame/codec-metadata allocation budget, independent of movie size; original file bytes remain accessible. |
| `mkv.frames` | None | items | unlimited | Read frames from disk, index timestamps on disk and write fragmented MP4 for the selected playlist item in a cancellable helper; no total size/count/duration gate. |
| `mkv.parser_read_bytes` | 2 GiB | bytes | budget | Parser work allowance resets after each frame, not cumulatively across the movie; optional seek metadata is skipped in a streaming pass. |
| `mkv.parser_operations` | 20,000,000 | operations | budget | Parser work allowance resets after each frame, not cumulatively across the movie; optional seek metadata is skipped in a streaming pass. |
| `mkv.tracks` | None | items | unlimited | Read frames from disk, index timestamps on disk and write fragmented MP4 for the selected playlist item in a cancellable helper; no total size/count/duration gate. |
| `mkv.duration_seconds` | None | seconds | unlimited | Read frames from disk, index timestamps on disk and write fragmented MP4 for the selected playlist item in a cancellable helper; no total size/count/duration gate. |
| `mkv.nal_units` | None | items | unlimited | Read frames from disk, index timestamps on disk and write fragmented MP4 for the selected playlist item in a cancellable helper; no total size/count/duration gate. |
| `mkv.nesting` | 16 | levels | budget | Track-metadata parser stack budget; optional chapters/tags/cues are skipped, and file-byte inspection remains available. |
| `mkv.elements` | None | items | unlimited | Read frames from disk, index timestamps on disk and write fragmented MP4 for the selected playlist item in a cancellable helper; no total size/count/duration gate. |

## json

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `json.input_bytes` | None | bytes | unlimited | Prepare the complete structured tree under parser and retained-node memory admission, without an input-size or node-count gate. |
| `json.field_chars` | 512 | characters | budget | Tree-label summary only; expand containers or double-click to inspect the complete key/value. Source retains all content. |
| `json.nodes` | None | items | unlimited | Prepare the complete structured tree under parser and retained-node memory admission, without an input-size or node-count gate. |
| `json.description_chars` | 80 | characters | budget | Tree-label summary only; expand containers or double-click to inspect the complete key/value. Source retains all content. |
| `json.array_summary_chars` | 120 | characters | budget | Tree-label summary only; expand containers or double-click to inspect the complete key/value. Source retains all content. |
| `json.auto_expand_children` | 16 | items | budget | Initial auto-expansion policy only; collapsed children can still be expanded explicitly. |

## csv

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `csv.input_bytes` | None | bytes | unlimited | Prepare complete rows and columns when admitted memory permits. Under pressure, publish complete row prefixes with continuation instead of a fixed row/column quota. |
| `csv.rows` | None | items | unlimited | Prepare complete rows and columns when admitted memory permits. Under pressure, publish complete row prefixes with continuation instead of a fixed row/column quota. |
| `csv.columns` | None | items | unlimited | Prepare complete rows and columns when admitted memory permits. Under pressure, publish complete row prefixes with continuation instead of a fixed row/column quota. |
| `csv.cells` | None | items | unlimited | Prepare complete rows and columns when admitted memory permits. Under pressure, publish complete row prefixes with continuation instead of a fixed row/column quota. |
| `csv.cell_chars` | 4,096 | characters | budget | Visible single-line cell excerpt; double-click retains the complete value and uses paged viewing for large values. |

## spreadsheet

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `spreadsheet.sheets` | None | items | unlimited | XLSX cells, sheet names and saved formulas stream into a disk-backed index with paged values; there is no input/sheet/row/column/value/HTML total gate. Legacy XLS and oversized individual XML records use native Quick Look without an application size cap. |
| `spreadsheet.rows` | None | items | unlimited | XLSX cells, sheet names and saved formulas stream into a disk-backed index with paged values; there is no input/sheet/row/column/value/HTML total gate. Legacy XLS and oversized individual XML records use native Quick Look without an application size cap. |
| `spreadsheet.columns` | None | items | unlimited | XLSX cells, sheet names and saved formulas stream into a disk-backed index with paged values; there is no input/sheet/row/column/value/HTML total gate. Legacy XLS and oversized individual XML records use native Quick Look without an application size cap. |
| `spreadsheet.cell_chars` | None | characters | unlimited | XLSX cells, sheet names and saved formulas stream into a disk-backed index with paged values; there is no input/sheet/row/column/value/HTML total gate. Legacy XLS and oversized individual XML records use native Quick Look without an application size cap. |
| `spreadsheet.input_bytes` | None | bytes | unlimited | XLSX cells, sheet names and saved formulas stream into a disk-backed index with paged values; there is no input/sheet/row/column/value/HTML total gate. Legacy XLS and oversized individual XML records use native Quick Look without an application size cap. |
| `spreadsheet.sheet_text_bytes` | None | bytes | unlimited | XLSX cells, sheet names and saved formulas stream into a disk-backed index with paged values; there is no input/sheet/row/column/value/HTML total gate. Legacy XLS and oversized individual XML records use native Quick Look without an application size cap. |
| `spreadsheet.html_bytes` | None | bytes | unlimited | XLSX cells, sheet names and saved formulas stream into a disk-backed index with paged values; there is no input/sheet/row/column/value/HTML total gate. Legacy XLS and oversized individual XML records use native Quick Look without an application size cap. |
| `spreadsheet.column_window` | None | columns | unlimited | Load the complete column or sheet-catalog range; worksheet rows retain query windows and disk-backed indexing, and projected cells require memory admission. |
| `spreadsheet.catalog_window` | None | items | unlimited | Load the complete column or sheet-catalog range; worksheet rows retain query windows and disk-backed indexing, and projected cells require memory admission. |

## xmind

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `xmind.title_bytes` | None | bytes | unlimited | Prepare complete map content under memory admission; recursive nesting and format validity remain bounded separately. |
| `xmind.nesting` | 64 | levels | fallback | Mind-map working budget. Exceeding it, including parser stack exhaustion, opens complete content.json/content.xml pages with cancellable on-demand decompression. |
| `xmind.content_bytes` | None | bytes | unlimited | Prepare complete map content under memory admission; recursive nesting and format validity remain bounded separately. |
| `xmind.topics` | None | items | unlimited | Prepare complete map content under memory admission; recursive nesting and format validity remain bounded separately. |
| `xmind.input_bytes` | None | bytes | unlimited | Open the ZIP container from disk without a total file-size or entry-count gate. |
| `xmind.archive_entries` | None | items | unlimited | Open the ZIP container from disk without a total file-size or entry-count gate. |
| `xmind.sheets` | None | items | unlimited | Prepare complete map content under memory admission; recursive nesting and format validity remain bounded separately. |
| `xmind.relationships` | None | items | unlimited | Prepare complete map content under memory admission; recursive nesting and format validity remain bounded separately. |
| `xmind.note_bytes` | None | bytes | unlimited | Prepare complete map content under memory admission; recursive nesting and format validity remain bounded separately. |
| `xmind.labels` | None | items | unlimited | Prepare complete map content under memory admission; recursive nesting and format validity remain bounded separately. |
| `xmind.xml_nodes` | None | items | unlimited | Prepare complete map content under memory admission; recursive nesting and format validity remain bounded separately. |
| `xmind.xml_nesting` | 256 | levels | fallback | Mind-map working budget. Exceeding it, including parser stack exhaustion, opens complete content.json/content.xml pages with cancellable on-demand decompression. |
| `xmind.link_bytes` | None | bytes | unlimited | Prepare complete map content under memory admission; recursive nesting and format validity remain bounded separately. |
| `xmind.labels_bytes` | None | bytes | unlimited | Prepare complete map content under memory admission; recursive nesting and format validity remain bounded separately. |

## archive

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `archive.entries` | None | items | unlimited | Enumerate all entries and spool the report for bounded page display; no total entry-count gate. |
| `archive.bytes` | 32 MiB | bytes | budget | Structured XML projection working buffer; larger content streams in full into the paged source report. |

## numpy

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `numpy.header_bytes` | 64 KiB | bytes | budget | Header string-token working buffer. Scan fields across the complete streamed header; preserve all header bytes and all supported numeric values in the paged report. |
| `numpy.values` | None | items | unlimited | Supported numeric values stream to a disk-backed paged report; no prefix-only value count. |

## geometry

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `geometry.input_bytes` | None | bytes | unlimited | Read complete geometry under memory admission; raster output and format-validity budgets still apply, with complete source fallback. |
| `geometry.vertices` | None | items | unlimited | Read complete geometry under memory admission; raster output and format-validity budgets still apply, with complete source fallback. |
| `geometry.faces` | None | items | unlimited | Read complete geometry under memory admission; raster output and format-validity budgets still apply, with complete source fallback. |
| `geometry.polygon_vertices` | None | items | unlimited | Read complete geometry under memory admission; raster output and format-validity budgets still apply, with complete source fallback. |

## bdf

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `bdf.glyph_edge` | 64 | pixels | fallback | Bitmap-font projection working budget; beyond it, complete BDF source pages remain available instead of a silently shortened glyph list. |
| `bdf.glyphs` | 64 | items | fallback | Bitmap-font projection working budget; beyond it, complete BDF source pages remain available instead of a silently shortened glyph list. |

## executable

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `executable.input_bytes` | None | bytes | unlimited | No file/name/count/report total gate. Read metadata through the io.cache_bytes working cache and spool reports; cache exhaustion opens complete file-byte pages. CString scans do not use the library prefix cutoff. |
| `executable.name_chars` | None | characters | unlimited | No file/name/count/report total gate. Read metadata through the io.cache_bytes working cache and spool reports; cache exhaustion opens complete file-byte pages. CString scans do not use the library prefix cutoff. |
| `executable.architectures` | None | items | unlimited | No file/name/count/report total gate. Read metadata through the io.cache_bytes working cache and spool reports; cache exhaustion opens complete file-byte pages. CString scans do not use the library prefix cutoff. |
| `executable.hex_bytes` | 4 KiB | bytes | budget | Hex excerpt within the metadata report; complete file bytes remain accessible through raw paging. |
| `executable.segments` | None | items | unlimited | No file/name/count/report total gate. Read metadata through the io.cache_bytes working cache and spool reports; cache exhaustion opens complete file-byte pages. CString scans do not use the library prefix cutoff. |
| `executable.sections` | None | items | unlimited | No file/name/count/report total gate. Read metadata through the io.cache_bytes working cache and spool reports; cache exhaustion opens complete file-byte pages. CString scans do not use the library prefix cutoff. |
| `executable.report_bytes` | None | bytes | unlimited | No file/name/count/report total gate. Read metadata through the io.cache_bytes working cache and spool reports; cache exhaustion opens complete file-byte pages. CString scans do not use the library prefix cutoff. |
| `executable.libraries` | None | items | unlimited | No file/name/count/report total gate. Read metadata through the io.cache_bytes working cache and spool reports; cache exhaustion opens complete file-byte pages. CString scans do not use the library prefix cutoff. |
| `executable.dynamic_entries` | None | items | unlimited | No file/name/count/report total gate. Read metadata through the io.cache_bytes working cache and spool reports; cache exhaustion opens complete file-byte pages. CString scans do not use the library prefix cutoff. |
| `executable.name_scan_bytes` | None | bytes | unlimited | No file/name/count/report total gate. Read metadata through the io.cache_bytes working cache and spool reports; cache exhaustion opens complete file-byte pages. CString scans do not use the library prefix cutoff. |

## tensor

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `tensor.header_bytes` | None | bytes | unlimited | Stream metadata records and dtype counts to disk. Oversized individual records use complete header pages without loading tensor weights. No header/name/shape/count/report total gate. |
| `tensor.rows` | None | items | unlimited | Stream metadata records and dtype counts to disk. Oversized individual records use complete header pages without loading tensor weights. No header/name/shape/count/report total gate. |
| `tensor.report_bytes` | None | bytes | unlimited | Stream metadata records and dtype counts to disk. Oversized individual records use complete header pages without loading tensor weights. No header/name/shape/count/report total gate. |
| `tensor.shape_dimensions` | None | items | unlimited | Stream metadata records and dtype counts to disk. Oversized individual records use complete header pages without loading tensor weights. No header/name/shape/count/report total gate. |
| `tensor.metadata_entries` | None | items | unlimited | Stream metadata records and dtype counts to disk. Oversized individual records use complete header pages without loading tensor weights. No header/name/shape/count/report total gate. |
| `tensor.name_chars` | None | characters | unlimited | Stream metadata records and dtype counts to disk. Oversized individual records use complete header pages without loading tensor weights. No header/name/shape/count/report total gate. |
| `tensor.dtype_chars` | None | characters | unlimited | Stream metadata records and dtype counts to disk. Oversized individual records use complete header pages without loading tensor weights. No header/name/shape/count/report total gate. |
| `tensor.metadata_key_chars` | None | characters | unlimited | Stream metadata records and dtype counts to disk. Oversized individual records use complete header pages without loading tensor weights. No header/name/shape/count/report total gate. |
| `tensor.metadata_value_chars` | None | characters | unlimited | Stream metadata records and dtype counts to disk. Oversized individual records use complete header pages without loading tensor weights. No header/name/shape/count/report total gate. |

## sqlite

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `sqlite.snapshot_bytes` | None | bytes | unlimited | Copy the snapshot in fixed-size blocks, without an application database total-size gate. |
| `sqlite.page_rows` | 500 | rows | page | Rows buffered per table/schema page, independent of total table size; a persistent pager provides row counts, page jumps and first/last navigation. |
| `sqlite.tables` | None | items | unlimited | List all tables and views without an application catalog-size cap. |
| `sqlite.columns` | None | columns | unlimited | All SQLite columns are accessible; horizontal virtualization bounds the working set. |
| `sqlite.value_bytes` | None | bytes | unlimited | Use native SQLite format constraints; full values and definitions are read in text.page_bytes windows with previous/next navigation. |
| `sqlite.text_bytes` | 512 | bytes | budget | Visible cell summary only; double-click opens a complete paged value, with binary values shown as hex. |
| `sqlite.schema_text_bytes` | None | bytes | unlimited | Use native SQLite format constraints; full values and definitions are read in text.page_bytes windows with previous/next navigation. |
| `sqlite.blob_bytes` | 16 | bytes | budget | Visible cell summary only; double-click opens a complete paged value, with binary values shown as hex. |
| `sqlite.query_seconds` | 5 | seconds | timeout | Initial cancellable query time allowance; Retry with more time explicitly extends it. |
| `sqlite.busy_milliseconds` | 1,000 | milliseconds | timeout | Native SQLite lock-wait allowance; a busy database can be retried and is not rejected by file size. |
| `sqlite.cache_kib` | 8,192 | KiB | budget | SQLite page-cache working memory; larger databases read further pages from disk. |
| `sqlite.column_window` | 32 | columns | page | Columns fetched per horizontal viewport window, without a total column limit. |

## markup

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `markup.eager_layout_bytes` | None | bytes | unlimited | Measure complete Markdown/markup layout without a byte or line-count switch to estimated heights; images and diagrams retain demand-driven decoding. |
| `markup.eager_layout_lines` | None | source lines | unlimited | Measure complete Markdown/markup layout without a byte or line-count switch to estimated heights; images and diagrams retain demand-driven decoding. |
| `markup.scan_bytes` | None | bytes | unlimited | Scan the current text for image references; large documents use source pages instead of a total reference-scan cutoff. |

## remote_image

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `remote_image.input_bytes` | None | bytes | unlimited | HTTP downloads stream to a temporary file and prepare bounded thumbnails; source input size is not an application rejection gate. |
| `remote_image.timeout_seconds` | 20 | seconds | timeout | Idle/no-progress timeout; active downloads have no fixed total elapsed-time ceiling. |
| `remote_image.first_attempt_seconds` | 8 | seconds | timeout | Connection establishment allowance, including the first GitHub raw URL attempt before API fallback. |
| `remote_image.svg_embedded_bytes` | 64 MiB | bytes | budget | Per-embedded-image working memory in SVG preparation; original SVG content remains available through Source or file-byte access. |
| `remote_image.redirects` | 5 | items | budget | Redirect-chain budget prevents request loops; it does not constrain image-file size. |
| `remote_image.connect_seconds` | 8 | seconds | timeout | Connection establishment allowance, including the first GitHub raw URL attempt before API fallback. |
| `remote_image.minimum_bytes_per_second` | 1 | bytes/second | timeout | Minimum progress for the idle timeout, not a bandwidth ceiling. |

## diagram

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `diagram.disk_cache_bytes` | 512 MiB | bytes | budget | Persistent diagram result cache capacity. Evict least recently used results; misses, invalid entries and unavailable storage regenerate diagrams through the normal background path. |
| `diagram.blocks` | 8 | items | budget | Maximum diagrams per background render batch. Visible blocks queue automatically; this does not limit total document formulas or require manual paging. |
| `diagram.source_bytes` | 256 KiB | bytes | fallback | Per-diagram structured-render working budget; failures preserve the complete block source. Other blocks remain available on demand. |
| `diagram.svg_bytes` | 8 MiB | bytes | fallback | Per-diagram structured-render working budget; failures preserve the complete block source. Other blocks remain available on demand. |
| `diagram.aggregate_encoded_bytes` | 64 MiB | bytes | budget | Encoded/decoded working budget for the viewport diagram cache and active render batch. Older resources are evicted and can be requested again; total document blocks remain unrestricted. |
| `diagram.aggregate_decoded_bytes` | 128 MiB | bytes | budget | Encoded/decoded working budget for the viewport diagram cache and active render batch. Older resources are evicted and can be requested again; total document blocks remain unrestricted. |
| `diagram.preview_edge` | 2,400 | pixels | budget | Active raster resolution/scale budget. Page raster dimensions also adapt to aggregate working memory, independently of document length. |
| `diagram.startup_seconds` | 15 | seconds | timeout | Cancellable renderer startup/per-block time allowance; an unfinished block shows its complete source instead of rejecting the Markdown document. |
| `diagram.block_seconds` | 8 | seconds | timeout | Cancellable renderer startup/per-block time allowance; an unfinished block shows its complete source instead of rejecting the Markdown document. |
| `diagram.ipc_bytes` | 128 MiB | bytes | budget | One page/request or one block IPC working buffer; document batches are paged and oversized render results preserve the complete source block. |
| `diagram.slot_bytes` | 16 MiB | bytes | budget | One page/request or one block IPC working buffer; document batches are paged and oversized render results preserve the complete source block. |
| `diagram.max_upscale` | 2 | scale | budget | Active raster resolution/scale budget. Page raster dimensions also adapt to aggregate working memory, independently of document length. |
| `diagram.request_bytes` | 64 MiB | bytes | budget | One page/request or one block IPC working buffer; document batches are paged and oversized render results preserve the complete source block. |
| `diagram.message_bytes` | 16388 KiB | bytes | budget | One page/request or one block IPC working buffer; document batches are paged and oversized render results preserve the complete source block. |
| `diagram.mermaid_edges` | 1,000 | edges | fallback | Per-diagram structured-render working budget; failures preserve the complete block source. Other blocks remain available on demand. |
| `diagram.math_macros` | 1,000 | expansions | budget | Math interpreter expansion budget to bound recursive work; the complete formula source remains available when rendering cannot finish. |

## book

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `book.chapter_bytes` | None | bytes | unlimited | Read complete selected chapter/text/XML when memory permits; chapter navigation, nesting, embedded-image and output budgets remain separate. |
| `book.xml_nodes` | None | nodes | unlimited | Read complete selected chapter/text/XML when memory permits; chapter navigation, nesting, embedded-image and output budgets remain separate. |
| `book.resource_bytes` | None | bytes | unlimited | Requested archive images stream to a private file and use native thumbnail decoding; no source-resource byte or pixel-edge gate. Only the active chapter/page previews are retained. |
| `book.preview_edge` | 2,400 | pixels | budget | Thumbnail resolution for the requested illustration/comic page; this does not reject the original image. |
| `book.input_bytes` | None | bytes | unlimited | No application book/archive/chapter/catalog/metadata total gate. Structured parsing keeps a working budget; oversized metadata exposes every archive member as complete source or byte pages. |
| `book.text_bytes` | None | bytes | unlimited | Read complete selected chapter/text/XML when memory permits; chapter navigation, nesting, embedded-image and output budgets remain separate. |
| `book.archive_entries` | None | items | unlimited | No application book/archive/chapter/catalog/metadata total gate. Structured parsing keeps a working budget; oversized metadata exposes every archive member as complete source or byte pages. |
| `book.chapters` | None | items | unlimited | No application book/archive/chapter/catalog/metadata total gate. Structured parsing keeps a working budget; oversized metadata exposes every archive member as complete source or byte pages. |
| `book.metadata_bytes` | None | bytes | unlimited | No application book/archive/chapter/catalog/metadata total gate. Structured parsing keeps a working budget; oversized metadata exposes every archive member as complete source or byte pages. |
| `book.toc_entries` | None | items | unlimited | No application book/archive/chapter/catalog/metadata total gate. Structured parsing keeps a working budget; oversized metadata exposes every archive member as complete source or byte pages. |
| `book.toc_depth` | 32 | levels | budget | Visual TOC indentation only; retain all nested entries and their actual hierarchy depth instead of cutting the catalog. |
| `book.fb2_depth` | 64 | levels | fallback | Structured chapter working budget. When exceeded, retain chapter navigation and provide complete UTF-8 source pages, with streamed transcoding for supported legacy encodings. No chapter is silently cut off. |
| `book.html_depth` | 128 | levels | fallback | Structured chapter working budget. When exceeded, retain chapter navigation and provide complete UTF-8 source pages, with streamed transcoding for supported legacy encodings. No chapter is silently cut off. |
| `book.chapter_decoded_bytes` | 128 MiB | bytes | fallback | Structured chapter working budget. When exceeded, retain chapter navigation and provide complete UTF-8 source pages, with streamed transcoding for supported legacy encodings. No chapter is silently cut off. |
| `book.chapter_images` | None | items | unlimited | Read complete selected chapter/text/XML when memory permits; chapter navigation, nesting, embedded-image and output budgets remain separate. |
| `book.image_decoded_bytes` | None | bytes | unlimited | Requested archive images stream to a private file and use native thumbnail decoding; no source-resource byte or pixel-edge gate. Only the active chapter/page previews are retained. |
| `book.image_edge` | None | pixels | unlimited | Requested archive images stream to a private file and use native thumbnail decoding; no source-resource byte or pixel-edge gate. Only the active chapter/page previews are retained. |

## book_state

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `book_state.bookmarks` | None | items | unlimited | Persist all bookmarks, labels and reading-state entries; no application bookmark-count or input-size gate. |
| `book_state.label_bytes` | None | bytes | unlimited | Persist all bookmarks, labels and reading-state entries; no application bookmark-count or input-size gate. |
| `book_state.input_bytes` | None | bytes | unlimited | Persist all bookmarks, labels and reading-state entries; no application bookmark-count or input-size gate. |
| `book_state.label_chars` | 120 | characters | budget | Derived bookmark/header/catalog label excerpt; chapter targets and full-metadata/text dialogs retain the complete content. |

## geo

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `geo.points` | None | items | unlimited | Prepare all coordinate rows when memory permits; pressure can publish complete prefixes with continuation. |

## workspace

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `workspace.content_buffer_bytes` | 64 KiB | bytes | budget | Streaming content-search read buffer; files and lines have no total size cap. |
| `workspace.content_snippet_bytes` | 320 | bytes per match | budget | Retained match-preview bytes; search still scans the complete line and file. |
| `workspace.search_debounce_ms` | 150 | milliseconds | budget | Debounce for query edits only; explicit search opening, mode changes and paging execute immediately. |
| `workspace.search_results` | 60 | items | page | Visible page/batch size; later search results or saved workspaces remain accessible through continuation controls. |
| `workspace.search_entries` | None | items | unlimited | Search traverses all eligible entries with cancellable result pages; there is no entry-scan total cutoff. |
| `workspace.recent_workspaces` | 30 | items | page | Visible page/batch size; later search results or saved workspaces remain accessible through continuation controls. |

## update

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `update.session_bytes` | None | bytes | unlimited | Stream session JSON and preserve all actual windows, roots and tabs while validating identities and active indexes. |
| `update.workspaces` | None | items | unlimited | Stream session JSON and preserve all actual windows, roots and tabs while validating identities and active indexes. |
| `update.roots_per_workspace` | None | items | unlimited | Stream session JSON and preserve all actual windows, roots and tabs while validating identities and active indexes. |
| `update.tabs_per_workspace` | None | items | unlimited | Stream session JSON and preserve all actual windows, roots and tabs while validating identities and active indexes. |

## audio_artwork

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `audio_artwork.input_bytes` | None | bytes | unlimited | Native metadata artwork is thumbnailed without an application byte gate. Open full artwork exposes its complete original snapshot. |
| `audio_artwork.preview_edge` | 600 | pixels | budget | Active cover thumbnail resolution; Open full artwork retains full-resolution source access. |

## io

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `io.buffer_bytes` | 64 KiB | bytes | budget | Fixed working buffer for cancellable streaming reads, copies and report generation, independent of total source size. |
| `io.cache_bytes` | 64 MiB | bytes | budget | Working cache/projection allowance for decoded text, executable metadata and vector/font projection buffers. Old text ranges regenerate; oversized representations retain complete source/byte pages. |
| `io.record_bytes` | 64 MiB | bytes | budget | One active XML/structured metadata record or value, independent of total file size; oversized records retain complete Source/header pages or native preview access. |

## svg

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `svg.event_bytes` | 64 MiB | bytes | budget | One active XML markup/text record in the sanitizer, not a document total gate; native File bytes retains the complete source when preparation exceeds this budget. |

## word

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `word.zoom_min_percent` | 25 | percent | budget | Minimum Word reading-view presentation zoom; does not constrain source content. |
| `word.zoom_max_percent` | 400 | percent | budget | Maximum Word reading-view presentation zoom; does not constrain source content. |
| `word.zoom_step_percent` | 25 | percent | budget | Zoom-in/out button increment in Word reading view; reset restores 100 percent. |

## drawio

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `drawio.xml_bytes` | None | bytes | unlimited | Read complete source and decoded page XML under memory admission; raster-output and nesting budgets remain separate. |
| `drawio.nesting` | 64 | levels | budget | Bound parent traversal and detect cyclic groups. |
| `drawio.preview_bytes` | 128 MiB | bytes | budget | Aggregate encoded PNG page memory; larger documents fall back to source pages. |

## opml

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `opml.xml_bytes` | None | bytes | unlimited | Prepare the complete outline under parser memory admission; nesting and format validity remain separate. |
| `opml.nodes` | None | items | unlimited | Prepare the complete outline under parser memory admission; nesting and format validity remain separate. |
| `opml.nesting` | 256 | levels | budget | Maximum outline nesting depth. |

## watch

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `watch.pending_paths` | 4,096 | paths | fallback | Maximum distinct pending filesystem paths. Overflow triggers a full source check instead of dropping changes; document access is unlimited. |

## jobs

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `jobs.content_search_concurrency` | 1 | jobs | budget | On-demand content search concurrency, independent of filename indexing and document opening. |
| `jobs.document_concurrency` | 2 | jobs | budget | Process-wide concurrent document loading and refresh jobs. Additional jobs wait asynchronously and can be canceled; file access is unlimited. |
| `jobs.projection_concurrency` | 2 | jobs | budget | Process-wide concurrent data projection and page preparation jobs. Additional jobs wait asynchronously and can be canceled; file access is unlimited. |
| `jobs.decode_concurrency` | 2 | jobs | budget | Process-wide concurrent image and animation decoding jobs. Additional jobs wait asynchronously and can be canceled; file access is unlimited. |
| `jobs.native_concurrency` | 2 | jobs | budget | Process-wide concurrent native document preparation jobs. Additional jobs wait asynchronously and can be canceled; file access is unlimited. |
| `jobs.diagram_concurrency` | 1 | jobs | budget | Process-wide diagram batch preparation concurrency. Waiting batches yield workers and retain their cancellation flag. |

## resources

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `resources.cache_bytes` | 256 MiB | bytes | budget | Process-wide budget for tracked reclaimable reader caches; evict background then least recently used unpinned values. Live state and active work are accounted separately and never discarded as cache. Framework allocations are outside this ledger. |
| `resources.working_bytes` | None | bytes | unlimited | Working allocations use live macOS memory headroom instead of a fixed byte budget. |
| `resources.system_used_percent` | 95 | percent | budget | Emergency system memory use ceiling; reserve the remaining physical memory for macOS and other apps. |
| `resources.memory_poll_ms` | 250 | milliseconds | budget | Background sampling interval for system memory admission. |

## remote

| Key | Value | Unit | Effect | Behavior |
| --- | ---: | --- | --- | --- |
| `remote.connection_check_interval_ms` | 30,000 | milliseconds | budget | Background SFTP health-check interval after a successful check; all saved connections are attempted at application startup, even without an active workspace. |
| `remote.connection_retry_ms` | 5,000 | milliseconds | budget | Initial quiet connection retry delay; doubles after each failure up to connection_retry_max_ms. Background attempts do not show authentication dialogs. |
| `remote.connection_retry_max_ms` | 60,000 | milliseconds | budget | Maximum delay between failed background connection attempts; retries continue while the connection remains saved and the application is running. |
| `remote.connection_concurrency` | 2 | connections | budget | Concurrent background connection checks or reconnects; additional saved connections queue without blocking the UI. |
| `remote.search_retry_initial_ms` | 10,000 | milliseconds | budget | Initial quiet retry delay for failed index connections or directories; doubles up to search_refresh_ms. |
| `remote.search_concurrency` | 2 | jobs | budget | Concurrent remote path-index jobs, independent of content searches and file opening; queued work waits without occupying workers. |
| `remote.search_batch_paths` | 256 | paths per transaction | budget | Paths committed per search-index transaction; partial results publish sooner on the first file or elapsed publish interval. Total indexed paths are unlimited. |
| `remote.search_poll_ms` | 250 | milliseconds | budget | Search-index publication and UI revision-poll interval; unchanged revisions do not rerun the filename query. |
| `remote.search_directory_batch` | 64 | directories per cycle | budget | Maximum directory metadata checks per incremental search-index job; changed listings stream through disk-backed staging. Total directory count is unlimited. |
| `remote.search_directory_poll_ms` | 2,000 | milliseconds | budget | Delay between background incremental directory batches for each retained remote workspace, independent of the search dialog. |
| `remote.search_refresh_ms` | 300,000 | milliseconds | budget | Age after which a directory listing is reconciled even when its coarse modification timestamp is unchanged. Also caps exponential retries; cached paths do not expire. Explicit retry wakes failed directories without rebuilding the full index. |
| `remote.search_record_bytes` | 1 MiB | bytes | fallback | Active NUL-framed SSH filename record budget. Invalid or oversized command output falls back to streaming SFTP enumeration; no total file or workspace-size limit. |
| `remote.refresh_interval_ms` | 2,000 | milliseconds | budget | Delay between automatic refresh cycles for the visible remote workspace; suspended while the app is inactive or the transport is busy/disconnected. |
| `remote.refresh_batch` | 4 | paths per cycle | budget | Round-robin directory and background-file checks per refresh cycle; the active file is checked first. Limits concurrent working scope, not total workspace size. |
| `remote.idle_timeout_ms` | 30,000 | milliseconds | timeout | Close a pending SFTP request after this interval without transmitted or received bytes. Progress resets the timer; total file size and total transfer time are unlimited. |
| `remote.packet_bytes` | 1 MiB | bytes | budget | Maximum active SFTP protocol packet; files are transferred as successive packets without a total-size limit. |
| `remote.transfer_bytes` | 32 KiB | bytes | budget | Bytes requested per SFTP read, independent of the total remote file size. |
| `remote.diagnostic_bytes` | 16 KiB | bytes | budget | Tail of SSH diagnostics retained for connection failures; earlier output is discarded. |
