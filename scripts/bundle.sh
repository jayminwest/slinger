#!/usr/bin/env bash
# Builds target/slinger.app from the release build of slinger-app. macOS only.
#   scripts/bundle.sh
# The bundle is signed ad hoc, which is enough to run it on this machine. To
# distribute it, sign it with a Developer ID and notarize it.
# The bundle holds the app alone: the app neither ships nor starts slingerd,
# it only dials the socket of one already running. A slingerd built from this
# checkout (cargo build or run) stamps its own commit (crates/daemon/build.rs).
set -euo pipefail
cd "$(dirname "$0")/.."

[ "$(uname -s)" = Darwin ] || { echo "✗ bundle.sh builds a macOS app; this is $(uname -s)" >&2; exit 1; }

target="${CARGO_TARGET_DIR:-target}"
app="$target/slinger.app"
# Remove the old bundle first, so a failed build leaves none behind.
rm -rf "$app"
cargo build --release --locked --bin slinger-app

version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
mkdir -p "$app/Contents/MacOS"
cp "$target/release/slinger-app" "$app/Contents/MacOS/slinger-app"
# The plist names the bundle and says why it reads the folders macOS guards
# (Desktop, Documents, Downloads), which the open panel shows.
sed "s/@VERSION@/$version/g" crates/app/Info.plist >"$app/Contents/Info.plist"
plutil -lint -s "$app/Contents/Info.plist"
codesign --force --sign - "$app" 2>/dev/null
echo "$app"
