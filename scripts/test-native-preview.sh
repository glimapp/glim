#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "${1:-}" != "" && "${1:-}" != --build-only && "${1:-}" != --toolbar-only && "${1:-}" != --pdf-controls-only && "${1:-}" != --quicklook-lifecycle-only ]]; then
    echo 'Usage: scripts/test-native-preview.sh [--build-only|--toolbar-only|--pdf-controls-only|--quicklook-lifecycle-only]' >&2
    exit 1
fi
preview_test_dir=$(mktemp -d)
trap 'rm -rf "$preview_test_dir"' EXIT
python3 scripts/generate-reader-limits.py --out-dir "$preview_test_dir"
python3 scripts/generate-reader-actions.py --out-dir "$preview_test_dir"
python3 scripts/embed-openapi.py "$preview_test_dir"
glass_flags=()
if [[ -f "$(xcrun --show-sdk-path)/System/Library/Frameworks/AppKit.framework/Headers/NSGlassEffectView.h" ]]; then
    glass_flags+=(-DGLIM_NATIVE_GLASS)
fi
preview_sources=()
while IFS= read -r source || [[ -n "$source" ]]; do
    preview_sources+=("native/${source%$'\r'}")
done < native/preview-sources.txt
xcrun swiftc "${glass_flags[@]}" -swift-version 5 \
    "$preview_test_dir/ReaderActionPresets.swift" "$preview_test_dir/ReaderLimits.swift" "$preview_test_dir/OpenAPIAssets.swift" \
    "${preview_sources[@]}" \
    native/tests/ReaderActionTests.swift native/tests/ReaderResourcesTests.swift native/tests/EmbeddedPreviewTests.swift -o "$preview_test_dir/preview-tests"
if [[ "${1:-}" != --build-only ]]; then
    "$preview_test_dir/preview-tests" "${1:-}"
fi
