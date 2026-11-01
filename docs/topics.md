# Find your use for Glim

**English** · [简体中文](topics.zh-CN.md) · [Documentation](README.md)

Glim helps you view mixed local files on a Mac and make basic text edits. Choose a topic for the task you have. A PDF, image or dataset can belong to several topics; all files still open in the same workspace.

[Office files](#office) · [Reading and study](#reading) · [Images and design assets](#images) · [Audio, video and subtitles](#media) · [Notes and mind maps](#notes) · [Tables and structured data](#data) · [Database inspection](#database) · [Developer workspace](#developer) · [2D graphics and 3D models](#graphics) · [GIS and track data](#gis) · [AI and research files](#research) · [Archives and handoffs](#archives)

<a id="office"></a>

## Office files

For people who receive contracts, reports, spreadsheets and presentations. Open the folder for a task and switch between its documents in tabs.

Page through, zoom and select text in PDFs, or show them fullscreen. Word offers Reading mode and Original layout. Excel offers worksheets and paged previews. PowerPoint and Keynote can open a system fullscreen presentation. Supported documents also offer preview-text copying and image-text recognition.

Typical files: PDF, DOC/DOCX, RTF, XLS/XLSX/XLSM, PPT/PPTX, Pages, Numbers and Keynote.

Current limits: Office previews are read-only. Complex layout, charts and embedded objects may differ; Excel does not recalculate formulas and some previews omit drawings. Some document and presentation behavior depends on the system preview provider. ODT/ODS/ODP offer container listings and selected text only.

<a id="reading"></a>

## Reading and study

For students, researchers and anyone reading books or reference material on a Mac. Keep a book, course PDFs and your notes together.

Navigate ebook chapters through the contents, adjust type size, line spacing and paper, and save bookmarks and reading position. View comics by page. PDFs have their own document reader. Use Markdown or plain text for notes.

Typical files: EPUB, FB2/FB2 ZIP, CBZ, PDF, TXT and Markdown.

Current limits: Ebooks do not fully reproduce publisher CSS or fixed layouts. DRM EPUB and MOBI/AZW are unsupported. Ebook progress is managed separately from general document scroll positions.

<a id="images"></a>

## Images and design assets

For people checking photos, design deliverables, screenshots and fonts. Open an asset folder and inspect its files.

View common raster images, animated GIFs, multipage TIFFs and SVGs. Copy images and extract text from supported raster previews. Inspect font glyphs and enter sample text without installing the font.

Typical files: PNG, JPEG, WebP, GIF, TIFF, SVG, EXR/HDR, QOI, and fonts such as TTF/OTF/TTC.

Current limits: Previews are read-only. EXR/HDR convert to SDR. Large images may use scaled decoding; Actual size does not guarantee original-resolution decoding. OCR can make mistakes. SVG previews have no image OCR and do not load external file resources. BDF has a basic glyph preview.

<a id="media"></a>

## Audio, video and subtitles

For people reviewing recordings, course videos, footage and subtitles, including creators organizing local material.

Play audio and video in tabs, with ordered or shuffled folder playlists. Read subtitles by timestamp and edit their source.

Typical files: MP3, MP4, MOV, M4A, WAV, FLAC and other system-supported media; selected MKV files; SRT and VTT.

Current limits: Decoding depends on macOS. MKV uses an H.264/AAC-LC remuxing path; other MKV codecs and embedded subtitles are unsupported. Separate subtitles do not sync with playback. Playlists enumerate direct children of the folder and resume paused.

<a id="notes"></a>

## Notes and mind maps

For people writing explanations, course notes, requirements and proposals, with related mind maps and diagrams.

Markdown supports tables, formulas, diagrams and task lists; tasks in ordinary documents can be checked and saved. XMind supports sheets, topic folding, zoom and details. Browse OPML as an expandable outline.

Typical files: Markdown, TXT, XMind, OPML, D2, Pikchr, Vega diagrams, and basic AsciiDoc/Org/RST previews.

Current limits: XMind is read-only, with a consistent layout rather than original positions, images or attachments. Diagram syntax has compatibility limits and does not load external imports or resources. Ordinary text edits save automatically to the original file.

<a id="data"></a>

## Tables and structured data

For operations staff, analysts, product managers and developers checking exports, API responses and configuration.

Read CSV/TSV as tables and inspect full cell values. Expand JSON-family files as trees. Supported JSON, YAML, CSV/TSV and XML can also appear as interactive graphs with expandable nodes.

Typical files: CSV/TSV, JSON/JSONC/JSON5/JSONL, YAML and XML; see Office for spreadsheets.

Current limits: Tables, trees and graphs are read-only; use editable source to change text. Graphs are an alternative view, with source fallback for complex data. Data analysis, schema generation and image export are not included.

<a id="database"></a>

## Database inspection

For developers, analysts and support staff inspecting local application databases or investigating a record.

Open the main SQLite file, choose a table or view, inspect schema and SQL definitions, search fields across records, sort and page, and double-click full values.

Typical files: SQLite, SQLite3, DB, DB3 and files recognized by a SQLite header.

Current limits: Inspection uses a separate read-only snapshot, including associated WAL/journal data through the main database. It does not write the source or run arbitrary write SQL. Encrypted databases and remote MySQL/PostgreSQL connections are unsupported.

<a id="developer"></a>

## Developer workspace

For developers reading projects, investigating logs, checking configuration and writing technical notes.

Source highlighting, folding, search and a minimap, with basic UTF-8 editing. Preview HTML and its source, and read Markdown formulas and architecture diagrams. Inspect basic executable metadata.

Typical files: Common programming languages, HTML/CSS/JS, shell, configuration, logs, Markdown and Mach-O/ELF/PE.

Current limits: No source compilation, debugging, execution, LSP or plugins. Executables are inspected read-only without running or decompiling. HTML previews execute page JavaScript and may access the network; check the file source.

<a id="graphics"></a>

## 2D graphics and 3D models

For people checking vector assets, technical diagrams and 3D meshes. 2D viewing and 3D previews have different capabilities.

View SVGs, basic draw.io diagrams, flowcharts and architecture diagrams. OBJ/STL offer static mesh projections.

Typical files: SVG, draw.io, Markdown diagrams, D2, Pikchr, OBJ and STL.

Current limits: 3D currently has no interactive rotation, materials or textures. Custom draw.io shapes, embedded images and exact routing may be simplified or omitted. DWG/DXF and STEP engineering-format support is not claimed.

<a id="gis"></a>

## GIS and track data

For map developers, geographic-data users and people checking track files.

Expand GeoJSON features and properties. Read GPX/KML coordinates as tables with XML source available. Check the fields and coordinates a file contains.

Typical files: GeoJSON, GPX and KML.

Current limits: No basemap, track-map rendering, spatial analysis or coordinate conversion. GeoJSON uses a data tree; GPX/KML use coordinate tables. No specialist Shapefile or GeoTIFF GIS capabilities are claimed.

<a id="research"></a>

## AI and research files

For AI developers and researchers checking model files, numeric data or notebook contents.

safetensors shows tensor names, shapes, dtypes, parameter counts and metadata. NPY shows headers and values for supported types. Read notebooks as JSON structures.

Typical files: safetensors, NPY and notebooks, alongside CSV, PDF and research notes.

Current limits: No model, notebook or pickle execution. NPY value support covers selected types; other types retain metadata. GGUF, ONNX and NPZ readers remain unfinished.

<a id="archives"></a>

## Archives and handoffs

For people checking project handoffs, backups and compressed logs.

Browse ZIP entries and sizes, or extract to a separate new directory and open it as a new workspace. View TAR entries. Read GZ/BZ2/XZ as on-demand decompressed source pages.

Typical files: ZIP, TAR, GZ, BZ2 and XZ.

Current limits: ZIP extraction creates new files, keeps the original archive and does not overwrite an existing directory; it can be cancelled. TAR currently offers a listing only. Compressed-text reading has different capabilities from general archive management.

## Start with a folder you already use

Office material, course notes, design deliverables or a code project can become a workspace. Use the folder tree to find files and tabs to keep them open. You do not need to tag files by topic. See the [user guide](user-guide.md) for steps and [format compatibility](file-format-support.md) for individual files.
