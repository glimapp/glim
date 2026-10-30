#!/usr/bin/env python3
"""Sign nested executables/bundles inside-out; preserve framework symlinks."""
import os
from pathlib import Path
import plistlib
import subprocess
import sys

bundle = Path(sys.argv[1]).resolve()
identity = os.environ.get('GLIM_CODESIGN_IDENTITY', '-')
options = ['--force', '--sign', identity]
options += ['--timestamp=none'] if identity == '-' else ['--timestamp', '--options', 'runtime']
paths = list(bundle.rglob('*'))
containers = [p for p in paths if p.is_dir() and not p.is_symlink()
              and p.suffix in ('.app', '.xpc', '.framework', '.appex')]
# Signing a bundle's main executable can sign its enclosing bundle as well.
# Defer these executables to their container, after every nested component.
main_executables = set()
for container in containers + [bundle]:
    framework = container.suffix == '.framework'
    metadata = container / ('Resources/Info.plist' if framework else 'Contents/Info.plist')
    info = plistlib.loads(metadata.read_bytes())
    executable = info['CFBundleExecutable']
    main_executables.add((container / (executable if framework else f'Contents/MacOS/{executable}')).resolve())
signing_units = containers.copy()
for path in paths:
    if path.is_file() and not path.is_symlink() and path.resolve() not in main_executables:
        description = subprocess.check_output(['file', '-b', str(path)], text=True)
        if 'Mach-O' in description:
            signing_units.append(path)
for path in sorted(signing_units, key=lambda p: len(p.parts), reverse=True) + [bundle]:
    entitlements = (['--entitlements', str(Path(__file__).resolve().parent.parent /
                     'assets/macos/Finder.entitlements')]
                    if path.name == 'GlimFinder.appex' else [])
    subprocess.run(['codesign', *options, *entitlements, str(path)], check=True)
subprocess.run(['codesign', '--verify', '--deep', '--strict', str(bundle)], check=True)
