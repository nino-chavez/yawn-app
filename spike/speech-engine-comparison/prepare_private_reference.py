#!/usr/bin/env python3
"""Freeze a private, time-anchored proxy for an existing Yawn event review.

This prepares comparison metadata only. It never loads a model, changes a
ledger, or promotes an agent-authored reference to human approval.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from pathlib import Path


HERE = Path(__file__).resolve()
NOTES = HERE.parents[2] / "notes"
sys.path.insert(0, str(NOTES))


def _sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _json(path: Path) -> dict | list:
    return json.loads(path.read_text(encoding="utf-8"))


def _find_cycle_file(cycle: Path, name: str) -> Path:
    for candidate in (cycle / name, cycle.parent / name):
        if candidate.is_file():
            return candidate
    raise SystemExit(f"cycle file is missing: {name} (checked {cycle} and its parent)")


def _lock_validation(cycle: Path, lock_sha256: str, reference: dict, manifest: dict) -> tuple[bool, str]:
    roots = (cycle, cycle.parent, cycle.parent.parent)
    for root in roots:
        for name in ("product-lock.pending.json", "product-lock.json"):
            lock = root / name
            if not lock.is_file():
                continue
            raw = lock.read_bytes()
            if _sha256_bytes(raw) != lock_sha256:
                continue
            data = json.loads(raw)
            expected_ledger = reference.get("ledger_sha256")
            if expected_ledger is not None and data.get("ledger_sha256") != expected_ledger:
                return False, "lock bytes do not bind the supplied ledger"
            expected_manifest = manifest.get("manifest_sha256")
            if expected_manifest is not None and data.get("manifest_sha256") != expected_manifest:
                return False, "lock bytes do not bind the supplied manifest"
            return True, "validated lock bytes and input bindings"
    for root in roots:
        approval = root / "LOCK-APPROVAL.md"
        if approval.is_file() and lock_sha256 in approval.read_text(encoding="utf-8"):
            return False, "lock SHA appears in prose approval, but lock bytes are absent"
    return False, "approved lock SHA is not recorded in a lock file or approval receipt"


def _terms(text: str) -> list[str]:
    from summarize import _NOTE_REGISTER, _words

    return sorted(_words(text) - _NOTE_REGISTER)


def _reference_rows(reference: dict, transcript: object, raw_turns: list[dict]) -> tuple[list[dict], bool]:
    rows: list[dict] = []
    turns = transcript.turns
    used_fallback = False

    def turn_end(index: int) -> float:
        nonlocal used_fallback
        raw = raw_turns[index]
        if isinstance(raw, dict) and raw.get("end") is not None:
            return float(raw["end"])
        used_fallback = True
        # Older loader inputs may lack explicit ends. Keep the fallback
        # visible in the output instead of silently presenting inferred bounds.
        if index + 1 < len(turns) and turns[index + 1].start is not None:
            return float(turns[index + 1].start)
        health = transcript.capture_health or {}
        timing = health.get("timing", {}) if isinstance(health, dict) else {}
        elapsed = timing.get("capture_elapsed_s")
        if elapsed is None:
            raise SystemExit("transcript has no end bound for its final turn")
        return float(elapsed)

    for event in reference["events"]:
        for bundle in event["evidence_bundles"]:
            evidence = bundle["evidence"]
            if not evidence:
                raise SystemExit(f"event {event['event_id']} has an empty evidence bundle")
            ordinals = [row["clean_turn_ordinal"] for row in evidence]
            if any(not isinstance(value, int) or value < 0 or value >= len(turns) for value in ordinals):
                raise SystemExit(f"event {event['event_id']} has an out-of-range turn anchor")
            start = min(float(turns[index].start) for index in ordinals)
            end = max(turn_end(index) for index in ordinals)
            excerpt = " ".join(row["text"] for row in evidence)
            rows.append({
                "id": f"{event['event_id']}:{bundle['bundle_sha256']}",
                "kind": event["kind"],
                "start": start,
                "end": end,
                "text": excerpt,
                "terms": _terms(excerpt),
                "event_id": event["event_id"],
                "bundle_sha256": bundle["bundle_sha256"],
                "anchor_clean_turn_ordinal": bundle["anchor_clean_turn_ordinal"],
            })
    return rows, used_fallback


def prepare(transcript_path: Path, cycle: Path, lock_sha256: str, out: Path) -> dict:
    from candidate_exposure import (
        validate_manifest,
        validate_reference,
    )
    from capture_exposure import capture_review_view, empty_regions, identity_coordinates
    from candidate_first import _sha256
    from transcript import load

    if not re.fullmatch(r"[0-9a-f]{64}", lock_sha256):
        raise SystemExit("approved lock SHA must be 64 lowercase hexadecimal characters")
    reference_path = _find_cycle_file(cycle, "product-reference.json")
    if not reference_path.is_file():
        reference_path = _find_cycle_file(cycle, "capture-exposure-reference.json")
    manifest_path = _find_cycle_file(cycle, "product-manifest.json")
    if not manifest_path.is_file():
        manifest_path = _find_cycle_file(cycle, "capture-candidate-manifest.json")
    transcript = load(transcript_path)
    raw_transcript = _json(transcript_path)
    raw_turns = raw_transcript.get("turns") if isinstance(raw_transcript, dict) else None
    if not isinstance(raw_turns, list) or len(raw_turns) != len(transcript.turns):
        raise SystemExit("source transcript JSON has no aligned turns")
    raw_sha256 = _sha256(transcript_path.read_bytes())
    reference = _json(reference_path)
    manifest = _json(manifest_path)
    view = capture_review_view(transcript)
    manifest = validate_manifest(manifest, view)
    coordinates = identity_coordinates(view)
    validate_reference(
        reference, view, manifest, coordinates, empty_regions(),
        enforce_registered=False,
        registration_sha256=reference["source"]["registration_sha256"],
    )
    if reference["source"]["corpus_sha256"] != raw_sha256:
        raise SystemExit("reference corpus digest does not match the supplied transcript")
    if reference["source"]["manifest_sha256"] != manifest["manifest_sha256"]:
        raise SystemExit("reference manifest digest does not match the supplied manifest")
    lock_validated, lock_validation = _lock_validation(cycle, lock_sha256, reference, manifest)
    original_text = "\n".join(turn.text for turn in transcript.turns)
    important_spans, timing_fallback = _reference_rows(reference, transcript, raw_turns)
    result = {
        "schema": "speech-reference/1",
        "kind": "event-time-proxy-not-human-ASR-reference",
        "reference_text": original_text,
        "important_spans": important_spans,
        "event_reference_sha256": reference["reference_sha256"],
        "review_reference_sha256": reference["reference_sha256"],
        "approved_lock_sha256": lock_sha256,
        "original_transcript_sha256": raw_sha256,
        "source_manifest_sha256": manifest["manifest_sha256"],
        "original_lock_validated": lock_validated,
        "original_lock_validation": lock_validation,
        "timing_used_fallback": timing_fallback,
        "counts": {
            "events": len(reference["events"]),
            "alternative_witness_spans": sum(len(event["evidence_bundles"]) for event in reference["events"]),
            "transcript_turns": len(transcript.turns),
        },
        "approval": {
            "reference_status": reference["status"],
            "human_approval": reference["human_approval"],
            "fresh_transcript_approval": False,
        },
        "limitations": [
            "The original Whisper transcript is a time-anchored proxy, not human ASR truth.",
            "Term survival is lexical event support, not semantic equivalence or note recall.",
            "Each evidence bundle is an alternative witness; spans are never merged across bundles.",
            "This helper does not create or imply fresh human approval for a changed engine transcript.",
        ],
    }
    os.umask(0o077)
    out.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    out.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    out.chmod(0o600)
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--transcript", required=True, type=Path)
    parser.add_argument("--cycle", required=True, type=Path)
    parser.add_argument("--approved-lock-sha256", required=True)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    result = prepare(args.transcript, args.cycle, args.approved_lock_sha256, args.out)
    print(json.dumps({"schema": result["schema"], "counts": result["counts"], "out": str(args.out)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
