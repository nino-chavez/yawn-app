#!/usr/bin/env python3
"""Prepare a private, unanswered review packet from saved note replays."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from pathlib import Path


VARIANTS = ("whisper-notes-short", "apple-notes-short", "parakeet-notes-short")
LABELS = {name: f"Variant {chr(65 + index)}" for index, name in enumerate(VARIANTS)}
VARIANT_SCHEMA = "speech-note-review-variants/1"
ROOT = Path.home() / "Library/Application Support/yawn-research/transcription-engine-comparison-2026-09-07"
ORIGINAL_TRANSCRIPT = Path("/Users/nino/Library/Application Support/com.ninochavez.local-meeting-notes/meetings/fdd59c81-30d7-480b-95d1-3753b9bad28e/transcript/b69f01c0de7d3c4de1f3c03a00f7fed9f288df761ed342c8b18886f078c843ef.json")


class ReviewRefused(ValueError):
    pass


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise ReviewRefused(f"invalid JSON: {path}") from exc
    if not isinstance(value, dict):
        raise ReviewRefused(f"JSON object required: {path}")
    return value


def _safe_digest(value: object, label: str) -> str:
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value):
        raise ReviewRefused(f"{label} is not a SHA-256 digest")
    return value


def _canonical_locators(claim: dict, transcript: object) -> list[dict]:
    """Use worker.note_validator's source-span rules without accepting output."""
    from note_validator import validate_locators

    refs = []
    for locator in claim.get("locators", []):
        if not isinstance(locator, dict):
            raise ReviewRefused("claim locator is not an object")
        refs.append({
            "turn": locator.get("turn"),
            "char_start": locator.get("start"),
            "char_end": locator.get("end"),
            "text_sha256": locator.get("text_sha256"),
        })
    try:
        return validate_locators(refs, transcript)
    except Exception as exc:  # canonical validator uses content-free failures
        raise ReviewRefused("claim locator does not resolve to its transcript") from exc


def validate_variant(root: Path, variant: str, spec: dict | None = None) -> dict:
    if spec is None and variant not in VARIANTS:
        raise ReviewRefused(f"unsupported variant: {variant}")
    directory_name = spec["directory"] if spec else variant
    expected_engine = spec["engine"] if spec else variant.removesuffix("-notes-short")
    expected_grouping = spec.get("grouping") if spec else None
    label = spec["label"] if spec else LABELS[variant]
    directory = root / directory_name
    receipt_path = directory / "receipt.json"
    generated_path = directory / "generated.json"
    receipt = load_json(receipt_path)
    generated = load_json(generated_path)
    if receipt.get("schema") != "speech-note-replay/1" or receipt.get("status") != "validated-note-output":
        raise ReviewRefused(f"receipt is not a validated note output: {variant}")
    if generated.get("schema") != "note-generation/2":
        raise ReviewRefused(f"generated note has the wrong schema: {variant}")
    generated_digest = sha256(generated_path)
    if generated_digest != _safe_digest(receipt.get("generated_sha256"), "generated_sha256"):
        raise ReviewRefused(f"receipt/generated hash mismatch: {variant}")
    transcript_id = _safe_digest(receipt.get("transcript_id"), "transcript_id")
    if generated.get("transcript_sha256") != transcript_id:
        raise ReviewRefused(f"generated note names a different transcript: {variant}")
    transcript_path = Path(receipt.get("transcript_path", ""))
    if not transcript_path.is_file() or sha256(transcript_path) != transcript_id:
        raise ReviewRefused(f"transcript hash/path mismatch: {variant}")

    import sys
    sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "notes"))
    sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "worker"))
    from transcript import load

    transcript = load(transcript_path)
    attempt_path = root / f"{variant}.attempt.json"
    attempt = load_json(attempt_path)
    if spec is not None and (
        attempt.get("schema") != "speech-note-replay-attempt/1"
        or attempt.get("directory") != directory_name
        or attempt.get("timed_out") is not False
        or attempt.get("source_unchanged") is not True
        or attempt.get("receipt_sha256") != sha256(receipt_path)
    ):
        raise ReviewRefused(f"attempt receipt binding failed for {directory_name}")
    if attempt.get("engine") != expected_engine or attempt.get("returncode") != 0:
        raise ReviewRefused(f"attempt did not complete for {directory_name}")
    if expected_grouping is not None and attempt.get("grouping") != expected_grouping:
        raise ReviewRefused(f"attempt grouping disagrees for {directory_name}")
    if receipt.get("source_unchanged") is not True:
        raise ReviewRefused(f"receipt does not verify unchanged sources: {variant}")
    if receipt.get("engine") != expected_engine or receipt.get("claims") != len(generated.get("claims", [])):
        raise ReviewRefused(f"receipt engine/claim count disagrees: {directory_name}")
    if expected_grouping is not None and receipt.get("grouping") != expected_grouping:
        raise ReviewRefused(f"receipt grouping disagrees for {directory_name}")
    claims = []
    for claim in generated.get("claims", []):
        if not isinstance(claim, dict) or not isinstance(claim.get("claim"), str):
            raise ReviewRefused(f"malformed claim: {variant}")
        locators = _canonical_locators(claim, transcript)
        evidence = []
        for locator in locators:
            text = transcript.turns[locator["turn"]].text[locator["start"]:locator["end"]]
            evidence.append({
                "turn": locator["turn"],
                "start": locator["start"],
                "end": locator["end"],
                "text_sha256": locator["text_sha256"],
                "text": text,
            })
        claims.append({
            "claim_ordinal": claim.get("claim_ordinal"),
            "claim_type": claim.get("claim_type"),
            "claim": claim["claim"],
            "evidence": evidence,
        })
    raw_turns = json.loads(transcript_path.read_text(encoding="utf-8")).get("turns", [])
    for claim in claims:
        for evidence in claim["evidence"]:
            raw = raw_turns[evidence["turn"]]
            evidence["start_seconds"] = raw.get("start")
            evidence["end_seconds"] = raw.get("end")
    return {
        "label": label,
        "variant": variant,
        "directory": directory_name,
        "engine": expected_engine,
        "grouping": expected_grouping,
        "receipt": str(receipt_path),
        "generated": str(generated_path),
        "transcript": str(transcript_path),
        "transcript_sha256": transcript_id,
        "generated_sha256": generated_digest,
        "attempt": str(attempt_path),
        "source_unchanged": receipt["source_unchanged"],
        "claim_count": len(claims),
        "claims": claims,
    }


def _audit_cycle(cycle: Path, lock_path: Path) -> dict:
    result = {"cycle": str(cycle), "lock": str(lock_path), "manifest": None, "reference": None, "decisions": None, "ledger": None, "lock_digest": None, "ledger_binding": None, "registration_binding": None, "validated": False}
    try:
        import sys
        sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "notes"))
        from capture_exposure import capture_review_view, empty_regions, identity_coordinates
        from candidate_exposure import validate_manifest, validate_reference
        from transcript import load
        import product_run
        raw_lock = lock_path.read_bytes()
        result["lock_digest"] = hashlib.sha256(raw_lock).hexdigest() == "7d3a45d9badf82dfcb05f769ea761cb81fcfd8f41c3f302c763d52ee3cd6930f"
        lock = json.loads(raw_lock)
        result["registration_binding"] = lock.get("registration_sha256") == product_run.product_run_sha256()
        view = capture_review_view(load(ORIGINAL_TRANSCRIPT))
        manifest = product_run.generate_manifest(view, product_run.STRATEGY_BROAD, contract=product_run.PRODUCT_CONTRACT)
        validate_manifest(manifest, view)
        stored_manifest = load_json(cycle / "product-manifest.json")
        result["manifest"] = product_run._json_bytes(stored_manifest) == product_run._json_bytes(manifest)
        reference = load_json(cycle / "product-reference.json")
        validate_reference(reference, view, stored_manifest, identity_coordinates(view), empty_regions(), enforce_registered=False)
        result["reference"] = True
        decisions = load_json(cycle / "product-review-decisions.validated.json")
        product_run.validate_review_decisions(decisions, reference, registration_sha256=product_run.product_run_sha256())
        result["decisions"] = True
        ledger = load_json(cycle / "product-events.json")
        expected_ledger = product_run.build_runner_ledger(decisions, reference, registration_sha256=product_run.product_run_sha256())
        result["ledger"] = product_run._json_bytes(ledger) == product_run._json_bytes(expected_ledger)
        result["ledger_binding"] = bool(result["ledger"] and lock.get("ledger_sha256") == ledger.get("ledger_sha256"))
        result["validated"] = all(result[key] is True for key in ("manifest", "reference", "decisions", "ledger", "lock_digest", "ledger_binding", "registration_binding"))
    except Exception as exc:
        result["error"] = type(exc).__name__
    return result


def old_lock_status() -> dict:
    evaluation = Path.home() / ".yawn-research/capture-exposure/630-fdd59c81-2026-08-14"
    supplied = "7d3a45d9badf82dfcb05f769ea761cb81fcfd8f41c3f302c763d52ee3cd6930f"
    related = []
    for root in (evaluation,):
        for current, dirs, names in os.walk(root, followlinks=False):
            dirs[:] = [name for name in dirs if not (Path(current) / name).is_symlink()]
            for name in names:
                path = Path(current) / name
                if not path.is_symlink() and path.is_file() and ("lock" in name.lower() or "approval" in name.lower()):
                    related.append(path)
    for root in (evaluation.parent, evaluation.parent.parent):
        for path in root.iterdir():
            if not path.is_symlink() and path.is_file() and ("lock" in path.name.lower() or "approval" in path.name.lower()):
                related.append(path)
    related = sorted(set(related))
    files = [path for path in related if path.name.endswith(".json") and "lock" in path.name.lower()]
    matches = [path for path in files if sha256(path) == supplied]
    audits = [_audit_cycle(path.parent, path) for path in matches if path.parent.name.startswith("product-cycle")]
    return {
        "supplied_lock_sha256": supplied,
        "lock_related_files": [str(path) for path in related],
        "lock_bytes_found": [str(path) for path in files],
        "matching_lock_bytes": [str(path) for path in matches],
        "original_scope": "prior capture-exposure reference/ledger approval only; never transferred to these engine transcripts",
        "old_binding": {
            "actual_lock_bytes": bool(matches),
            "audits": audits,
            "ledger_bound": any(a["ledger_binding"] is True for a in audits) if any(a["ledger_binding"] is not None for a in audits) else None,
            "registration_bound": any(a["registration_binding"] is True for a in audits) if audits else None,
            "validated": any(a["validated"] is True for a in audits) if audits else None,
        },
        "fresh_approval": False,
    }


def corpus_metadata(root: Path) -> dict:
    corpus_path = root / "corpus.json"
    corpus = load_json(corpus_path)
    selected = []
    for case in corpus.get("cases", []):
        if case.get("id") not in {"private-short-mic", "private-short-system"}:
            continue
        audio = Path(case["audio"])
        expected = _safe_digest(case.get("audio_sha256"), "corpus audio hash")
        if not audio.is_file() or sha256(audio) != expected:
            raise ReviewRefused(f"corpus audio hash/path mismatch: {case.get('id')}")
        selected.append({"id": case["id"], "path": str(audio), "sha256": expected, "duration_seconds": case.get("duration")})
    if len(selected) != 2:
        raise ReviewRefused("corpus does not contain both frozen short-call legs")
    return {"schema": corpus.get("schema"), "path": str(corpus_path), "audio": selected}


def load_variants_manifest(path: Path) -> list[dict]:
    manifest = load_json(path)
    if manifest.get("schema") != VARIANT_SCHEMA or not isinstance(manifest.get("variants"), list) or not manifest["variants"]:
        raise ReviewRefused("variants manifest has the wrong schema")
    seen_dirs: set[str] = set(); seen_labels: set[str] = set(); specs = []
    for spec in manifest["variants"]:
        if not isinstance(spec, dict):
            raise ReviewRefused("variant entry must be an object")
        if set(spec) != {"directory", "engine", "grouping", "label"}:
            raise ReviewRefused("variant entry has the wrong shape")
        directory, engine, grouping, label = (spec[k] for k in ("directory", "engine", "grouping", "label"))
        if not isinstance(directory, str) or not re.fullmatch(r"[A-Za-z0-9._-]+", directory) or directory in {".", ".."}:
            raise ReviewRefused("variant directory must be one relative path component")
        if Path(directory).name != directory or directory.startswith("."):
            raise ReviewRefused("variant directory may not traverse or be hidden")
        if engine not in {"apple", "parakeet", "whisper"} or grouping not in {"current", "bounded"}:
            raise ReviewRefused("variant engine or grouping is unsupported")
        if not isinstance(label, str) or not re.fullmatch(r"Variant [A-Z]", label) or directory in seen_dirs or label in seen_labels:
            raise ReviewRefused("variant directories and labels must be distinct")
        seen_dirs.add(directory); seen_labels.add(label); specs.append(dict(spec))
    return specs


def _clock(seconds: float | None) -> str:
    if seconds is None:
        return "unknown"
    whole = max(0, int(seconds))
    return f"{whole // 60}:{whole % 60:02d}"


def _markdown(variants: list[dict], lock: dict, corpus: dict) -> str:
    lines = [
        "# Private note review packet",
        "",
        "This packet contains saved local note outputs from the speech-engine comparison.",
        "It is a human review aid. It is not an approval, passed ledger, ranking, or shipping decision.",
        "",
        "## Review order",
        "",
        "Read each variant on its own first. Then compare the same event across variants.",
        "Processing speed, setup cost, and engine names are absent from this review surface.",
        "",
        "For every claim, check:",
        "",
        "- Missing commitment: did a decision, action, owner, deadline, or open question disappear?",
        "- Meaning: did negation, uncertainty, ownership, or timing change?",
        "- Source support: does the cited excerpt support the claim as written?",
        "- Reviewability: could a tired reader find the source and understand the limit of the claim?",
        "",
        "## Variants",
        "",
    ]
    for variant in variants:
        lines += [f"### {variant['label']}", "", f"Claims: {variant['claim_count']}", ""]
        if variant.get("unavailable"):
            lines += [f"Unavailable: {variant['unavailable']}", ""]
            continue
        lines += [f"Full transcript: [source](sources/{variant['label'].lower().replace(' ', '-')}-transcript.json)", ""]
        for claim in variant["claims"]:
            lines += [f"#### Claim {claim['claim_ordinal']} ({claim['claim_type']})", "", claim["claim"], "", "Evidence:", ""]
            for evidence in claim["evidence"]:
                lines += [f"- source time {_clock(evidence['start_seconds'])}–{_clock(evidence['end_seconds'])}: {evidence['text']}", f"  source file: [{variant['label']} transcript](sources/{variant['label'].lower().replace(' ', '-')}-transcript.json)"]
            lines.append("")
    lines += [
        "## Provenance",
        "",
        "Each variant is bound to its own saved generated note and content-addressed transcript.",
        "The prior lock was checked separately and is not applied to these new transcripts.",
        f"Old-lock binding validated: {'yes' if lock['old_binding']['validated'] else 'no'}; it does not approve these transcripts.",
        "",
        "The response template is in review.json. It remains pending until a person records a review.",
        "",
        "## Original short-call audio",
        "",
        "The frozen microphone and system legs are available at these verified local paths:",
        "",
    ]
    for audio in corpus["audio"]:
        label = "Microphone audio" if audio["id"].endswith("-mic") else "System audio"
        lines.append(f"- [{label}](<{audio['path']}>) ({_clock(audio['duration_seconds'])})")
    lines += ["", "## Full transcript sources", ""]
    for variant in variants:
        if not variant.get("unavailable"):
            lines.append(f"- [{variant['label']} transcript](sources/{variant['label'].lower().replace(' ', '-')}-transcript.json)")
    return "\n".join(lines)


def prepare(root: Path, out: Path, variants_manifest: Path | None = None) -> Path:
    if out.exists():
        raise ReviewRefused(f"review output already exists: {out}")
    resolved = out.resolve()
    forbidden = {
        Path("/Applications/Yawn.app"),
        Path.home() / "Library/Application Support/com.ninochavez.local-meeting-notes",
        Path.home() / "Library/Application Support/com.ninochavez.local-meeting-notes.preview",
        Path.home() / "Library/Application Support/local-meeting-notes",
        Path.home() / "Library/Application Support/Yawn",
        Path.home() / "Library/Application Support/Yawn Preview",
    }
    in_checkout = any((ancestor / ".git").is_file() or (ancestor / ".git").is_dir() for ancestor in (resolved, *resolved.parents))
    if in_checkout or any(root == resolved or root in resolved.parents for root in forbidden):
        raise ReviewRefused("output path is inside a product checkout or product storage")
    os.umask(0o077)
    out.mkdir(mode=0o700, parents=False)
    created: list[Path] = []
    try:
        specs = load_variants_manifest(variants_manifest) if variants_manifest else list(VARIANTS)
        variants = []
        for spec in specs:
            try:
                variants.append(validate_variant(root, spec["directory"], spec) if isinstance(spec, dict) else validate_variant(root, spec))
            except ReviewRefused as exc:
                if variants_manifest:
                    variants.append({"label": spec["label"], "directory": spec["directory"], "engine": spec["engine"], "grouping": spec["grouping"], "claim_count": 0, "unavailable": "Condition unavailable", "failure_detail": str(exc)})
                else:
                    raise
        lock = old_lock_status()
        corpus = corpus_metadata(root)
        sources = out / "sources"
        sources.mkdir(mode=0o700)
        created.append(sources)
        for variant in variants:
            if variant.get("unavailable"):
                continue
            target = sources / f"{variant['label'].lower().replace(' ', '-')}-transcript.json"
            target.write_bytes(Path(variant["transcript"]).read_bytes())
            os.chmod(target, 0o600)
            created.append(target)
        review_path = out / "REVIEW.md"
        review_path.write_text(_markdown(variants, lock, corpus), encoding="utf-8")
        created.append(review_path)
        review = {
            "schema": "speech-note-review/1",
            "status": "pending",
            "answered": False,
            "rubric": [
                "missing commitments, owners, deadlines, or open questions",
                "incorrect meaning, ownership, deadline, uncertainty, or negation",
                "source support from the cited transcript spans",
            ],
            "variants": [{"label": variant["label"], "claim_count": variant["claim_count"], **({"unavailable": variant["unavailable"]} if variant.get("unavailable") else {})} for variant in variants],
            "responses": [],
        }
        review_json = out / "review.json"
        review_json.write_text(json.dumps(review, indent=2) + "\n", encoding="utf-8")
        created.append(review_json)
        provenance = out / "provenance.json"
        provenance.write_text(json.dumps({"schema": "speech-note-review-provenance/1", "variants": variants, "old_lock": lock, "corpus": corpus}, indent=2) + "\n", encoding="utf-8")
        created.append(provenance)
        engine_key = {
            "schema": "speech-note-review-engine-key/1",
            "labels": {variant["label"]: {"engine": variant["engine"], "grouping": variant["grouping"]} for variant in variants},
            "note": "Keep this file separate from the human review surface to avoid engine-name anchoring.",
        }
        key_path = out / "engine-key.json"
        key_path.write_text(json.dumps(engine_key, indent=2) + "\n", encoding="utf-8")
        created.append(key_path)
        for path in created:
            os.chmod(path, 0o700 if path.is_dir() else 0o600)
    except Exception:
        for path in reversed(created):
            if path.is_file():
                path.unlink()
            elif path.is_dir() and not any(path.iterdir()):
                path.rmdir()
        if out.is_dir() and not any(out.iterdir()):
            out.rmdir()
        raise
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--variants-manifest", type=Path)
    args = parser.parse_args()
    print(json.dumps({"schema": "speech-note-review/1", "out": str(prepare(args.root, args.out, args.variants_manifest))}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
