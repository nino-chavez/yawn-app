#!/usr/bin/env bash
# Give a locally packaged capture app the same microphone entitlement boundary
# as the installed alpha, without notarizing or promoting it as a release. The
# historical LMN_PREVIEW_APP override remains accepted for existing callers.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="${LMN_BUNDLE_APP:-${LMN_PREVIEW_APP:-$ROOT/target/release/bundle/macos/Yawn Preview.app}}"
RESOURCES="$APP/Contents/Resources"
MAIN="$APP/Contents/MacOS/local-meeting-notes-desktop"
CAPTURE="$RESOURCES/bin/meeting-capture"
PROBE="$RESOURCES/bin/permission-probe"
ENTITLEMENTS="$ROOT/apps/desktop/src-tauri/capture-entitlements.plist"
EXPECTED_TEAM_ID="34VZ63G58M"
REQUIRED_NOTE_RUNTIME_RESOURCES=(
  "note-bridge.py"
  "note-generator-mlx.py"
  "note-runtime-generate.json"
  "note-runtime-project.json"
  "note-validator.zip"
)

die() {
  echo "prepare-preview-bundle: $*" >&2
  exit 1
}

require_bundle() {
  [[ -d "$APP" ]] || die "Preview app is missing: $APP"
  [[ -x "$MAIN" ]] || die "Preview app executable is missing"
  [[ -x "$CAPTURE" ]] || die "Preview meeting-capture helper is missing"
  [[ -x "$PROBE" ]] || die "Preview permission-probe helper is missing"
  [[ -x "$RESOURCES/python-runtime/bin/python3.12" ]] \
    || die "Preview Python runtime is missing"
  [[ -f "$ENTITLEMENTS" ]] || die "capture entitlements are missing"
}

has_audio_input_entitlement() {
  codesign -d --entitlements :- "$1" 2>/dev/null \
    | plutil -extract 'com\.apple\.security\.device\.audio-input' raw -o - - \
    | grep -qx true
}

require_note_runtime_complete() {
  local resource path
  for resource in "${REQUIRED_NOTE_RUNTIME_RESOURCES[@]}"; do
    path="$RESOURCES/$resource"
    [[ -f "$path" && ! -L "$path" ]] \
      || die "Preview bundle is missing a note runtime resource: $resource"
  done
}

verify_bundle() {
  require_note_runtime_complete
  codesign --verify --deep --strict "$APP"
  has_audio_input_entitlement "$APP" \
    || die "Preview app is missing the audio-input entitlement"
  has_audio_input_entitlement "$MAIN" \
    || die "Preview executable is missing the audio-input entitlement"
  has_audio_input_entitlement "$CAPTURE" \
    || die "Preview meeting-capture helper is missing the audio-input entitlement"
  # The probe is a requesting binary too: it calls AVCaptureDevice.requestAccess.
  has_audio_input_entitlement "$PROBE" \
    || die "Preview permission-probe helper is missing the audio-input entitlement"
}

sign_bundle() {
  local identity
  identity="$(
    security find-identity -v -p codesigning 2>/dev/null \
      | awk -F'"' -v team="($EXPECTED_TEAM_ID)" \
          '/Developer ID Application/ && index($2, team) {print $2; exit}'
  )"
  [[ -n "$identity" ]] || identity="-"

  # Finder/Gatekeeper provenance attributes are not product content, but they
  # make strict verification report an otherwise byte-identical bundle as
  # modified. Remove bundle metadata before creating the final code seals.
  xattr -cr "$APP"
  codesign --force --sign "$identity" --entitlements "$ENTITLEMENTS" "$CAPTURE"
  codesign --force --sign "$identity" --entitlements "$ENTITLEMENTS" "$PROBE"
  manifest_args=()
  if [[ "$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["schema"])' \
      "$RESOURCES/app-runtime.json")" == "app-runtime/2" ]]; then
    manifest_args+=(--external-transcript-models)
  fi
  "$RESOURCES/python-runtime/bin/python3.12" -E -s -B \
    "$ROOT/worker/build_manifest.py" "$RESOURCES" --admission internal-alpha \
    ${manifest_args[@]+"${manifest_args[@]}"}
  # Sign the enclosing app last. Signing CFBundleExecutable as a standalone
  # path first makes it seal the surrounding bundle; replacing the outer
  # signature afterward then invalidates that inner resource seal.
  codesign --force --sign "$identity" --entitlements "$ENTITLEMENTS" "$APP"
  verify_bundle
}

require_bundle
case "${1:-sign}" in
  sign) sign_bundle ;;
  verify) verify_bundle ;;
  *) die "usage: prepare-preview-bundle.sh [sign|verify]" ;;
esac
