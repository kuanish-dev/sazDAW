#!/bin/bash
# Builds SazDAW.dmg from a finished SazDAW.app without Finder or AppleScript
# (Tauri's stock dmg build asks for permission to control Finder and hangs without it).
set -euo pipefail
cd "$(dirname "$0")/.."
APP="target/release/bundle/macos/SazDAW.app"
OUT="target/release/bundle/SazDAW.dmg"
[ -d "$APP" ] || { echo "Missing $APP — first run: cd app && npx tauri build"; exit 1; }
# The plugin scanning helper (a separate process: a plugin that crashes while loading doesn't take the app down).
# `npx tauri build` builds it itself (beforeBuildCommand); the app looks for it next to its own executable.
SCANNER="target/release/daw-plugin-scan"
[ -x "$SCANNER" ] || { echo "Missing $SCANNER — first run: cargo build --release -p daw-vst3 --bin daw-plugin-scan"; exit 1; }
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/"
cp "$SCANNER" "$STAGE/SazDAW.app/Contents/MacOS/"
# Licenses ship with the app: SazDAW's own GPL-3.0-or-later, third-party license texts and the component list
# (LAME's LGPL requires the texts to accompany a distributed build).
python3 scripts/third_party_notices.py --check || exit 1
RES="$STAGE/SazDAW.app/Contents/Resources"
cp LICENSE THIRD_PARTY_NOTICES.md "$RES/"
cp -R licenses "$RES/"
# Files were added to the signed bundle above, so its signature no longer matches: re-sign the whole bundle ad hoc
# (no Apple ID). Without a valid signature macOS on Apple Silicon calls a downloaded app "damaged" and refuses to open it.
codesign --force --deep --sign - "$STAGE/SazDAW.app"
codesign --verify --deep --strict "$STAGE/SazDAW.app"
ln -s /Applications "$STAGE/Applications"
rm -f "$OUT"
hdiutil create -volname "SazDAW" -srcfolder "$STAGE" -ov -format UDZO "$OUT" >/dev/null
echo "Done: $OUT ($(du -h "$OUT" | cut -f1))"
