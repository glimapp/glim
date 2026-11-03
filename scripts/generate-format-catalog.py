#!/usr/bin/env python3
"""Compile shared format declarations and check their independent consumers."""
import argparse
import json
from pathlib import Path
import plistlib
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / 'assets/format-catalog.json'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def quote(value):
    return json.dumps(value, ensure_ascii=False)


def strings(values):
    return '[' + ', '.join(map(quote, values)) + ']'


def effective_icon(suffix, icons):
    while True:
        if suffix in icons['extensions']:
            return icons['extensions'][suffix]
        if '.' not in suffix:
            return dict(dark=icons['default'], light=icons['default'])
        suffix = suffix.split('.', 1)[1]


def read_catalog():
    data = json.loads(CATALOG.read_text())
    require(data['version'] == 1, 'Unsupported format catalog version')
    known = set()
    for name, rule in data['sets'].items():
        require(re.fullmatch('[A-Z][A-Za-z0-9]*', name), f'Invalid set: {name}')
        require(rule.get('match', 'extension') in ('extension', 'filename-suffix'), f'Invalid matching mode: {name}')
        for field in ('suffixes', 'tails'):
            values = rule.get(field, [])
            require(len(values) == len(set(values)), f'Duplicate {field} in {name}')
            for value in values:
                require(value == value.lower() and re.fullmatch(r'[a-z0-9.-]+', value), f'Invalid {field}: {value}')
                require(not value.startswith('.') and not value.endswith('.'), f'Invalid suffix boundary: {value}')
        known.update(rule['suffixes'])
    languages = {}
    for language, suffixes in data['languages'].items():
        require(re.fullmatch('[a-z]+', language), f'Invalid language: {language}')
        for suffix in suffixes:
            require(suffix not in languages and re.fullmatch('[a-z0-9-]+', suffix), f'Duplicate or invalid language suffix: {suffix}')
            languages[suffix] = language
    known.update(languages)
    identifiers = []
    for provider in data['providers']:
        identifier = provider['id']
        require(re.fullmatch('[a-z][a-z0-9-]*', identifier), f'Invalid provider: {identifier}')
        require(identifier not in identifiers, f'Duplicate provider: {identifier}')
        identifiers.append(identifier)
        require(provider['probe'] in ('none', 'required', 'any-file'), f'Invalid probe: {identifier}')
        require(all(name in data['sets'] for name in provider['sets']), f'Unknown set in {identifier}')
        require(provider['sets'] or provider['probe'] == 'any-file', f'Unreachable provider: {identifier}')
        candidates = {suffix for name in provider['sets'] for suffix in data['sets'][name]['suffixes']}
        require(set(provider.get('exclude', [])) <= candidates, f'Unknown exclusion in {identifier}')
    require(identifiers.index('book') < identifiers.index('archive'), 'Book containers must precede archives')
    # Rust retains executable function pointers; the catalog owns declaration order.
    registry = (ROOT / 'crates/glim-services/src/documents/registry.rs').read_text()
    require(re.findall(r'\bid:\s*"([^"]*)"', registry) == identifiers, 'Provider IDs/order differ from services registry')
    bundle_extensions = set()
    for group in data['bundle_types']:
        values = [suffix.lower() for suffix in group['CFBundleTypeExtensions']]
        require(len(values) == len(set(values)), 'Duplicate bundle extension')
        require(not bundle_extensions.intersection(values), 'Extension declared in multiple bundle types')
        bundle_extensions.update(values)
    for suffix in bundle_extensions:
        sidecar = any(suffix.endswith(tail) for tail in data['sets']['Sqlite']['tails'])
        require(suffix in known or sidecar, f'Bundle extension has no reading declaration: {suffix}')
    known.update(bundle_extensions)
    grouped = set()
    for group in data['associations']:
        require(group['name'] and group['extensions'], 'Empty association group')
        for suffix in group['extensions']:
            lower = suffix.lower()
            require(lower in bundle_extensions, f'Association missing bundle registration: {suffix}')
            require(lower not in grouped, f'Duplicate association: {suffix}')
            grouped.add(lower)
    icons = json.loads((ROOT / 'assets/file-icons/manifest.json').read_text())
    missing = known - icons['extensions'].keys()
    require(set(data['icon_fallbacks']) == missing, 'Update explicit icon_fallbacks for: ' + ', '.join(sorted(set(data['icon_fallbacks']) ^ missing)))
    for suffix, expected in data['icon_fallbacks'].items():
        require(expected == effective_icon(suffix, icons), f'Icon fallback changed: {suffix}')
    # Cross-check the actual table consumed by GPUI, not just the icon manifest.
    icon_source = (ROOT / 'crates/glim-app/src/views/file_icons_data.rs').read_text().split('static EXTENSIONS:')[1]
    actual = {name: dict(dark=dark, light=light) for name, dark, light in re.findall(r'\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,?\s*\)', icon_source)}
    require(actual == icons['extensions'], 'Icon manifest and runtime extension table differ')
    for suffix in known:
        for icon in effective_icon(suffix, icons).values():
            require(icon in icons['icons'] and (ROOT / f'assets/file-icons/{icon}.png').is_file(), f'Missing icon asset: {icon}')
    return data, languages, known, icons


def generated(data, languages, known, icons):
    rust = ['// Generated by scripts/generate-format-catalog.py; do not edit.', '#[derive(Clone, Copy, Debug, PartialEq, Eq)]', 'pub enum FormatSet {']
    rust.extend('    ' + name + ',' for name in data['sets'])
    rust.extend(['}', '', 'impl FormatSet {', '    fn rule(self) -> FormatRule {', '        match self {'])
    for name, rule in data['sets'].items():
        rust.extend([
            f'            Self::{name} => FormatRule {{',
            '                #[rustfmt::skip]',
            '                suffixes: &' + strings(rule['suffixes']) + ',',
            '                #[rustfmt::skip]',
            '                tails: &' + strings(rule.get('tails', [])) + ',',
            '                filename_suffix: ' + str(rule.get('match') == 'filename-suffix').lower() + ',',
            '            },',
        ])
    rust.extend(['        }', '    }', '}', '', '#[rustfmt::skip]', 'static LANGUAGES: &[(&str, &str)] = &['])
    rust.extend(f'    ({quote(suffix)}, {quote(language)}),' for suffix, language in sorted(languages.items()))
    rust.extend(['];', ''])
    swift = ['// Generated by scripts/generate-format-catalog.py; do not edit.', 'enum FormatCatalog {', '    static let associations: [(String, [String])] = [']
    swift.extend(f'        ({quote(group["name"])}, {strings(group["extensions"])}),' for group in data['associations'])
    swift.extend(['    ]', '}', ''])
    outputs = {
        'crates/glim-core/src/formats_data.rs': '\n'.join(rust),
        'native/FormatCatalog.swift': '\n'.join(swift),
    }
    for chinese in (False, True):
        text = ['# 格式能力目录' if chinese else '# Format capability catalog', '',
            '[English](format-catalog.md) · **简体中文**' if chinese else '**English** · [简体中文](format-catalog.zh-CN.md)', '',
            ('由 `assets/format-catalog.json` 生成。此目录声明候选路由、文本投影、高亮、关联与图标，不保证文件有效或系统解码器可用。编辑仅在文件成功进入 UTF-8 文本／可编辑分页路径后启用；预览能力不能推导出编辑权限。未知 UTF-8 文件仍可走普通文本路径。' if chinese else 'Generated from `assets/format-catalog.json`. Declarations cover candidate routing, text projections, highlighting, associations and icons; they do not guarantee valid content or available system codecs. Editing requires successful UTF-8 text/editable-page routing; preview support does not grant editing rights. Unknown UTF-8 files retain the ordinary text path.'), '',
            ('运行 `python3 scripts/generate-format-catalog.py --write` 更新生成文件，`--check` 核对一致性。内容探测和解析规则保留在服务模块；此处列出的路由按注册顺序排列，探测未通过时继续后续候选。二进制 plist 需要文件头，原生预览需要实际文件／包，`.key` 另需 ZIP／包识别；可执行文件与 SQLite 的文件头探测也可识别未列出的扩展名。' if chinese else 'Run `python3 scripts/generate-format-catalog.py --write` to update generated files and `--check` to verify consistency. Content probes and parser rules remain in services. Routes below follow registration order; a failed probe continues to later candidates. Binary plist requires its header; native preview requires an actual file/package and `.key` additionally requires ZIP/package recognition. Executable and SQLite header probes can recognize unlisted suffixes.'), '',
            ('文件名高亮规则（Dockerfile、`.env`、点配置文件等）继续由 `language_for_path` 判断。图标有独立的文件名／路径优先级，本表表示普通 `sample.<后缀>` 的扩展名图标。缺少专属图标时的复合后缀回退或中性图标必须在目录中显式声明。' if chinese else 'Filename highlighting rules (Dockerfile, `.env`, dot-config names) remain in `language_for_path`. Icons have separate filename/path precedence; this table describes extension icons for ordinary `sample.<suffix>` names. Compound-suffix or neutral fallbacks without a dedicated icon must be explicitly declared.'), '',
            '| 后缀 | 候选提供者 | 文本投影 | 高亮 | 默认关联设置 | Bundle 声明 | 图标（暗 / 亮） |' if chinese else '| Suffix | Candidate providers | Text projections | Highlighting | Association settings | Bundle declaration | Icon (dark / light) |',
            '| --- | --- | --- | --- | --- | --- | --- |']
        groups = {suffix.lower(): group['name'] for group in data['associations'] for suffix in group['extensions']}
        bundle = {suffix.lower() for group in data['bundle_types'] for suffix in group['CFBundleTypeExtensions']}
        projections = ['Markdown', 'Html', 'ConvertedMarkup', 'TableCsv', 'TableTsv', 'TableGpx', 'TableKml', 'StructuredGraph']
        def matches(name, suffix):
            rule = data['sets'][name]
            return any(suffix == entry or suffix.endswith('.' + entry) for entry in rule['suffixes']) or any(suffix.endswith(tail) for tail in rule.get('tails', []))
        for suffix in sorted(known):
            providers = [p['id'] for p in data['providers'] if suffix not in p.get('exclude', []) and any(matches(name, suffix) for name in p['sets'])]
            projection = [name for name in projections if matches(name, suffix)]
            icon = effective_icon(suffix, icons)
            text.append(f'| `{suffix}` | {", ".join(providers) or "text / bytes"} | {", ".join(projection) or "—"} | {languages.get(suffix.rsplit(".",1)[-1], "plain")} | {groups.get(suffix, "—")} | {"✓" if suffix in bundle else "—"} | {icon["dark"]} / {icon["light"]} |')
        outputs['docs/format-catalog' + ('.zh-CN' if chinese else '') + '.md'] = '\n'.join(text) + '\n'
    return outputs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true')
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    require(not (args.write and args.check), 'Choose --write or --check')
    data, languages, known, icons = read_catalog()
    outputs = generated(data, languages, known, icons)
    plist_path = ROOT / 'assets/macos/Info.plist'
    plist = plistlib.loads(plist_path.read_bytes())
    if plist['CFBundleDocumentTypes'] != data['bundle_types']:
        require(args.write, 'Bundle declarations differ from catalog')
        # Preserve release metadata formatting consumed by the Rust build script.
        source = plist_path.read_text()
        start = source.index('<key>CFBundleDocumentTypes</key>') + len('<key>CFBundleDocumentTypes</key>')
        array = plistlib.dumps(data['bundle_types'], sort_keys=False).decode().split('<plist version="1.0">\n')[1].split('\n</plist>')[0]
        # The document declarations are the last value in the top-level dictionary.
        end = source.rfind('</dict>')
        require(not re.search(r'^    <key>', source[start:end], re.M), 'Unexpected keys after CFBundleDocumentTypes; update the focused writer')
        plist_path.write_text(source[:start] + '\n' + array + '\n' + source[end:])
    stale = []
    for name, expected in outputs.items():
        path = ROOT / name
        if args.write:
            path.write_text(expected)
        elif not path.exists() or path.read_text() != expected:
            stale.append(name)
    require(not stale, 'Regenerate format catalog outputs: ' + ', '.join(stale))
    print(f'Format catalog: {len(known)} suffixes, {len(data["providers"])} providers; declarations and generated consumers agree.')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError) as error:
        sys.exit(str(error))
