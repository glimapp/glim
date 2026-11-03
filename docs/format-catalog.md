# Format capability catalog

**English** · [简体中文](format-catalog.zh-CN.md)

Generated from `assets/format-catalog.json`. Declarations cover candidate routing, text projections, highlighting, associations and icons; they do not guarantee valid content or available system codecs. Editing requires successful UTF-8 text/editable-page routing; preview support does not grant editing rights. Unknown UTF-8 files retain the ordinary text path.

Run `python3 scripts/generate-format-catalog.py --write` to update generated files and `--check` to verify consistency. Content probes and parser rules remain in services. Routes below follow registration order; a failed probe continues to later candidates. Binary plist requires its header; native preview requires an actual file/package and `.key` additionally requires ZIP/package recognition. Executable and SQLite header probes can recognize unlisted suffixes.

Filename highlighting rules (Dockerfile, `.env`, dot-config names) remain in `language_for_path`. Icons have separate filename/path precedence; this table describes extension icons for ordinary `sample.<suffix>` names. Compound-suffix or neutral fallbacks without a dedicated icon must be explicitly declared.

| Suffix | Candidate providers | Text projections | Highlighting | Association settings | Bundle declaration | Icon (dark / light) |
| --- | --- | --- | --- | --- | --- | --- |
| `7z` | archive | — | plain | Archives | ✓ | zip / zip |
| `a` | archive | — | plain | Archives | ✓ | lib / lib |
| `aac` | native | — | plain | Audio | ✓ | audio / audio |
| `adoc` | text / bytes | ConvertedMarkup | structured | — | ✓ | asciidoc / asciidoc |
| `aif` | native | — | plain | — | — | audio / audio |
| `aiff` | native | — | plain | Audio | ✓ | audio / audio |
| `ar` | archive | — | plain | Archives | ✓ | file / file |
| `astro` | text / bytes | — | astro | — | — | astro / astro |
| `bash` | text / bytes | — | bash | — | — | console / console |
| `bazel` | text / bytes | — | python | — | — | bazel / bazel |
| `bdf` | vector | — | structured | — | ✓ | file / file |
| `bmp` | native, image | — | plain | Images | ✓ | image / image |
| `bz2` | archive, container-report | — | plain | Archives | ✓ | zip / zip |
| `bzl` | text / bytes | — | python | — | — | bazel / bazel |
| `c` | text / bytes | — | c | Source Code | ✓ | c / c |
| `cab` | archive | — | plain | Archives | ✓ | zip / zip |
| `caf` | native | — | plain | Audio | ✓ | audio / audio |
| `cbz` | book | — | plain | Ebooks | ✓ | file / file |
| `cc` | text / bytes | — | cpp | — | — | cpp / cpp |
| `cfg` | text / bytes | — | ini | — | — | settings / settings |
| `cjs` | text / bytes | — | javascript | — | — | javascript / javascript |
| `cmake` | text / bytes | — | cmake | — | — | cmake / cmake |
| `code-workspace` | text / bytes | — | json | — | — | vscode / vscode |
| `command` | text / bytes | — | bash | — | — | command / command |
| `conf` | text / bytes | — | ini | — | — | settings / settings |
| `cpio` | archive | — | plain | Archives | ✓ | zip / zip |
| `cpp` | text / bytes | — | cpp | Source Code | ✓ | cpp / cpp |
| `cs` | text / bytes | — | csharp | — | — | csharp / csharp |
| `css` | text / bytes | — | css | Source Code | ✓ | css / css |
| `csv` | text / bytes | TableCsv, StructuredGraph | plain | — | — | table / table |
| `csx` | text / bytes | — | csharp | — | — | csharp / csharp |
| `cts` | text / bytes | — | typescript | — | — | typescript / typescript |
| `cu` | text / bytes | — | cpp | — | — | cuda / cuda |
| `cuh` | text / bytes | — | cpp | — | — | cuda / cuda |
| `cxx` | text / bytes | — | cpp | — | — | cpp / cpp |
| `d2` | text / bytes | ConvertedMarkup | structured | — | ✓ | file / file |
| `db` | sqlite | — | plain | SQLite | ✓ | database / database |
| `db-shm` | sqlite | — | plain | — | ✓ | database / database |
| `db-wal` | sqlite | — | plain | — | ✓ | database / database |
| `db3` | sqlite | — | plain | SQLite | ✓ | database / database |
| `desktop` | text / bytes | — | ini | — | — | file / file |
| `dfont` | native | — | plain | Fonts | ✓ | font / font |
| `dhall` | text / bytes | — | structured | — | ✓ | dhall / dhall |
| `doc` | native | — | plain | Word | ✓ | word / word |
| `docx` | native | — | plain | Word | ✓ | word / word |
| `drawio` | drawio | — | html | — | — | drawio / drawio |
| `ejs` | text / bytes | — | ejs | — | — | ejs / ejs |
| `env` | text / bytes | — | dotenv | — | — | tune / tune |
| `epub` | book, container-report | — | plain | Ebooks | ✓ | epub / epub |
| `erb` | text / bytes | — | erb | — | — | ruby / ruby |
| `ex` | text / bytes | — | elixir | — | — | elixir / elixir |
| `exr` | native, image | — | plain | — | — | image / image |
| `exs` | text / bytes | — | elixir | — | — | elixir / elixir |
| `fb2` | book | — | plain | Ebooks | ✓ | file / file |
| `fb2.zip` | book, archive, container-report | — | plain | — | — | zip / zip |
| `flac` | native | — | plain | Audio | ✓ | audio / audio |
| `frag` | text / bytes | — | c | — | — | shader / shader |
| `gemspec` | text / bytes | — | ruby | — | — | ruby / ruby |
| `geojson` | text / bytes | — | json | JSON | ✓ | json / json |
| `geom` | text / bytes | — | c | — | — | shader / shader |
| `gif` | image | — | plain | Images | ✓ | image / image |
| `glsl` | text / bytes | — | c | — | — | shader / shader |
| `go` | text / bytes | — | go | Source Code | ✓ | go / go |
| `gpx` | text / bytes | ConvertedMarkup, TableGpx | html | — | ✓ | xml / xml |
| `gql` | text / bytes | — | graphql | — | — | graphql / graphql |
| `graphql` | text / bytes | — | graphql | — | — | graphql / graphql |
| `gz` | archive, container-report | — | plain | Archives | ✓ | zip / zip |
| `h` | text / bytes | — | c | Source Code | ✓ | h / h |
| `hbs` | text / bytes | — | html | — | — | handlebars / handlebars |
| `hcl` | text / bytes | — | structured | — | ✓ | hcl / hcl_light |
| `hdr` | native, image | — | plain | — | — | file / file |
| `hh` | text / bytes | — | cpp | — | — | hpp / hpp |
| `hpp` | text / bytes | — | cpp | Source Code | ✓ | hpp / hpp |
| `htm` | text / bytes | Html | html | HTML | ✓ | html / html |
| `html` | text / bytes | Html | html | HTML | ✓ | html / html |
| `http` | text / bytes | — | structured | — | ✓ | http / http |
| `hxx` | text / bytes | — | cpp | — | — | hpp / hpp |
| `icns` | native, image | — | plain | — | — | image / image |
| `ico` | native, image | — | plain | — | — | image / image |
| `ini` | text / bytes | — | ini | — | — | settings / settings |
| `ipynb` | text / bytes | — | json | — | — | jupyter / jupyter |
| `iso` | archive | — | plain | Archives | ✓ | disc / disc |
| `j2` | text / bytes | — | jinja | — | — | jinja / jinja_light |
| `java` | text / bytes | — | java | Source Code | ✓ | java / java |
| `jinja` | text / bytes | — | jinja | — | — | jinja / jinja_light |
| `jinja2` | text / bytes | — | jinja | — | — | jinja / jinja_light |
| `jpeg` | native, image | — | plain | Images | ✓ | image / image |
| `jpg` | native, image | — | plain | Images | ✓ | image / image |
| `js` | text / bytes | — | javascript | Source Code | ✓ | javascript / javascript |
| `json` | text / bytes | StructuredGraph | json | JSON | ✓ | json / json |
| `json5` | text / bytes | StructuredGraph | json | — | ✓ | json / json |
| `jsonc` | text / bytes | StructuredGraph | json | — | — | json / json |
| `jsonl` | text / bytes | StructuredGraph | json | JSON | ✓ | json / json |
| `jsx` | text / bytes | — | javascript | Source Code | ✓ | react / react |
| `kdl` | text / bytes | — | structured | — | ✓ | file / file |
| `key` | native | — | plain | Keynote | ✓ | key / key |
| `kml` | text / bytes | ConvertedMarkup, TableKml | html | — | ✓ | xml / xml |
| `kt` | text / bytes | — | kotlin | Source Code | ✓ | kotlin / kotlin |
| `kts` | text / bytes | — | kotlin | — | — | kotlin / kotlin |
| `less` | text / bytes | — | css | — | — | less / less |
| `lha` | archive | — | plain | Archives | ✓ | zip / zip |
| `log` | text / bytes | — | plain | Plain Text | ✓ | log / log |
| `lua` | text / bytes | — | lua | — | — | lua / lua |
| `lz` | archive | — | plain | Archives | ✓ | zip / zip |
| `lz4` | archive | — | plain | Archives | ✓ | zip / zip |
| `lzh` | archive | — | plain | Archives | ✓ | zip / zip |
| `lzma` | archive | — | plain | Archives | ✓ | zip / zip |
| `m` | text / bytes | — | structured | — | ✓ | objective-c / objective-c |
| `m4a` | native | — | plain | Audio | ✓ | audio / audio |
| `m4v` | native | — | plain | Video | ✓ | video / video |
| `markdown` | text / bytes | Markdown | markdown | Markdown | ✓ | markdown / markdown |
| `md` | text / bytes | Markdown | markdown | Markdown | ✓ | markdown / markdown |
| `metal` | text / bytes | — | cpp | — | — | file / file |
| `mjs` | text / bytes | — | javascript | — | — | javascript / javascript |
| `mk` | text / bytes | — | make | — | — | makefile / makefile |
| `mkv` | native | — | plain | Video | ✓ | video / video |
| `mov` | native | — | plain | Video | ✓ | video / video |
| `mp3` | native | — | plain | Audio | ✓ | audio / audio |
| `mp4` | native | — | plain | Video | ✓ | video / video |
| `mtlx` | text / bytes | — | html | — | — | xml / xml |
| `mts` | text / bytes | — | typescript | — | — | typescript / typescript |
| `ndjson` | text / bytes | StructuredGraph | json | — | ✓ | json / json |
| `nix` | text / bytes | — | nix | — | — | nix / nix |
| `njk` | text / bytes | — | jinja | — | — | nunjucks / nunjucks |
| `npy` | container-report | — | plain | — | ✓ | file / file |
| `numbers` | native | — | plain | Numbers | ✓ | table / table |
| `nunjucks` | text / bytes | — | jinja | — | — | nunjucks / nunjucks |
| `obj` | vector | — | structured | — | ✓ | 3d / 3d |
| `ocio` | text / bytes | — | yaml | — | — | yaml / yaml |
| `odp` | container-report | — | plain | — | ✓ | powerpoint / powerpoint |
| `ods` | container-report | — | plain | — | ✓ | table / table |
| `odt` | container-report | — | plain | — | ✓ | word / word |
| `opml` | opml | — | html | — | — | xml / xml |
| `org` | text / bytes | ConvertedMarkup | structured | — | ✓ | markdown / markdown |
| `osl` | text / bytes | — | c | — | — | file / file |
| `otc` | native | — | plain | Fonts | ✓ | font / font |
| `otf` | native | — | plain | Fonts | ✓ | font / font |
| `pages` | native | — | plain | Pages | ✓ | word / word |
| `pdf` | native | — | plain | PDF | ✓ | pdf / pdf |
| `php` | text / bytes | — | php | Source Code | ✓ | php / php |
| `php3` | text / bytes | — | php | — | — | file / file |
| `php4` | text / bytes | — | php | — | — | php / php |
| `php5` | text / bytes | — | php | — | — | php / php |
| `phtml` | text / bytes | — | php | — | — | php / php |
| `pikchr` | text / bytes | ConvertedMarkup | structured | — | ✓ | file / file |
| `plist` | binary-plist | — | html | — | — | xml / xml |
| `png` | native, image | — | plain | Images | ✓ | image / image |
| `podspec` | text / bytes | — | ruby | — | — | ruby / ruby |
| `ppt` | native | — | plain | PowerPoint | ✓ | powerpoint / powerpoint |
| `pptx` | native | — | plain | PowerPoint | ✓ | powerpoint / powerpoint |
| `properties` | text / bytes | — | ini | — | — | settings / settings |
| `proto` | text / bytes | — | proto | — | — | proto / proto |
| `pxd` | text / bytes | — | python | — | — | file / file |
| `pxi` | text / bytes | — | python | — | — | file / file |
| `py` | text / bytes | — | python | Source Code | ✓ | python / python |
| `pyi` | text / bytes | — | python | — | — | python / python |
| `pyx` | text / bytes | — | python | — | — | file / file |
| `qoi` | native, image | — | plain | — | ✓ | file / file |
| `rake` | text / bytes | — | ruby | — | — | ruby / ruby |
| `rar` | archive | — | plain | Archives | ✓ | zip / zip |
| `rb` | text / bytes | — | ruby | Source Code | ✓ | ruby / ruby |
| `rs` | text / bytes | — | rust | Source Code | ✓ | rust / rust |
| `rst` | text / bytes | ConvertedMarkup | structured | — | ✓ | markdown / markdown |
| `rtf` | native | — | plain | Word | ✓ | word / word |
| `ru` | text / bytes | — | ruby | — | — | ruby / ruby |
| `s3db` | sqlite | — | plain | — | ✓ | database / database |
| `safetensors` | safetensors | — | plain | — | ✓ | file / file |
| `sbt` | text / bytes | — | scala | — | — | sbt / sbt |
| `sc` | text / bytes | — | scala | — | — | scala / scala |
| `scala` | text / bytes | — | scala | — | — | scala / scala |
| `scss` | text / bytes | — | css | — | — | sass / sass |
| `service` | text / bytes | — | ini | — | — | file / file |
| `sh` | text / bytes | — | bash | Source Code | ✓ | console / console |
| `shm` | sqlite | — | plain | — | ✓ | database / database |
| `sql` | text / bytes | — | sql | Source Code | ✓ | database / database |
| `sqlite` | sqlite | — | plain | SQLite | ✓ | database / database |
| `sqlite-shm` | sqlite | — | plain | — | ✓ | database / database |
| `sqlite-wal` | sqlite | — | plain | — | ✓ | database / database |
| `sqlite3` | sqlite | — | plain | SQLite | ✓ | database / database |
| `sqlite3-shm` | sqlite | — | plain | — | ✓ | database / database |
| `sqlite3-wal` | sqlite | — | plain | — | ✓ | database / database |
| `srt` | text / bytes | ConvertedMarkup | structured | — | ✓ | subtitles / subtitles |
| `stan` | text / bytes | — | structured | — | ✓ | stan / stan |
| `stl` | vector | — | structured | — | ✓ | 3d / 3d |
| `svelte` | text / bytes | — | svelte | — | — | svelte / svelte |
| `svg` | native, image | — | plain | Images | ✓ | svg / svg |
| `swift` | text / bytes | — | swift | Source Code | ✓ | swift / swift |
| `tar` | archive, container-report | — | plain | Archives | ✓ | zip / zip |
| `taz` | archive | — | plain | Archives | ✓ | zip / zip |
| `tbz` | archive | — | plain | Archives | ✓ | zip / zip |
| `tbz2` | archive | — | plain | Archives | ✓ | zip / zip |
| `tgz` | archive | — | plain | Archives | ✓ | zip / zip |
| `tif` | native, image | — | plain | — | — | image / image |
| `tiff` | native, image | — | plain | Images | ✓ | image / image |
| `tlz` | archive | — | plain | Archives | ✓ | zip / zip |
| `tmx` | text / bytes | — | html | — | — | xml / xml |
| `toml` | text / bytes | — | toml | YAML / TOML | ✓ | toml / toml_light |
| `ts` | text / bytes | — | typescript | Source Code | ✓ | typescript / typescript |
| `tsbuildinfo` | text / bytes | — | json | — | — | json / json |
| `tsv` | text / bytes | TableTsv, StructuredGraph | plain | — | ✓ | table / table |
| `tsx` | text / bytes | — | tsx | Source Code | ✓ | react_ts / react_ts |
| `ttc` | native | — | plain | Fonts | ✓ | font / font |
| `ttf` | native | — | plain | Fonts | ✓ | font / font |
| `twig` | text / bytes | — | jinja | — | — | twig / twig |
| `txt` | text / bytes | — | plain | Plain Text | ✓ | document / document |
| `txz` | archive | — | plain | Archives | ✓ | zip / zip |
| `tzst` | archive | — | plain | Archives | ✓ | zip / zip |
| `vega.json` | text / bytes | ConvertedMarkup, StructuredGraph | json | — | — | json / json |
| `vert` | text / bytes | — | c | — | — | shader / shader |
| `vtt` | text / bytes | ConvertedMarkup | structured | — | ✓ | subtitles / subtitles |
| `vue` | text / bytes | — | html | — | — | vue / vue |
| `wal` | sqlite | — | plain | — | ✓ | database / database |
| `wav` | native | — | plain | Audio | ✓ | audio / audio |
| `webp` | native, image | — | plain | Images | ✓ | image / image |
| `xar` | archive | — | plain | Archives | ✓ | zip / zip |
| `xls` | native | — | plain | Excel | ✓ | table / table |
| `xlsm` | native | — | plain | Excel | ✓ | table / table |
| `xlsx` | native | — | plain | Excel | ✓ | table / table |
| `xmind` | xmind | — | plain | Mind maps | ✓ | tree / tree |
| `xml` | text / bytes | StructuredGraph | html | XML | ✓ | xml / xml |
| `xsd` | text / bytes | — | html | — | — | xml / xml |
| `xsl` | text / bytes | — | html | — | — | xml / xml |
| `xz` | archive, container-report | — | plain | Archives | ✓ | zip / zip |
| `yaml` | text / bytes | StructuredGraph | yaml | YAML / TOML | ✓ | yaml / yaml |
| `yml` | text / bytes | StructuredGraph | yaml | YAML / TOML | ✓ | yaml / yaml |
| `z` | archive | — | plain | Archives | ✓ | zip / zip |
| `zig` | text / bytes | — | zig | — | — | zig / zig |
| `zip` | archive, container-report | — | plain | Archives | ✓ | zip / zip |
| `zon` | text / bytes | — | zig | — | — | zig / zig |
| `zsh` | text / bytes | — | bash | — | — | console / console |
| `zst` | archive | — | plain | Archives | ✓ | zip / zip |
| `zstd` | archive | — | plain | Archives | ✓ | zip / zip |
