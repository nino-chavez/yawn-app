#!/usr/bin/env bash
# Notarized, stapled Yawn.app -> the in-app updater's package and signature:
#
#   target/release/bundle/macos/Yawn-<version>-macos-arm64.app.tar.gz
#   target/release/bundle/macos/Yawn-<version>-macos-arm64.app.tar.gz.sig
#
# tauri-plugin-updater downloads the tarball, verifies the .sig against the
# public key in tauri.conf.json, unpacks the first path component's contents
# over the installed Yawn.app, and Yawn relaunches. Tauri's own
# `createUpdaterArtifacts` would package the app at `tauri build` time, before
# sign-notarize.sh re-signs and notarizes it, so this runs afterwards on the
# exact bundle that was stapled.
#
# The signing key is the "Tauri yawn-app" item in 1Password's Developer
# Secrets vault (fields `credential` and `password`). TAURI_SIGNING_PRIVATE_KEY
# and TAURI_SIGNING_PRIVATE_KEY_PASSWORD, when set, are used instead.
#
# This is safe to re-run on its own: it never re-signs or re-notarizes the app.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KEY_ITEM="op://Developer Secrets/Tauri yawn-app"

die() { echo "build-update-artifact: $*" >&2; exit 1; }

APP="${1:-$ROOT/target/release/bundle/macos/Yawn.app}"
ADMISSION="${2:-internal-alpha}"
[[ -d "$APP" ]] || die "no app bundle at $APP"
[[ "$(basename "$APP")" == "Yawn.app" ]] || die "the bundle must be named Yawn.app; the updater installs it under that name"

VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")"
OUT="$ROOT/target/release/bundle/macos"
TARBALL="$OUT/Yawn-${VERSION}-macos-arm64.app.tar.gz"

# The update must carry the same notarized app the DMG does.
xcrun stapler validate "$APP" >/dev/null || die "$APP is not notarized and stapled; run sign-notarize.sh first"
spctl --assess --type execute "$APP" || die "Gatekeeper rejects $APP"

STAGE="$(mktemp -d /tmp/yawn-update-artifact.XXXXXX)"
cleanup() { rm -rf "$STAGE"; }
trap cleanup EXIT

echo "== packaging $TARBALL"
rm -f "$TARBALL" "$TARBALL.sig"
# COPYFILE_DISABLE keeps macOS tar from adding AppleDouble `._*` entries,
# which would land inside the installed bundle and break its code seal.
COPYFILE_DISABLE=1 tar -czf "$TARBALL" -C "$(dirname "$APP")" "$(basename "$APP")"
if tar -tzf "$TARBALL" | grep -q '/\._'; then
  die "the package contains AppleDouble files"
fi
[[ "$(tar -tzf "$TARBALL" | head -1 | cut -d/ -f1)" == "Yawn.app" ]] \
  || die "the package's top-level entry is not Yawn.app"

echo "== proving the package unpacks, as the updater unpacks it, into an app that still verifies"
# examples/unpack_like_updater.rs uses the updater's own archive crates and
# path handling; macOS tar is a different implementation.
(cd "$ROOT" && cargo run --quiet --release -p local-meeting-notes-desktop --example unpack_like_updater -- \
  "$TARBALL" "$STAGE/Yawn.app") || die "the package does not unpack the way the updater unpacks it"
codesign --verify --deep --strict "$STAGE/Yawn.app" || die "the unpacked app fails code-signature verification"
xcrun stapler validate "$STAGE/Yawn.app" >/dev/null || die "the unpacked app lost its notarization ticket"
spctl --assess --type execute "$STAGE/Yawn.app" || die "Gatekeeper rejects the unpacked app"
"$ROOT/scripts/verify-release-bundle.py" "$STAGE/Yawn.app" --signed --admission "$ADMISSION" \
  || die "the unpacked app fails release verification"

echo "== signing with the updater key"
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  TAURI_SIGNING_PRIVATE_KEY="$(op read "$KEY_ITEM/credential")" || die "could not read the updater key from 1Password"
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(op read "$KEY_ITEM/password")" || die "could not read the updater key password from 1Password"
fi
export TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD
signed=0
(cd "$ROOT/apps/desktop" && npx --no-install tauri signer sign "$TARBALL" >/dev/null) && signed=1
# Nothing after this needs the private key; keep it out of cargo and its
# build scripts.
unset TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD
[[ "$signed" == "1" ]] || die "tauri signer could not sign the package (does apps/desktop have node_modules?)"
[[ -s "$TARBALL.sig" ]] || die "no signature was written"

echo "== verifying the signature the way the updater will"
(cd "$ROOT" && cargo run --quiet --release -p local-meeting-notes-desktop --example verify_update_signature -- \
  "$TARBALL" "$TARBALL.sig" "$ROOT/apps/desktop/src-tauri/tauri.conf.json") \
  || die "the package's signature does not verify against tauri.conf.json's public key"

shasum -a 256 "$TARBALL" | tee "$TARBALL.sha256"
echo "DONE: $TARBALL"
