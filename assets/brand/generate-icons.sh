#!/usr/bin/env bash
# Every app icon, at every size, from one picture: assets/brand/madrasa-icon.svg.
#
#   assets/brand/generate-icons.sh
#
# Writes the desktop icon set (Windows .ico, Linux PNGs, macOS .icns, Store logos) into
# apps/desktop/src-tauri/icons and apps/parent/src-tauri/icons, and the parent app's Android
# launcher icons (legacy, round and adaptive, every density) into
# apps/parent/src-tauri/icons/android, which scripts/build-parent-apk.sh copies into the
# generated Android project. The Android adaptive icon's two layers are cut from the same
# SVG: the tile alone (background) and the emblem alone (foreground).
#
# Needs `pnpm install` done (it uses the Tauri CLI). Edit the SVG through
# assets/brand/tools/build_svg.py, not by hand, then run this again and commit the result.
set -euo pipefail
cd "$(dirname "$0")/../.."

src=assets/brand/madrasa-icon.svg
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
tauri() { pnpm --silent --dir apps/desktop exec tauri "$@"; }

# Desktop: the icon as it is.
tauri icon "$PWD/$src" -o "$work/desktop" >/dev/null 2>&1
for app in desktop parent; do
  dest="apps/$app/src-tauri/icons"
  find "$dest" -maxdepth 1 -type f \( -name '*.png' -o -name '*.ico' -o -name '*.icns' \) -delete
  cp "$work/desktop"/*.png "$work/desktop"/icon.ico "$work/desktop"/icon.icns "$dest/"
done

# Android: background = the tile filling the square; foreground = the emblem alone.
python3 - "$src" "$work" <<'PY'
import re, sys
src, work = sys.argv[1], sys.argv[2]
svg = open(src, encoding="utf-8").read()
tile = re.search(r'\n  <rect id="tile"[^>]*/>', svg).group(0)
emblem = re.search(r'\n  <g id="emblem".*\n  </g>(?=\n</svg>)', svg, re.S).group(0)
full_tile = tile.replace('x="32" y="32" width="960" height="960" rx="216"', 'x="0" y="0" width="1024" height="1024"')
open(f"{work}/android-bg.svg", "w").write(svg.replace(emblem, "").replace(tile, full_tile))
# Launchers crop the foreground to a circle (or squircle) of about 61% of the layer, so the
# emblem goes in smaller than on the tile.
fg = emblem.replace("scale(0.86)", "scale(0.58)")
assert fg != emblem, "the emblem's scale in madrasa-icon.svg changed: update this script"
open(f"{work}/android-fg.svg", "w").write(svg.replace(tile, "").replace(emblem, fg))
open(f"{work}/manifest.json", "w").write(
    '{"default": "%s", "android_bg": "android-bg.svg", "android_fg": "android-fg.svg"}'
    % src.replace("\\", "/").split("/")[-1]
)
PY
cp "$src" "$work/"
tauri icon "$work/manifest.json" -o "$work/android" >/dev/null 2>&1
rm -rf apps/parent/src-tauri/icons/android
cp -r "$work/android/android" apps/parent/src-tauri/icons/android
echo "Icons written from $src"
