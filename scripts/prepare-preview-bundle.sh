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
PYTHON_ENTITLEMENTS="$ROOT/apps/desktop/src-tauri/python-entitlements.plist"
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
  # The bundled runtime carries the source stamp written at staging time.
  # Checking it here — before signing mutates the bundle — is what refuses the
  # Aug-19-helper-under-a-Sep-1-app class of bundle instead of sealing it.
  python3 "$ROOT/worker/source_digest.py" check "$RESOURCES" \
    || die "bundled runtime failed the source-freshness gate (see above); run worker/build_runtime.sh, then rebuild the bundle"
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
  if [[ "$identity" != "-" ]]; then
    # A Developer ID identity is present, so give the preview the release
    # lane's nested signing (scripts/sign-notarize.sh minus the Apple
    # submission): every Mach-O hardened and timestamped, the interpreter
    # under the bundle-derived identifier with the Python entitlements.
    # SecurityCodeVerifier admits the note generator only against that
    # shape; an ad-hoc bundle cannot generate a note by design
    # (docs/note-runtime-decision.md, "Running a local build requires the
    # release lane's signing stage").
    local bundle_id python_identifier machos count
    bundle_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Contents/Info.plist")"
    python_identifier="${bundle_id}.python-runtime"
    machos="$(mktemp)"
    find "$APP" -type f -print0 \
      | while IFS= read -r -d '' path; do
          case "$(file -b "$path")" in Mach-O*) printf '%s\0' "$path" ;; esac
        done > "$machos" || true
    count=0
    while IFS= read -r -d '' path; do
      local sign_args=(--force --options runtime --timestamp --sign "$identity")
      if [[ "$path" == "$RESOURCES/python-runtime/bin/python3.12" ]]; then
        sign_args+=(--identifier "$python_identifier" --entitlements "$PYTHON_ENTITLEMENTS")
      elif [[ "$path" == "$MAIN" || "$path" == "$CAPTURE" || "$path" == "$PROBE" ]]; then
        sign_args+=(--entitlements "$ENTITLEMENTS")
      fi
      codesign "${sign_args[@]}" "$path"
      count=$((count + 1))
    done < "$machos"
    rm -f "$machos"
    [[ "$count" -gt 0 ]] || die "Preview app contains no Mach-O files"
    echo "prepare-preview-bundle: $count Mach-O files signed with $identity"
  else
    codesign --force --sign "$identity" --entitlements "$ENTITLEMENTS" "$CAPTURE"
    codesign --force --sign "$identity" --entitlements "$ENTITLEMENTS" "$PROBE"
  fi
  manifest_args=()
  local runtime_schema
  runtime_schema="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["schema"])' \
      "$RESOURCES/app-runtime.json")"
  if [[ "$runtime_schema" == "app-runtime/2" || "$runtime_schema" == "app-runtime/3" ]]; then
    manifest_args+=(--external-transcript-models)
  fi
  if [[ "$runtime_schema" == "app-runtime/3" ]]; then
    manifest_args+=(--apple-speech)
  fi
  "$RESOURCES/python-runtime/bin/python3.12" -E -s -B \
    "$ROOT/worker/build_manifest.py" "$RESOURCES" --admission internal-alpha \
    ${manifest_args[@]+"${manifest_args[@]}"}
  # Sign the enclosing app last. Signing CFBundleExecutable as a standalone
  # path first makes it seal the surrounding bundle; replacing the outer
  # signature afterward then invalidates that inner resource seal.
  if [[ "$identity" != "-" ]]; then
    codesign --force --options runtime --timestamp --sign "$identity" --entitlements "$ENTITLEMENTS" "$APP"
  else
    codesign --force --sign "$identity" --entitlements "$ENTITLEMENTS" "$APP"
  fi
  verify_bundle
}

require_bundle
case "${1:-sign}" in
  sign) sign_bundle ;;
  verify) verify_bundle ;;
  *) die "usage: prepare-preview-bundle.sh [sign|verify]" ;;
esac
