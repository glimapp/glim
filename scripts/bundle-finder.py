#!/usr/bin/env python3
"""Build the Finder extension for the staged application's actual architecture."""
from pathlib import Path
import plistlib
import subprocess
import sys

bundle = Path(sys.argv[1])
arch = subprocess.check_output(
    ['lipo', '-archs', str(bundle / 'Contents/MacOS/glim')], text=True).strip()
assert arch in ('arm64', 'x86_64'), 'One macOS architecture is required'
extension = bundle / 'Contents/PlugIns/GlimFinder.appex'
binary = extension / 'Contents/MacOS/GlimFinder'
binary.parent.mkdir(parents=True, exist_ok=True)
subprocess.run([
    'xcrun', 'swiftc', '-O', '-swift-version', '5', '-application-extension',
    '-parse-as-library', '-emit-executable', '-module-name', 'GlimFinder',
    '-target', f'{arch}-apple-macos11.0',
    '-Xlinker', '-e', '-Xlinker', '_NSExtensionMain',
    'native/FinderSync.swift', '-o', str(binary),
], check=True)
info = plistlib.loads(Path('assets/macos/FinderInfo.plist').read_bytes())
host = plistlib.loads((bundle / 'Contents/Info.plist').read_bytes())
for key in ('CFBundleShortVersionString', 'CFBundleVersion'):
    info[key] = host[key]
(extension / 'Contents/Info.plist').write_bytes(plistlib.dumps(info))
import shutil
shutil.copytree('assets/macos/Finder', extension / 'Contents/Resources', dirs_exist_ok=True)
assert subprocess.check_output(['lipo', '-archs', str(binary)], text=True).strip() == arch
