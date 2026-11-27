#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
case "${1:-}" in
    ""|--build-only|--toolbar-only|--pdf-controls-only|--quicklook-lifecycle-only|--request-contract-only) ;;
    *)
        echo 'Usage: scripts/test-native-preview.sh [--build-only|--toolbar-only|--pdf-controls-only|--quicklook-lifecycle-only|--request-contract-only]' >&2
        exit 1
        ;;
esac
preview_test_dir=$(mktemp -d)
trap 'rm -rf "$preview_test_dir"' EXIT
python3 scripts/generate-reader-limits.py --out-dir "$preview_test_dir"
python3 scripts/generate-reader-actions.py --out-dir "$preview_test_dir"
python3 scripts/embed-openapi.py "$preview_test_dir"
swift_flags=(-swift-version 5)
if [[ -f "$(xcrun --show-sdk-path)/System/Library/Frameworks/AppKit.framework/Headers/NSGlassEffectView.h" ]]; then
    swift_flags+=(-DGLIM_NATIVE_GLASS)
fi
preview_sources=()
while IFS= read -r source || [[ -n "$source" ]]; do
    preview_sources+=("native/${source%$'\r'}")
done < native/preview-sources.txt
xcrun swiftc "${swift_flags[@]}" \
    "$preview_test_dir/ReaderActionPresets.swift" "$preview_test_dir/ReaderLimits.swift" "$preview_test_dir/OpenAPIAssets.swift" \
    "${preview_sources[@]}" \
    native/tests/ReaderActionTests.swift native/tests/ReaderResourcesTests.swift native/tests/EmbeddedPreviewTests.swift -o "$preview_test_dir/preview-tests"
if [[ "${1:-}" != --build-only ]]; then
    "$preview_test_dir/preview-tests" "${1:-}"
fi
