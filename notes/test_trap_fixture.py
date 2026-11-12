"""The planted-trap answer key stays internally consistent and gate-exempt."""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
KEY = ROOT / "notes" / "fixtures" / "synthetic" / "trap-briefing.json"
sys.path.insert(0, str(ROOT))

import privacy_gate  # noqa: E402


def _key() -> dict:
    return json.loads(KEY.read_text())


def test_every_trap_is_spoken_in_its_line_and_accepted():
    key = _key()
    lines = {line["id"]: line["text"] for line in key["lines"]}
    for trap in key["traps"]:
        assert trap["spoken"] in lines[trap["line"]], trap["id"]
        assert trap["spoken"] in trap["accepted"], trap["id"]


def test_ids_are_unique_and_speakers_are_declared():
    key = _key()
    line_ids = [line["id"] for line in key["lines"]]
    trap_ids = [trap["id"] for trap in key["traps"]]
    assert len(line_ids) == len(set(line_ids))
    assert len(trap_ids) == len(set(trap_ids))
    assert {line["speaker"] for line in key["lines"]} <= set(key["speakers"])


def test_key_is_exempt_only_because_of_its_header():
    relative = KEY.relative_to(ROOT)
    payload = KEY.read_bytes()
    assert privacy_gate.parse_and_scan(payload, "key", relative) == []
    stripped = {k: v for k, v in _key().items() if k != "synthetic"}
    assert privacy_gate.parse_and_scan(json.dumps(stripped).encode(), "key", relative)


FROZEN_SHA256 = "2c8e19db634c041428f1b448d26c0cf9cabe6af6db6f5d033e00d9c1b112102c"


def test_key_matches_its_frozen_hash():
    # Results before and after an edit are not comparable. Changing the key
    # means updating this hash and the scope doc together, on purpose.
    import hashlib

    assert hashlib.sha256(KEY.read_bytes()).hexdigest() == FROZEN_SHA256
