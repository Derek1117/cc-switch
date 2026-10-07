#!/bin/bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
    printf '%s\n' 'This build script requires macOS.' >&2
    exit 1
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
pnpm tauri build --bundles app -c "$root/src-tauri/tauri.local.conf.json"

app="$root/src-tauri/target/release/bundle/macos/CC Switch Local.app"
/usr/libexec/PlistBuddy -c 'Set :CFBundleURLTypes:0:CFBundleURLSchemes:0 ccswitch-local' "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"
printf 'Signed release app: %s\n' "$app"
