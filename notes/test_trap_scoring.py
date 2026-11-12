"""The --traps scorer: clean scores full, mangled scores zero, positions hold."""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import compare_transcripts as ct  # noqa: E402

KEY = json.loads((HERE / "fixtures" / "synthetic" / "trap-briefing.json").read_text())


def _script(lines=None) -> str:
    return " ".join(line["text"] for line in (lines or KEY["lines"]))


def _mangled() -> str:
    text = _script()
    for trap in KEY["traps"]:
        text = text.replace(trap["spoken"], "zork")
    return text


def test_clean_script_keeps_every_trap():
    report = ct.trap_report(KEY, [_script()])
    assert report["kept"] == report["total"] == len(KEY["traps"])


def test_mangled_control_keeps_none():
    report = ct.trap_report(KEY, [_mangled()])
    assert report["kept"] == 0
    assert all(set(row["statuses"]) == {"wrong"} for row in report["rows"])


def test_a_form_said_elsewhere_does_not_count():
    # "sixteen" is spoken in L03, L13 and L14; losing it only in L14 must fail T17.
    text = _script().replace("sixteen gigabytes per workstation", "sixty gigabytes per workstation")
    rows = {row["id"]: row for row in ct.trap_report(KEY, [text])["rows"]}
    assert rows["T17"]["statuses"] == ["wrong"]
    assert rows["T16"]["statuses"] == ["kept"]


def test_wrong_form_is_recorded_and_repeat_is_fixable():
    text = _script().replace("RTX 4070 Ti Super", "RTX 470T Super")
    report = ct.trap_report(KEY, [text, text, text])
    rows = {row["id"]: row for row in report["rows"]}
    assert rows["T02"]["heard"] == ["rtx 470t super"] * 3
    assert report["vocabulary_fixable"] == ["T02"]


def test_varying_wrong_forms_are_not_fixable():
    a = _script().replace("Nguyen", "Win")
    b = _script().replace("Nguyen", "When")
    report = ct.trap_report(KEY, [a, b])
    assert "T09" not in report["vocabulary_fixable"]


def test_dropped_line_is_missing():
    lines = [line for line in KEY["lines"] if line["id"] != "L09"]
    rows = {row["id"]: row for row in ct.trap_report(KEY, [_script(lines)])["rows"]}
    assert rows["T11"]["statuses"] == ["missing"]
    assert rows["T12"]["statuses"] == ["missing"]


def test_one_leg_scores_only_its_speaker():
    me = [line for line in KEY["lines"] if line["speaker"] == "Me"]
    report = ct.trap_report(KEY, [_script(me)], speaker="Me")
    assert report["kept"] == report["total"] > 0
    me_ids = {line["id"] for line in me}
    assert report["traps"] == sum(trap["line"] in me_ids for trap in KEY["traps"])


def test_exit_status_fails_unless_all_kept(tmp_path):
    clean, mangled = tmp_path / "clean.txt", tmp_path / "mangled.txt"
    clean.write_text(_script())
    mangled.write_text(_mangled())
    key = HERE / "fixtures" / "synthetic" / "trap-briefing.json"
    assert ct.trap_main(key, [clean], None, True) == 0
    assert ct.trap_main(key, [mangled], None, True) == 1


def test_dropped_trap_word_is_not_fixable():
    text = _script().replace("Nguyen owns", "owns")
    report = ct.trap_report(KEY, [text, text])
    rows = {row["id"]: row for row in report["rows"]}
    assert rows["T09"]["heard"] == ["", ""]
    assert "T09" not in report["vocabulary_fixable"]


def test_nothing_scored_is_not_a_pass(tmp_path):
    key = HERE / "fixtures" / "synthetic" / "trap-briefing.json"
    empty = tmp_path / "empty.json"
    empty.write_text(json.dumps({"runs": []}))
    clean = tmp_path / "clean.txt"
    clean.write_text(_script())
    assert ct.trap_main(key, [empty], None, True) == 1
    assert ct.trap_main(key, [clean], "me", True) == 2
