#!/usr/bin/env python3
"""Check crate direction and the built-in reader registration contract."""
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parent.parent
subprocess.run([os.environ.get('PYTHON', 'python3'), str(ROOT / 'scripts/generate-format-catalog.py'), '--check'], check=True)
metadata = json.loads(subprocess.check_output(
    [os.environ.get('CARGO', 'cargo'), 'metadata', '--no-deps', '--locked', '--format-version', '1'],
    cwd=ROOT, text=True))
allowed = {
    'glim-core': set(),
    'glim-services': {'glim-core'},
    'glim-app': {'glim-core', 'glim-services'},
}

packages = {p['name']: p for p in metadata['packages'] if p['id'] in metadata['workspace_members']}
errors = []
for name, package in packages.items():
    if name not in allowed:
        errors.append(f'{name}: declare the new crate boundary in this check')
        continue
    for dependency in package['dependencies']:
        target = dependency['name']
        if target in packages and target not in allowed[name]:
            errors.append(f'{name} must not depend on {target}')
        if name in ('glim-core', 'glim-services') and (target.startswith('gpui') or target in ('wry', 'tao')):
            errors.append(f'{name}: UI dependency {target} belongs in glim-app')

registry = (ROOT / 'crates/glim-services/src/documents/registry.rs').read_text()
identifiers = re.findall(r'\bid:\s*"([^"]+)"', registry)
if not identifiers or len(set(identifiers)) != len(identifiers):
    errors.append('built-in reader IDs must be nonempty and unique')
for identifier in identifiers:
    if not re.fullmatch(r'[a-z][a-z0-9-]*', identifier):
        errors.append(f'invalid built-in reader ID: {identifier}')

# Paths may be emitted for playlist actions, but prepared-format dispatch belongs to reader construction.
for path in (ROOT / 'crates/glim-app/src/views/workspace').glob('*.rs'):
    source = path.read_text()
    if re.search(r'(?:Ok\(Content::\w+\([^\n]*\)\)|Content::\w+\([^\n]*\))\s*=>', source):
        errors.append(f'{path.relative_to(ROOT)}: move prepared-format dispatch to reader/construction')

# These presentation modules consume pure prepared data and explicit host hooks.
# Keep this boundary even while they compile inside the app crate.
reader_root = ROOT / 'crates/glim-app/src/views/reader'
for name in ('contract.rs', 'controls.rs', 'opml.rs', 'xmind.rs', 'json/view.rs', 'csv/view.rs'):
    source = (reader_root / name).read_text()
    if re.search(r'\bglim_services\s*::|\bcrate::app\s*::|\bstd::(?:fs|process|net)\s*::', source):
        errors.append(f'{name}: use pure payloads and injected host operations')

if errors:
    raise SystemExit('\n'.join(errors))
print('Crate direction and built-in reader registration boundaries are consistent.')
