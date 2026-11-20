#!/usr/bin/env bash
# Builds target/slinger.app from the release build. macOS only.
#   scripts/bundle.sh [--install]
# The bundle holds all three programs in Contents/MacOS: slinger-app, and
# beside it slingerd and slinger. `<bundle>/Contents/MacOS/slingerd` is a
# contract: the app starts that daemon, and a slingerd finds the slinger
# beside it for its agents' hooks. One cargo build makes all three, so they
# carry one build stamp (crates/control/build.rs; SLINGER_BUILD_SHA in the
# environment passes through to it untouched).
# --install: then copy the app to /Applications (SLINGER_APP_DIR) and link
# slinger and slingerd in ~/.local/bin (SLINGER_BIN_DIR) to the programs
# inside the installed app, so there is one copy of each. It stops and
# starts nothing: a running daemon or app keeps its build until restarted.
# The bundle is signed ad hoc, which is enough to run it on this machine. To
# distribute it, sign it with a Developer ID and notarize it.
set -euo pipefail
cd "$(dirname "$0")/.."

install=false
case "${1:-}" in
  --install) install=true ;;
  "") ;;
  *) echo "✗ usage: scripts/bundle.sh [--install]" >&2; exit 2 ;;
esac

[ "$(uname -s)" = Darwin ] || { echo "✗ bundle.sh builds a macOS app; this is $(uname -s)" >&2; exit 1; }

fail() { echo "✗ $*" >&2; exit 1; }

target="${CARGO_TARGET_DIR:-target}"
app="$target/slinger.app"
programs=(slinger-app slingerd slinger)
helpers=(slingerd slinger)
# Remove the old bundle first, so a failed build leaves none behind.
rm -rf "$app"
cargo build --release --locked "${programs[@]/#/--bin=}"

version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
# The build number grows with each commit; without git it is the version.
build=$(git rev-list --count HEAD 2>/dev/null || echo "$version")
mkdir -p "$app/Contents/MacOS"
for program in "${programs[@]}"; do
  [ -f "$target/release/$program" ] || fail "no $target/release/$program after the build"
  cp "$target/release/$program" "$app/Contents/MacOS/$program"
done
# The plist names the bundle and says why it reads the folders macOS guards
# (Desktop, Documents, Downloads), which the open panel shows.
sed -e "s/@VERSION@/$version/g" -e "s/@BUILD@/$build/g" crates/app/Info.plist >"$app/Contents/Info.plist"
plutil -lint -s "$app/Contents/Info.plist" || fail "$app/Contents/Info.plist is not a valid plist"
# Helpers first, then the bundle, which seals them.
for program in "${helpers[@]}"; do
  codesign --force --sign - "$app/Contents/MacOS/$program" 2>/dev/null ||
    fail "could not sign $app/Contents/MacOS/$program"
done
codesign --force --sign - "$app" 2>/dev/null || fail "could not sign $app"
codesign --verify --strict "$app" || fail "$app does not pass 'codesign --verify --strict'"

if $install; then
  apps="${SLINGER_APP_DIR:-/Applications}"
  bin="${SLINGER_BIN_DIR:-$HOME/.local/bin}"
  mkdir -p "$bin"
  # Absolute, so the links point at the app from anywhere.
  apps=$(cd "$apps" && pwd -P) || fail "no folder $apps to install the app in"
  # Remove, then copy: a write over a running program breaks its signature.
  rm -rf "$apps/slinger.app"
  cp -R "$app" "$apps/slinger.app"
  app="$apps/slinger.app"
  # Whatever is there (a copy from an older install, or a link) goes.
  for program in "${helpers[@]}"; do
    rm -f "$bin/$program"
    ln -s "$app/Contents/MacOS/$program" "$bin/$program"
  done
fi
echo "$app"
