#!/usr/bin/env python3
"""Regress hotfix ordering and reject Homebrew linkage on both Mac architectures."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

from release_version import cargo_version, parse_version, validate_tag

spec = importlib.util.spec_from_file_location('dependencies', Path(__file__).with_name('check-macos-dependencies.py'))
dependencies = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dependencies)


class HotfixReleaseTests(unittest.TestCase):
    def test_numeric_hotfix_ordering(self):
        versions = ['0.2.26', '0.2.26.1', '0.2.26.2', '0.2.26.10', '0.2.27', '0.3.0']
        self.assertEqual(sorted(reversed(versions), key=parse_version), versions)
        self.assertEqual(validate_tag('v0.2.26.1'), '0.2.26.1')
        self.assertEqual(cargo_version('0.2.26.1'), '0.2.26+hotfix.1')
        self.assertEqual(cargo_version('0.2.27'), '0.2.27')

    def test_unstable_and_malformed_versions_are_rejected(self):
        for version in ['v0.2.26', '0.2', '0.2.26.0', '0.2.26.01', '0.2.26.1.2',
                        '0.2.26-beta.1', '0.2.26+hotfix.1', '0.2.26.1\n']:
            with self.subTest(version=version), self.assertRaises(ValueError):
                parse_version(version)
        with self.assertRaises(ValueError):
            validate_tag('0.2.26.1')

    def test_external_linkage_in_main_or_helper_is_rejected(self):
        with tempfile.TemporaryDirectory(prefix='glim-dependency-test-') as directory:
            root = Path(directory)
            library = root / 'dependency.c'
            library.write_text('int dependency(void) { return 0; }\n')
            source = root / 'main.c'
            source.write_text('extern int dependency(void); int main(void) { return dependency(); }\n')
            for arch, prefix in [('arm64', '/opt/homebrew'), ('x86_64', '/usr/local')]:
                target = f'{arch}-apple-macos11'
                dylib = root / f'{arch}.dylib'
                subprocess.run(['xcrun', 'clang', '-target', target, '-dynamiclib', str(library),
                                '-Wl,-install_name,' + prefix + '/opt/xz/lib/liblzma.5.dylib',
                                '-o', str(dylib)], check=True, capture_output=True)
                for location in ['Contents/MacOS/glim',
                                 'Contents/Helpers/GlimRenderer.app/Contents/MacOS/glim-renderer']:
                    bundle = root / f'{arch}-{Path(location).name}.app'
                    executable = bundle / location
                    executable.parent.mkdir(parents=True)
                    subprocess.run(['xcrun', 'clang', '-target', target, str(source), str(dylib),
                                    '-o', str(executable)], check=True, capture_output=True)
                    with self.subTest(arch=arch, location=location), self.assertRaisesRegex(ValueError, 'liblzma'):
                        dependencies.check_bundle(bundle)
                    subprocess.run(['xcrun', 'clang', '-target', target, str(source), str(library),
                                    '-o', str(executable)], check=True, capture_output=True)
                    dependencies.check_bundle(bundle)


if __name__ == '__main__':
    unittest.main()
