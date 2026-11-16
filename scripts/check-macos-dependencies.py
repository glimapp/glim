#!/usr/bin/env python3
"""Reject build-machine dylibs and search paths in every bundled Mach-O file."""
from pathlib import Path
import re
import subprocess
import sys


def check_bundle(bundle):
    checked = 0
    for path in sorted(bundle.rglob('*')):
        if not path.is_file() or path.is_symlink():
            continue
        kind = subprocess.check_output(['file', '-b', str(path)], text=True)
        if 'Mach-O' not in kind:
            continue
        checked += 1
        commands = subprocess.check_output(['otool', '-l', str(path)], text=True)
        for block in commands.split('Load command ')[1:]:
            command = re.search(r'^\s*cmd (LC_\w+)\s*$', block, re.M)
            if not command:
                continue
            name = command[1]
            if name not in {'LC_LOAD_DYLIB', 'LC_LOAD_WEAK_DYLIB', 'LC_REEXPORT_DYLIB',
                            'LC_LOAD_UPWARD_DYLIB', 'LC_LAZY_LOAD_DYLIB', 'LC_RPATH'}:
                continue
            value = re.search(r'^\s*(?:name|path) (.+) \(offset \d+\)', block, re.M)
            if not value:
                raise ValueError(f'Unrecognized {name} in {path}')
            dependency = value[1]
            if dependency.startswith(('/System/Library/', '/usr/lib/')):
                continue
            if dependency.startswith(('@rpath/', '@loader_path/', '@executable_path/')):
                continue
            raise ValueError(f'{path.relative_to(bundle)}: external {name}: {dependency}')
    if not checked:
        raise ValueError('No Mach-O files found in bundle')
    print(f'Verified {checked} Mach-O files: no external non-system dylibs or search paths.')


if __name__ == '__main__':
    check_bundle(Path(sys.argv[1]).resolve())
