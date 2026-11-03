#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "${1:-}" != "" && "${1:-}" != --build-only ]]; then
    echo 'Usage: scripts/test-native-preview.sh [--build-only]' >&2
    exit 1
fi
preview_test_dir=$(mktemp -d)
trap 'rm -rf "$preview_test_dir"' EXIT
python3 scripts/generate-reader-limits.py --out-dir "$preview_test_dir"
xcrun swiftc -swift-version 5 \
    "$preview_test_dir/ReaderLimits.swift" native/Preview.swift native/FormatCatalog.swift native/PreviewRequest.swift native/ImagePreview.swift native/ImageTextExtraction.swift native/PreviewCopy.swift native/FloatingReaderToolbar.swift native/ReaderActions.swift native/HTMLPreview.swift native/WorkbookPreview.swift native/WordPreview.swift native/EmbeddedPreview.swift \
    native/tests/EmbeddedPreviewTests.swift -o "$preview_test_dir/preview-tests"
if [[ "${1:-}" != --build-only ]]; then
    "$preview_test_dir/preview-tests"
fi
