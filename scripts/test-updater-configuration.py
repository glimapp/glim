#!/usr/bin/env python3
"""Exercise both Mach-O deployment formats using real cross-compiled binaries."""
import base64
import os
from pathlib import Path
import plistlib
import subprocess
import tempfile

script = Path(__file__).with_name('configure-updater.py')
with tempfile.TemporaryDirectory(prefix='glim-deployment-test-') as tmp:
    root = Path(tmp)
    source = root / 'main.c'
    source.write_text('int main(void) { return 0; }\n')
    for arch, minimum, command in [
        ('x86_64', '10.13', 'LC_VERSION_MIN_MACOSX'),
        ('arm64', '11.0', 'LC_BUILD_VERSION'),
    ]:
        bundle = root / f'{arch}.app'
        binary = bundle / 'Contents/MacOS/glim'
        binary.parent.mkdir(parents=True)
        plist = bundle / 'Contents/Info.plist'
        plist.write_bytes(plistlib.dumps({}))
        subprocess.run(['xcrun', 'clang', '-target', f'{arch}-apple-macos{minimum}',
                        str(source), '-o', str(binary)], check=True, capture_output=True)
        output = subprocess.check_output(['xcrun', 'vtool', '-show-build', str(binary)], text=True)
        assert command in output
        env = os.environ.copy()
        env.update(GLIM_ENABLE_UPDATER='1',
                   GLIM_UPDATE_PUBLIC_KEY=base64.b64encode(bytes(range(32))).decode())
        subprocess.run(['python3', str(script), str(bundle)], env=env, check=True)
        info = plistlib.loads(plist.read_bytes())
        assert info['LSMinimumSystemVersion'] == '11.0'
        assert info['SUFeedURL'].endswith(f'appcast-macos-{arch}.xml')
        assert info['GlimUpdatesEnabled'] is True
        assert info['SURequireSignedFeed'] is True
        assert info['SUEnableAutomaticChecks'] is True
        assert info['SUScheduledCheckInterval'] == 3600
print('Updater configuration: Intel legacy and Apple Silicon deployment commands passed.')
