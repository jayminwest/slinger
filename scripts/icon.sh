#!/usr/bin/env bash
# Rebuilds crates/app/slinger.icns from crates/app/icon/slinger.svg with the
# tools of a stock macOS (sips renders the SVG, iconutil packs the sizes).
# Commit the .icns: scripts/bundle.sh copies it and does not run this.
#   scripts/icon.sh
set -euo pipefail
cd "$(dirname "$0")/.."

[ "$(uname -s)" = Darwin ] || { echo "✗ icon.sh needs macOS (sips, iconutil); this is $(uname -s)" >&2; exit 1; }

source=crates/app/icon/slinger.svg
out=crates/app/slinger.icns
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
set=$work/slinger.iconset
mkdir "$set"

sips -s format png "$source" --out "$work/1024.png" >/dev/null
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$work/1024.png" --out "$set/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$work/1024.png" --out "$set/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$set" -o "$out"
echo "$out"
