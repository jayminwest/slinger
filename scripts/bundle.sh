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
# Signing: a Developer ID or Apple Development certificate from the keychain when
# there is one (SLINGER_SIGN_IDENTITY names another), else ad hoc, which is enough to
# run the app on this machine.
# SLINGER_NOTARIZE=1 makes the artifact to distribute: it needs a Developer ID identity
# and a notarytool keychain profile (SLINGER_NOTARY_PROFILE, default "slinger"; make it
# once with `xcrun notarytool store-credentials slinger`). It signs with the hardened
# runtime, sends the app to Apple (network), staples the ticket and writes
# $CARGO_TARGET_DIR/slinger-<version>-<arch>.app.zip.
set -euo pipefail
cd "$(dirname "$0")/.."

install=false
case "${1:-}" in
  --install) install=true ;;
  "") ;;
  *) echo "✗ usage: scripts/bundle.sh [--install]" >&2; exit 2 ;;
esac

[ "$(uname -s)" = Darwin ] || { echo "✗ bundle.sh builds a macOS app; this is $(uname -s)" >&2; exit 1; }

# A bundle that failed a check is removed, so none is left half made.
partial=
fail() { echo "✗ $*" >&2; [ -z "$partial" ] || rm -rf "$partial"; exit 1; }

# Signing identity: SLINGER_SIGN_IDENTITY, else the first "Developer ID Application"
# certificate in the keychain, else the first "Apple Development" one, else ad hoc
# (CI). A stable identity keeps macOS privacy grants across rebuilds; an ad hoc
# signature changes every build, so macOS asks again each time.
# Not ${SLINGER_SIGN_IDENTITY:-$(...)}: macOS /bin/bash 3.2 misparses the quotes inside.
identity="${SLINGER_SIGN_IDENTITY:-}"
if [ -z "$identity" ]; then
  identities="$(security find-identity -v -p codesigning 2>/dev/null || true)"
  for kind in "Developer ID Application" "Apple Development"; do
    identity="$(awk -F'"' -v k="$kind: " 'index($2, k) == 1 {print $2; exit}' <<<"$identities")"
    if [ -n "$identity" ]; then break; fi
  done
fi
notarize=false
if [ "${SLINGER_NOTARIZE:-0}" = 1 ]; then
  notarize=true
  # Checked before the build: Apple notarizes only Developer ID signatures.
  case "$identity" in
    "" | - | "Apple Development:"*)
      fail "SLINGER_NOTARIZE=1 needs a Developer ID Application certificate in the keychain (found: ${identity:-none})" ;;
  esac
fi

target="${CARGO_TARGET_DIR:-target}"
app="$target/slinger.app"
programs=(slinger-app slingerd slinger)
helpers=(slingerd slinger)
# Remove the old bundle first, so a failed build leaves none behind.
rm -rf "$app"
partial=$app
cargo build --release --locked "${programs[@]/#/--bin=}"

version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
# The build number grows with each commit; without git it is the version.
build=$(git rev-list --count HEAD 2>/dev/null || echo "$version")
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
for program in "${programs[@]}"; do
  [ -f "$target/release/$program" ] || fail "no $target/release/$program after the build"
  cp "$target/release/$program" "$app/Contents/MacOS/$program"
done
# The icon is committed; scripts/icon.sh rebuilds it from its source.
icon=crates/app/slinger.icns
[ -s "$icon" ] || fail "no app icon at $icon (scripts/icon.sh makes it)"
cp "$icon" "$app/Contents/Resources/slinger.icns"
# The plist names the bundle and says why it reads the folders macOS guards
# (Desktop, Documents, Downloads), which the open panel shows.
sed -e "s/@VERSION@/$version/g" -e "s/@BUILD@/$build/g" crates/app/Info.plist >"$app/Contents/Info.plist"
plutil -lint -s "$app/Contents/Info.plist" || fail "$app/Contents/Info.plist is not a valid plist"
if [ -z "$identity" ]; then
  identity=-
  echo "warning: no Developer ID Application or Apple Development identity; ad hoc signing" >&2
fi
sign_args=(--force --sign "$identity")
# Notarization needs the hardened runtime and a secure timestamp on every program.
if $notarize; then sign_args+=(--options runtime --timestamp); fi
# Helpers first, then the bundle, which seals them.
for program in "${helpers[@]}"; do
  codesign "${sign_args[@]}" "$app/Contents/MacOS/$program" 2>/dev/null ||
    fail "could not sign $app/Contents/MacOS/$program as $identity"
done
codesign "${sign_args[@]}" "$app" 2>/dev/null || fail "could not sign $app as $identity"
codesign --verify --strict "$app" || fail "$app does not pass 'codesign --verify --strict'"

if $notarize; then
  profile="${SLINGER_NOTARY_PROFILE:-slinger}"
  zip="$target/slinger-$version-$(uname -m).app.zip"
  rm -f "$zip"
  # ditto, not zip: the code signature survives.
  ditto -c -k --keepParent "$app" "$zip"
  xcrun notarytool submit "$zip" --keychain-profile "$profile" --wait ||
    fail "notarytool submit failed (xcrun notarytool store-credentials $profile)"
  # Stapling fails when Apple did not accept the app, whatever submit printed.
  xcrun stapler staple "$app" ||
    fail "Apple did not notarize $app (xcrun notarytool log <submission id> --keychain-profile $profile)"
  spctl --assess --type execute "$app" || fail "Gatekeeper refuses $app after notarization"
  # Zip again: the first zip has no stapled ticket.
  rm -f "$zip"
  ditto -c -k --keepParent "$app" "$zip"
  echo "notarized $zip" >&2
fi
partial=

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
