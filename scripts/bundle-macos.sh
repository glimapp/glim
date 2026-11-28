#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."
if [[ "$(uname -s)" != Darwin ]]; then
    echo 'The app bundle requires macOS.' >&2
    exit 1
fi
python3 scripts/check-icons.py
export GLIM_BUILD_CHANNEL="${GLIM_BUILD_CHANNEL:-local}"
export GLIM_BUILD_TIMESTAMP="${GLIM_BUILD_TIMESTAMP:-$(date '+%Y-%m-%d %H:%M:%S %z')}"
build_profile="${1:-release}"
build_target="${2:-}"
cargo_args=(build --locked)
case "${GLIM_ENABLE_GIT:-1}" in
    1) cargo_args+=(--features git) ;;
    0) cargo_args+=(--no-default-features) ;;
    *) echo 'GLIM_ENABLE_GIT must be 0 or 1.' >&2; exit 1 ;;
esac
case "$build_profile" in
    debug) ;;
    release) cargo_args+=(--release) ;;
    *) echo 'Usage: scripts/bundle-macos.sh [debug|release] [target-triple]' >&2; exit 1 ;;
esac
if [[ -n "$build_target" ]]; then
    cargo_args+=(--target "$build_target")
fi
if [[ "${GLIM_ENABLE_UPDATER:-0}" == 1 ]]; then
    : "${GLIM_UPDATE_PUBLIC_KEY:?Set the release Ed25519 public key before enabling updates}"
    export GLIM_SPARKLE_DIR="${GLIM_SPARKLE_DIR:-$PWD/target/sparkle/2.10.0}"
    scripts/fetch-sparkle.sh >/dev/null
    cargo_args+=(--features macos-updater)
fi
cargo "${cargo_args[@]}"
# Respect Cargo target-dir overrides without assuming a checkout-local target directory.
target_dir="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
if [[ -n "$build_target" ]]; then
    target_dir="$target_dir/$build_target"
fi
bundle_dir="${GLIM_BUNDLE_DIR:-${GLIMPSE_BUNDLE_DIR:-$PWD/dist/Glim.app}}"
# The app and helper are signed independently. Clone their executable data
# when the filesystem supports it to leave room for signing's temporary files.
copy_executable() {
    /bin/cp -cf "$target_dir/$build_profile/glim" "$1" 2>/dev/null ||
        /bin/cp -f "$target_dir/$build_profile/glim" "$1"
}
mkdir -p "$bundle_dir/Contents/MacOS" "$bundle_dir/Contents/Resources"
copy_executable "$bundle_dir/Contents/MacOS/glim"
install -m 755 scripts/glim "$bundle_dir/Contents/Resources/glim"
mkdir -p "$bundle_dir/Contents/Resources/skills/glim"
install -m 644 skills/glim/SKILL.md "$bundle_dir/Contents/Resources/skills/glim/SKILL.md"
cp assets/macos/Glim.icns "$bundle_dir/Contents/Resources/Glim.icns"
cp assets/macos/Info.plist "$bundle_dir/Contents/Info.plist"
renderer_dir="$bundle_dir/Contents/Helpers/GlimRenderer.app"
mkdir -p "$renderer_dir/Contents/MacOS"
copy_executable "$renderer_dir/Contents/MacOS/glim-renderer"
cp assets/macos/RendererInfo.plist "$renderer_dir/Contents/Info.plist"
if [[ "${GLIM_ENABLE_UPDATER:-0}" == 1 ]]; then
    mkdir -p "$bundle_dir/Contents/Frameworks"
    ditto "$GLIM_SPARKLE_DIR/Sparkle.framework" "$bundle_dir/Contents/Frameworks/Sparkle.framework"
    cp "$GLIM_SPARKLE_DIR/LICENSE" "$bundle_dir/Contents/Resources/Sparkle-LICENSE.txt"
fi
python3 scripts/configure-updater.py "$bundle_dir"
python3 scripts/bundle-finder.py "$bundle_dir"
python3 scripts/check-macos-dependencies.py "$bundle_dir"
python3 scripts/sign-macos.py "$bundle_dir"
printf 'Created %s\n' "$bundle_dir"
