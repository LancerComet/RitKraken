#!/bin/bash
set -euo pipefail

fail() {
    printf '%s\n' "$1" >&2
    exit 1
}

[[ "$(uname -s)" == Darwin ]] || fail 'Run this build script on macOS.'
command -v cargo >/dev/null && command -v rustc >/dev/null ||
    fail 'Install Rust and Cargo with rustup before building.'
xcrun --sdk macosx --show-sdk-path >/dev/null ||
    fail 'Install the macOS SDK with xcode-select --install.'

project_dir="$(CDPATH= cd -- "$(dirname "$0")" && pwd -P)"
cd "$project_dir"
target="$(rustc -vV | sed -n 's/^host: //p')"
case "$target" in
    aarch64-apple-darwin | x86_64-apple-darwin) ;;
    *) fail "Use a macOS Rust toolchain. Detected target: $target" ;;
esac

export MACOSX_DEPLOYMENT_TARGET=15.0
target_dir="$project_dir/target/macos"
build_log="$(mktemp "${TMPDIR:-/tmp}/ritkraken-build.XXXXXX")"
cargo build --locked --release --package ritkraken --bin RitKraken \
    --target "$target" --target-dir "$target_dir" \
    --message-format=json-render-diagnostics > "$build_log"
out_dir="$(python3 -c '
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    if msg.get("reason") == "build-script-executed" and "#ritkraken@" in msg.get("package_id", ""):
        print(msg["out_dir"])
        break
' "$build_log")"
rm -f "$build_log"
[[ -n "$out_dir" && -f "$out_dir/RitKraken.icns" ]] ||
    fail 'Could not locate the generated macOS application icon RitKraken.icns.'

package_id="$(cargo pkgid --locked --offline --package ritkraken)"
version="${package_id##*#}"
version="${version##*@}"
version="${version%%[-+]*}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
    fail "Could not read the RitKraken version from $package_id."

release_dir="$target_dir/$target/release"
bundle="$release_dir/RitKraken.app"
staging="$release_dir/.ritkraken-bundle"
mkdir "$staging" || fail "Could not create the bundle staging directory $staging."
candidate="$staging/RitKraken.app"
previous="$staging/previous.app"

cleanup() {
    if [[ -e "$previous" || -L "$previous" ]] && [[ ! -e "$bundle" && ! -L "$bundle" ]]; then
        mv "$previous" "$bundle" || return 1
    fi
    rm -rf "$staging"
}
trap cleanup EXIT
trap 'exit 1' INT TERM

mkdir -p "$candidate/Contents/MacOS" "$candidate/Contents/Resources"
install -m 755 "$release_dir/RitKraken" "$candidate/Contents/MacOS/RitKraken"
install -m 644 "$out_dir/RitKraken.icns" "$candidate/Contents/Resources/RitKraken.icns"
cat > "$candidate/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>RitKraken</string>
    <key>CFBundleIdentifier</key>
    <string>org.ritkraken.desktop</string>
    <key>CFBundleExecutable</key>
    <string>RitKraken</string>
    <key>CFBundleIconFile</key>
    <string>RitKraken</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>$MACOSX_DEPLOYMENT_TARGET</string>
</dict>
</plist>
PLIST
plutil -lint "$candidate/Contents/Info.plist"
codesign --force --sign - "$candidate"
codesign --verify --strict "$candidate"

if [[ -e "$bundle" || -L "$bundle" ]]; then
    mv "$bundle" "$previous"
fi
mv "$candidate" "$bundle"
printf 'Built %s\n' "$bundle"
