#!/usr/bin/env python3
"""Validate matched grouping attempts without exposing meeting content.

This offline helper reads four frozen attempt directories and writes one
content-free local receipt.  It never runs a model or copies transcript, note,
or audio text into its output.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import sys
from collections import Counter
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(ROOT), str(ROOT / "notes"), str(ROOT / "spike")]
DIGEST = re.compile(r"[0-9a-f]{64}\Z")
PLAN_SCHEMA = "speech-grouping-run-plan/1"
VARIANTS_SCHEMA = "speech-note-review-variants/1"
ATTEMPT_SCHEMA = "speech-note-replay-attempt/1"
PREFLIGHT_SCHEMA = "speech-grouping-preflight/1"
RECEIPT_SCHEMA = "speech-note-replay/1"
EXPECTED_CONDITIONS = {
    ("apple", "current"),
    ("apple", "bounded"),
    ("parakeet", "current"),
    ("parakeet", "bounded"),
}
RELEVANT_SOURCE_HASHES = {
    "spike/speech-engine-comparison/replay_notes.py",
    "spike/speech-engine-comparison/compare_segmentation.py",
    "worker/transcription.py",
    "worker/speech_results.py",
    "worker/note_validator.py",
    "worker/note_generator_mlx.py",
    "notes/candidate_first.py",
    "notes/transcript.py",
    "spike/dual_capture.py",
}


class GroupingPilotRefused(ValueError):
    pass


def sha256(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def _digest(value: object, label: str) -> str:
    if not isinstance(value, str) or not DIGEST.fullmatch(value):
        raise GroupingPilotRefused(f"{label} is not a SHA-256 digest")
    return value


def _number(value: object, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
        raise GroupingPilotRefused(f"{label} is not a non-negative finite number")
    return float(value)


def _load_json(path: Path, label: str) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        raise GroupingPilotRefused(f"{label} is missing or unsafe")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise GroupingPilotRefused(f"{label} is unreadable") from exc
    if not isinstance(value, dict):
        raise GroupingPilotRefused(f"{label} is not an object")
    return value


def _safe_child(root: Path, name: object, label: str) -> Path:
    if not isinstance(name, str) or not name or Path(name).name != name or name in {".", ".."}:
        raise GroupingPilotRefused(f"{label} is not a single safe directory name")
    child = root / name
    if child.is_symlink() or not child.is_dir():
        raise GroupingPilotRefused(f"{label} is missing or unsafe")
    return child


def _load_variants(run_root: Path) -> tuple[dict[str, Any], list[dict[str, str]], str]:
    path = run_root / "variants.json"
    document = _load_json(path, "variants manifest")
    if document.get("schema") != VARIANTS_SCHEMA:
        raise GroupingPilotRefused("variants manifest schema is invalid")
    raw = document.get("variants")
    if not isinstance(raw, list) or len(raw) != 4:
        raise GroupingPilotRefused("variants manifest must contain four conditions")
    variants: list[dict[str, str]] = []
    seen_directories: set[str] = set()
    seen_labels: set[str] = set()
    seen_conditions: set[tuple[str, str]] = set()
    for row in raw:
        if not isinstance(row, dict):
            raise GroupingPilotRefused("variant is not an object")
        directory, engine, grouping, label = row.get("directory"), row.get("engine"), row.get("grouping"), row.get("label")
        if not all(isinstance(value, str) and value for value in (directory, engine, grouping, label)):
            raise GroupingPilotRefused("variant lacks identity fields")
        if Path(directory).name != directory or directory in {".", ".."} or directory in seen_directories:
            raise GroupingPilotRefused("variant directory is unsafe or duplicated")
        if label != label.strip() or not label or label in seen_labels or any(ord(character) < 32 for character in label):
            raise GroupingPilotRefused("variant label is unsafe or duplicated")
        condition = (engine, grouping)
        if condition not in EXPECTED_CONDITIONS or condition in seen_conditions:
            raise GroupingPilotRefused("variant engine/grouping set is invalid")
        seen_directories.add(directory)
        seen_labels.add(label)
        seen_conditions.add(condition)
        variants.append({"directory": directory, "engine": engine, "grouping": grouping, "label": label})
    if seen_conditions != EXPECTED_CONDITIONS:
        raise GroupingPilotRefused("variants do not form the matched condition set")
    return document, variants, sha256(path)


def _request_metrics(value: object) -> tuple[int, dict[str, int]]:
    if not isinstance(value, list):
        raise GroupingPilotRefused("receipt note_requests is invalid")
    kinds: Counter[str] = Counter()
    for request in value:
        if not isinstance(request, dict) or not isinstance(request.get("kind"), str):
            raise GroupingPilotRefused("receipt request is invalid")
        _number(request.get("seconds"), "receipt request seconds")
        kinds[request["kind"]] += 1
    return len(value), dict(sorted(kinds.items()))


def _canonical_claim_count(generated: dict[str, Any], transcript_path: Path) -> int:
    from worker.note_validator import validate_locators
    from transcript import load

    claims = generated.get("claims")
    if not isinstance(claims, list):
        raise GroupingPilotRefused("generated claims are invalid")
    try:
        transcript = load(transcript_path)
    except Exception as exc:
        raise GroupingPilotRefused("transcript is invalid") from exc
    for claim in claims:
        if not isinstance(claim, dict) or not isinstance(claim.get("locators"), list):
            raise GroupingPilotRefused("generated claim locators are invalid")
        refs = [
            {
                "turn": locator.get("turn") if isinstance(locator, dict) else None,
                "char_start": locator.get("start") if isinstance(locator, dict) else None,
                "char_end": locator.get("end") if isinstance(locator, dict) else None,
                "text_sha256": locator.get("text_sha256") if isinstance(locator, dict) else None,
            }
            for locator in claim["locators"]
        ]
        try:
            validate_locators(refs, transcript)
        except Exception as exc:
            raise GroupingPilotRefused("generated claim locator does not resolve") from exc
    return len(claims)


def _source_hashes(document: dict[str, Any], label: str) -> dict[str, str]:
    values = document.get("source_code_hashes")
    if not isinstance(values, dict) or set(values) != RELEVANT_SOURCE_HASHES:
        raise GroupingPilotRefused(f"{label} source-code identities are invalid")
    return {name: _digest(values[name], f"{label} source hash {name}") for name in sorted(RELEVANT_SOURCE_HASHES)}


def _hash_list(value: object, label: str, expected_count: int) -> list[str]:
    if not isinstance(value, list) or len(value) != expected_count:
        raise GroupingPilotRefused(f"{label} is invalid")
    return [_digest(item, label) for item in value]


def _source_snapshot(attempt: dict[str, Any]) -> dict[str, Any]:
    before, after = attempt.get("source_hashes_before"), attempt.get("source_hashes_after")
    if not isinstance(before, dict) or not before or before != after:
        raise GroupingPilotRefused("outer attempt source snapshot changed or is invalid")
    if attempt.get("source_unchanged") is not True:
        raise GroupingPilotRefused("outer attempt reports changed source bytes")
    rehashed, unavailable = 0, 0
    audio_names = {"mic.wav", "system.wav", "session.json"}
    checked_audio: set[str] = set()
    for label, value in before.items():
        if not isinstance(label, str):
            raise GroupingPilotRefused("outer attempt source snapshot key is invalid")
        _digest(value, "outer attempt source hash")
        path = Path(label)
        if path.is_file() and not path.is_symlink():
            if sha256(path) != value:
                raise GroupingPilotRefused("current referenced source bytes do not match outer snapshot")
            rehashed += 1
            if path.name in audio_names:
                checked_audio.add(path.name)
        else:
            unavailable += 1
    if checked_audio and checked_audio != audio_names:
        raise GroupingPilotRefused("current referenced capture audio snapshot is incomplete")
    return {
        "sha256": hashlib.sha256(json.dumps(before, sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
        "rehashed_files": rehashed,
        "unavailable_files": unavailable,
        "audio_revalidated": checked_audio == audio_names,
    }


def _validate_file_binding(value: object, label: str) -> None:
    if not isinstance(value, dict):
        raise GroupingPilotRefused(f"{label} binding is invalid")
    path_value, expected_sha, expected_bytes = value.get("path"), value.get("sha256"), value.get("bytes")
    if not isinstance(path_value, str) or not isinstance(expected_bytes, int) or isinstance(expected_bytes, bool) or expected_bytes < 0:
        raise GroupingPilotRefused(f"{label} binding is invalid")
    expected_sha = _digest(expected_sha, f"{label} SHA-256")
    path = Path(path_value)
    if path.is_symlink() or not path.is_file() or path.stat().st_size != expected_bytes or sha256(path) != expected_sha:
        raise GroupingPilotRefused(f"{label} current bytes do not match preflight")


def _validate_native_preflight(preflight: dict[str, Any], engine: str) -> None:
    native = preflight.get("native")
    legs = native.get(engine) if isinstance(native, dict) else None
    if not isinstance(legs, dict) or set(legs) != {"mic", "system"}:
        raise GroupingPilotRefused("preflight native bindings are invalid")
    for leg in ("mic", "system"):
        row = legs[leg]
        if not isinstance(row, dict) or row.get("receipt_status") != "ok" or row.get("receipt_source_unchanged") is not True:
            raise GroupingPilotRefused("preflight native receipt binding is invalid")
        _validate_file_binding(row.get("result"), f"preflight {engine} {leg} result")
        _validate_file_binding(row.get("receipt"), f"preflight {engine} {leg} receipt")


def _successful_condition(
    run_root: Path,
    variant: dict[str, str],
    attempt: dict[str, Any],
    attempt_path: Path,
    plan_hash: str,
    variants_hash: str,
) -> dict[str, Any]:
    directory = _safe_child(run_root, variant["directory"], "condition directory")
    if attempt.get("directory") != variant["directory"]:
        raise GroupingPilotRefused("outer attempt directory does not match manifest")
    receipt_path = directory / "receipt.json"
    receipt = _load_json(receipt_path, "replay receipt")
    if receipt.get("schema") != RECEIPT_SCHEMA:
        raise GroupingPilotRefused("replay receipt schema is invalid")
    if attempt.get("receipt_sha256") != sha256(receipt_path):
        raise GroupingPilotRefused("outer attempt receipt hash does not match")
    for identity in (attempt, receipt):
        if identity.get("engine") != variant["engine"] or identity.get("grouping") != variant["grouping"]:
            raise GroupingPilotRefused("attempt or receipt engine/grouping does not match manifest")
    if attempt.get("returncode") != 0 or attempt.get("timed_out") is not False:
        raise GroupingPilotRefused("successful attempt status is invalid")
    outer_source_snapshot = _source_snapshot(attempt)
    preflight_path = directory / "preflight.json"
    preflight = _load_json(preflight_path, "grouping preflight")
    if (
        preflight.get("schema") != PREFLIGHT_SCHEMA
        or attempt.get("preflight_sha256") != sha256(preflight_path)
        or preflight.get("run_plan_sha256") != plan_hash
        or preflight.get("variants_sha256") != variants_hash
    ):
        raise GroupingPilotRefused("outer attempt preflight binding is invalid")
    if receipt.get("status") != "validated-note-output" or receipt.get("source_unchanged") is not True:
        raise GroupingPilotRefused("replay receipt is not an unchanged validated note")
    generated_path = directory / "generated.json"
    generated = _load_json(generated_path, "generated note")
    if receipt.get("generated_sha256") != sha256(generated_path):
        raise GroupingPilotRefused("receipt generated hash does not match")
    transcript_id = _digest(receipt.get("transcript_id"), "receipt transcript id")
    transcript_path = directory / "meetings" / "research" / "transcript" / f"{transcript_id}.json"
    if transcript_path.is_symlink() or not transcript_path.is_file() or sha256(transcript_path) != transcript_id:
        raise GroupingPilotRefused("receipt transcript identity does not match")
    if generated.get("transcript_sha256") != transcript_id:
        raise GroupingPilotRefused("generated note does not bind the replay transcript")
    claims = _canonical_claim_count(generated, transcript_path)
    if receipt.get("claims") != claims:
        raise GroupingPilotRefused("receipt claim count does not match generated note")
    note_model = receipt.get("note_model")
    if not isinstance(note_model, dict) or not isinstance(note_model.get("revision"), str) or not note_model["revision"]:
        raise GroupingPilotRefused("receipt note model revision is invalid")
    request_count, request_kinds = _request_metrics(receipt.get("note_requests"))
    source_audio = _hash_list(receipt.get("source_audio_hashes"), "receipt audio hashes", 3)
    speech_sources = _hash_list(receipt.get("speech_source_hashes"), "receipt ASR source hashes", 2)
    code_hashes = _source_hashes(receipt, "receipt")
    preflight_model = preflight.get("note_model")
    if not isinstance(preflight_model, dict) or preflight_model.get("revision") != note_model["revision"]:
        raise GroupingPilotRefused("preflight model revision does not match receipt")
    preflight_code = _source_hashes(preflight, "preflight")
    if preflight_code != code_hashes:
        raise GroupingPilotRefused("preflight source-code identities do not match receipt")
    _validate_native_preflight(preflight, variant["engine"])
    return {
        "label": variant["label"],
        "engine": variant["engine"],
        "grouping": variant["grouping"],
        "status": "available",
        "elapsed_seconds": _number(attempt.get("elapsed_seconds"), "attempt elapsed seconds"),
        "setup_seconds": _number(receipt.get("note_setup_seconds"), "receipt setup seconds"),
        "generation_seconds": _number(receipt.get("note_generation_seconds"), "receipt generation seconds"),
        "note_request_count": request_count,
        "note_requests_by_kind": request_kinds,
        "process_peak_rss_bytes": _number(receipt.get("process_peak_rss_bytes"), "receipt peak RSS"),
        "peak_rss_scope": "process-wide replay-process maximum RSS as reported by producer",
        "validated_claim_count": claims,
        "receipt_sha256": sha256(receipt_path),
        "generated_sha256": sha256(generated_path),
        "transcript_sha256": transcript_id,
        "source_audio_hashes": source_audio,
        "speech_source_hashes": speech_sources,
        "source_code_hashes": code_hashes,
        "note_model_revision": note_model["revision"],
        "attempt_sha256": sha256(attempt_path),
        "preflight_sha256": sha256(preflight_path),
        "outer_source_unchanged": True,
        "outer_source_snapshot_sha256": outer_source_snapshot["sha256"],
        "outer_source_snapshot_rehashed_files": outer_source_snapshot["rehashed_files"],
        "outer_source_snapshot_unavailable_files": outer_source_snapshot["unavailable_files"],
        "audio_snapshot_revalidated": outer_source_snapshot["audio_revalidated"],
    }


def _unavailable_condition(variant: dict[str, str], attempt: dict[str, Any], attempt_path: Path) -> dict[str, Any]:
    if (
        attempt.get("engine") != variant["engine"]
        or attempt.get("grouping") != variant["grouping"]
        or attempt.get("directory") != variant["directory"]
    ):
        raise GroupingPilotRefused("unavailable attempt engine/grouping does not match manifest")
    snapshot = _source_snapshot(attempt)
    timed_out = attempt.get("timed_out")
    status = attempt.get("status")
    receipt_status = attempt.get("receipt_status")
    if timed_out is True:
        reason = "timeout"
    elif receipt_status == "transcript-only":
        reason = "refusal"
    elif status in {"failed", "refused"} or attempt.get("returncode") != 0:
        reason = "failed"
    else:
        raise GroupingPilotRefused("unavailable attempt status is invalid")
    return {
        "label": variant["label"],
        "engine": variant["engine"],
        "grouping": variant["grouping"],
        "status": "unavailable",
        "unavailable_reason": reason,
        "elapsed_seconds": _number(attempt.get("elapsed_seconds"), "attempt elapsed seconds"),
        "attempt_sha256": sha256(attempt_path),
        "outer_source_unchanged": True,
        "outer_source_snapshot_sha256": snapshot["sha256"],
        "outer_source_snapshot_rehashed_files": snapshot["rehashed_files"],
        "outer_source_snapshot_unavailable_files": snapshot["unavailable_files"],
        "audio_snapshot_revalidated": snapshot["audio_revalidated"],
    }


def _compare(conditions: list[dict[str, Any]]) -> list[dict[str, Any]]:
    by_condition = {(row["engine"], row["grouping"]): row for row in conditions}
    comparisons = []
    for engine in ("apple", "parakeet"):
        current, bounded = by_condition[(engine, "current")], by_condition[(engine, "bounded")]
        if current["status"] != "available" or bounded["status"] != "available":
            comparisons.append({"engine": engine, "status": "unavailable"})
            continue
        comparisons.append({
            "engine": engine,
            "status": "available",
            "bounded_minus_current": {
                key: bounded[key] - current[key]
                for key in (
                    "elapsed_seconds", "setup_seconds", "generation_seconds",
                    "note_request_count", "process_peak_rss_bytes", "validated_claim_count",
                )
            },
        })
    return comparisons


def summarize(run_root: Path, output: Path) -> dict[str, Any]:
    run_root = run_root.resolve()
    if output.exists() or output.is_symlink():
        raise GroupingPilotRefused("summary output already exists")
    plan_path = run_root / "run-plan.json"
    plan = _load_json(plan_path, "run plan")
    if plan.get("schema") != PLAN_SCHEMA or plan.get("human_review") != "pending":
        raise GroupingPilotRefused("run plan schema or review state is invalid")
    _manifest, variants, manifest_hash = _load_variants(run_root)
    plan_hash = sha256(plan_path)
    conditions = []
    for variant in variants:
        attempt_path = run_root / f"{variant['directory']}.attempt.json"
        if attempt_path.is_symlink() or not attempt_path.is_file():
            conditions.append({
                "label": variant["label"],
                "engine": variant["engine"],
                "grouping": variant["grouping"],
                "status": "unavailable",
                "unavailable_reason": "missing_attempt",
            })
            continue
        attempt = _load_json(attempt_path, "outer attempt")
        if attempt.get("schema") != ATTEMPT_SCHEMA:
            raise GroupingPilotRefused("outer attempt schema is invalid")
        if attempt.get("returncode") == 0 and attempt.get("timed_out") is False and attempt.get("receipt_status") == "validated-note-output":
            conditions.append(_successful_condition(run_root, variant, attempt, attempt_path, plan_hash, manifest_hash))
        else:
            conditions.append(_unavailable_condition(variant, attempt, attempt_path))
    available = [row for row in conditions if row["status"] == "available"]
    if available:
        baseline = available[0]
        for row in available[1:]:
            for key in ("note_model_revision", "source_audio_hashes", "source_code_hashes"):
                if row[key] != baseline[key]:
                    raise GroupingPilotRefused(f"matched source identity mismatch: {key}")
        for engine in ("apple", "parakeet"):
            engine_rows = [row for row in available if row["engine"] == engine]
            if len(engine_rows) == 2 and engine_rows[0]["speech_source_hashes"] != engine_rows[1]["speech_source_hashes"]:
                raise GroupingPilotRefused("matched source identity mismatch: speech_source_hashes")
    document = {
        "schema": "speech-grouping-pilot-summary/1",
        "frozen_run_plan_sha256": plan_hash,
        "frozen_variants_manifest_sha256": manifest_hash,
        "local_checker_sha256": sha256(Path(__file__).resolve()),
        "producer_replay_notes_sha256": (
            available[0]["source_code_hashes"]["spike/speech-engine-comparison/replay_notes.py"]
            if available else None
        ),
        "conditions": conditions,
        "engine_comparisons": _compare(conditions),
        "human_review": "pending",
        "adopted_change": False,
        "limitations": [
            "raw per-condition receipt diagnostic; no model or source data was changed by this helper",
            "one sample per condition cannot establish a causal speed improvement",
            "validated claim count is a diagnostic only and is not a quality score",
            "unavailable timeout, refusal, and failed attempts are preserved and excluded from signed deltas",
            "no missing-commitment, negation, or meaning conclusion is inferred from counts or string overlap",
            "outer source snapshots bind before/after hashes; only snapshot entries with resolvable local paths are rehashed here, while capture rehash remains a separate parent check",
        ],
    }
    previous_umask = os.umask(0o077)
    try:
        output.parent.mkdir(parents=True, exist_ok=True)
        with output.open("x", encoding="utf-8") as handle:
            json.dump(document, handle, sort_keys=True, separators=(",", ":"))
            handle.write("\n")
        output.chmod(0o600)
    finally:
        os.umask(previous_umask)
    return document


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-root", type=Path, required=True)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    output = args.out or args.run_root / "summary.json"
    document = summarize(args.run_root, output)
    print(json.dumps({"schema": document["schema"], "conditions": len(document["conditions"])}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
