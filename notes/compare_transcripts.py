#!/usr/bin/env python3
"""Compare two transcripts of the same meeting.

Written to answer one question: is our own speech recognition good enough to
build notes on, or is it the thing holding recall down? Word error rate is the
usual answer and it is the wrong one here. A transcript can miss "um" a hundred
times and lose nothing; miss the one noun phrase naming the document somebody
promised to send, once, and the commitment that depends on it cannot be written.
So this measures **whether the terms the
meeting's commitments rest on survived**, and takes those terms from the
reference action items rather than from a hand-written list.

The second measurement is fragmentation, and it exists because of a result that
went the opposite way from expected. Google Meet's transcript carries speaker
labels; ours does not. Google's therefore looked like the better input. But Meet
resolves overlapping speech by cutting turns at every interruption, so 42% of
its units were three words or fewer on a real call, and during crosstalk it
emits speaker labels *inside* another speaker's sentence:

    Speaker A: Oh, wait. Are you able to share? You should be able Speaker B:
    Speaker A: to. Speaker B: to

Whisper, given the same audio, returns continuous sentences and no labels at
all. Attribution is bought with sentence integrity, and for note-writing the
sentences turned out to matter more.

Holding one side fixed and varying the other answers different questions with the
same measurement. Against a platform's transcript it asks whether our recogniser
is the bottleneck. Against our own direct decode of a file it asks what the
capture path costs — same audio, same model, so anything lost belongs to the tap,
the resampling round-trip, or the block chunking. The answer to both, so far, is
nothing.

Usage:
    python notes/compare_transcripts.py theirs.txt ours.json
    python notes/compare_transcripts.py theirs.txt ours.json --reference items.md
    python notes/compare_transcripts.py --traps notes/fixtures/synthetic/trap-briefing.json \
        [--speaker Me] run.json [run.json ...]

`--traps` scores planted traps instead (docs/planted-trap-evaluation-scope.md).
Each transcript is aligned to the synthetic script by its words, and each trap
is looked for only inside the stretch aligned to its own line, so "sixteen" in
one line cannot stand in for "sixty" in another. A trap is **kept** when an
accepted form appears there, **wrong** when other words do (the closest window
is recorded verbatim), and **missing** when nothing aligns to the line. With
several runs of one engine, a wrong form that repeats identically in every run
is **vocabulary-fixable**: Yawn's local vocabulary replaces exact phrases only.
Matching ignores case and punctuation. Exit status is 1 unless every trap in
every run is kept, so a mangled control fails.
"""

from __future__ import annotations

import argparse
import difflib
import json
import re
import sys
from pathlib import Path

from summarize import _NOTE_REGISTER, _words, load_reference
from transcript import load


def stats(t) -> dict:
    lengths = [len(x.text.split()) for x in t.turns]
    if not lengths:
        return {}
    return {
        "units": len(lengths),
        "words": sum(lengths),
        "mean_words": sum(lengths) / len(lengths),
        # Short units are the fingerprint of turn-cutting at every interruption.
        "tiny_pct": 100 * sum(1 for n in lengths if n <= 3) / len(lengths),
        "speakers": len(t.speakers),
    }


def term_survival(reference_items: list[str], theirs: str, ours: str) -> list[dict]:
    """For each commitment, which of its content words appear in each transcript.

    A term missing from a transcript is a commitment that transcript cannot
    support, whatever its word error rate.
    """
    rows = []
    for item in reference_items:
        # Drop the bracketed owner: names are an attribution question, and this
        # is about whether the substance of the commitment is recoverable.
        body = re.sub(r"^\[[^\]]*\]", "", item)
        terms = sorted(_words(body) - _NOTE_REGISTER)
        if not terms:
            continue
        in_theirs = [w for w in terms if w in theirs]
        in_ours = [w for w in terms if w in ours]
        rows.append({
            "item": body.strip()[:60],
            "terms": len(terms),
            "theirs": len(in_theirs),
            "ours": len(in_ours),
            "lost_by_ours": sorted(set(in_theirs) - set(in_ours)),
        })
    return rows


# --- planted traps -----------------------------------------------------------

_TOKEN = re.compile(r"[a-z0-9]+")


def _tokens(text: str) -> list[re.Match]:
    return list(_TOKEN.finditer(text.lower()))


def _contains(haystack: list[str], needle: list[str]) -> bool:
    n = len(needle)
    return n > 0 and any(haystack[i:i + n] == needle for i in range(len(haystack) - n + 1))


def line_spans(script_lines: list[dict], text: str) -> dict[str, tuple[int, int]]:
    """Token index range of `text` aligned to each script line."""
    script: list[tuple[str, str]] = []
    for line in script_lines:
        script.extend((line["id"], m.group()) for m in _tokens(line["text"]))
    heard = [m.group() for m in _tokens(text)]
    matcher = difflib.SequenceMatcher(None, [w for _, w in script], heard, autojunk=False)
    to_heard: dict[int, int] = {}
    for block in matcher.get_matching_blocks():
        for k in range(block.size):
            to_heard[block.a + k] = block.b + k

    # A line's stretch runs from just after the last word aligned to any
    # earlier line to the first word aligned to any later line. Unaligned words
    # between two lines therefore belong to both: a garbled first or last word
    # is never silently credited to the neighbour alone.
    spans: dict[str, tuple[int, int]] = {}
    index = 0
    for line in script_lines:
        count = len(_tokens(line["text"]))
        earlier = [to_heard[i] for i in range(0, index) if i in to_heard]
        later = [to_heard[i] for i in range(index + count, len(script)) if i in to_heard]
        start = max(earlier) + 1 if earlier else 0
        end = min(later) if later else len(heard)
        spans[line["id"]] = (start, max(start, end))
        index += count
    return spans


def _heard_in_place(line: str, spoken: str, region: list[str]) -> str:
    """The words aligned to the trap's own position in its line.

    Positional, not by similarity: a garble that looks nothing like the trap is
    still what was heard there. Empty means the trap's words were dropped.
    """
    script = [m.group() for m in _tokens(line)]
    needle = [m.group() for m in _tokens(spoken)]
    first = next(i for i in range(len(script)) if script[i:i + len(needle)] == needle)
    last = first + len(needle)
    heard: list[str] = []
    matcher = difflib.SequenceMatcher(None, script, region, autojunk=False)
    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        if tag == "insert":
            if first < i1 <= last or (i1 == first and heard):
                heard.extend(region[j1:j2])
        elif i1 < last and i2 > first:
            if tag == "equal":
                lo, hi = max(i1, first), min(i2, last)
                heard.extend(region[j1 + lo - i1:j1 + hi - i1])
            else:
                heard.extend(region[j1:j2])
    return " ".join(heard)


def score_traps(key: dict, text: str, speaker: str | None = None) -> list[dict]:
    lines = [ln for ln in key["lines"] if speaker is None or ln["speaker"] == speaker]
    traps = [t for t in key["traps"] if t["line"] in {ln["id"] for ln in lines}]
    words = [m.group() for m in _tokens(text)]
    spans = line_spans(lines, text)
    line_text = {ln["id"]: ln["text"] for ln in lines}
    rows = []
    for trap in traps:
        start, end = spans[trap["line"]]
        region = words[start:end]
        if not region:
            rows.append({"id": trap["id"], "status": "missing", "heard": None})
            continue
        if any(_contains(region, [m.group() for m in _tokens(form)]) for form in trap["accepted"]):
            rows.append({"id": trap["id"], "status": "kept", "heard": None})
            continue
        rows.append({"id": trap["id"], "status": "wrong",
                     "heard": _heard_in_place(line_text[trap["line"]], trap["spoken"], region)})
    return rows


def transcript_texts(path: Path) -> list[str]:
    """One text per run: a probe result's `runs`, or one transcript file."""
    if path.suffix == ".txt":
        return [path.read_text()]
    data = json.loads(path.read_text())
    if isinstance(data, dict) and isinstance(data.get("runs"), list):
        return [run["text"] for run in data["runs"]]
    return [" ".join(turn.text for turn in load(path).turns)]


def trap_report(key: dict, texts: list[str], speaker: str | None = None) -> dict:
    runs = [score_traps(key, text, speaker) for text in texts]
    by_trap: dict[str, list[dict]] = {}
    for run in runs:
        for row in run:
            by_trap.setdefault(row["id"], []).append(row)
    traps = []
    for trap_id, rows in by_trap.items():
        heard = {row["heard"] for row in rows}
        traps.append({
            "id": trap_id,
            "kept": sum(row["status"] == "kept" for row in rows),
            "statuses": [row["status"] for row in rows],
            "heard": [row["heard"] for row in rows],
            # A dropped trap (nothing heard in place) has no phrase to replace.
            "vocabulary_fixable": all(row["status"] == "wrong" for row in rows)
            and len(heard) == 1 and bool(next(iter(heard))),
        })
    total = len(traps) * len(runs)
    kept = sum(t["kept"] for t in traps)
    return {"runs": len(runs), "traps": len(traps), "kept": kept, "total": total,
            "vocabulary_fixable": [t["id"] for t in traps if t["vocabulary_fixable"]],
            "rows": traps}


def trap_main(key_path: Path, paths: list[Path], speaker: str | None, as_json: bool) -> int:
    key = json.loads(key_path.read_text())
    if speaker is not None and speaker not in key["speakers"]:
        print(f"unknown speaker {speaker!r}; the key declares {sorted(key['speakers'])}",
              file=sys.stderr)
        return 2
    texts = [text for path in paths for text in transcript_texts(path)]
    report = trap_report(key, texts, speaker)
    if report["total"] == 0:
        # Nothing scored is not a pass: no runs, or no traps for this speaker.
        print("no trap checks were scored", file=sys.stderr)
        return 1
    if as_json:
        print(json.dumps(report, indent=2))
    else:
        spoken = {t["id"]: t["spoken"] for t in key["traps"]}
        print(f"\n  {report['kept']}/{report['total']} trap checks kept "
              f"({report['traps']} traps x {report['runs']} run(s))\n")
        for row in report["rows"]:
            symbol = {"kept": "+", "wrong": "x", "missing": "-"}
            marks = " ".join(symbol[status] for status in row["statuses"])
            line = f"  {row['id']} {spoken[row['id']]:22s} {marks}"
            wrong = sorted({h for h in row["heard"] if h})
            if wrong:
                line += "   heard: " + " | ".join(wrong)
            if row["vocabulary_fixable"]:
                line += "   (same every run)"
            print(line)
    return 0 if report["kept"] == report["total"] else 1


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("paths", type=Path, nargs="+",
                   help="theirs and ours; with --traps, one or more transcripts or "
                        "probe results to score")
    p.add_argument("--reference", type=Path,
                   help="action items whose terms must survive; without it only "
                        "shape is compared")
    p.add_argument("--traps", type=Path, help="a planted-trap answer key")
    p.add_argument("--speaker", help="with --traps, score only this speaker's lines (one leg)")
    p.add_argument("--json", action="store_true", help="with --traps, print the report as JSON")
    args = p.parse_args()

    if args.traps:
        return trap_main(args.traps, args.paths, args.speaker, args.json)
    if len(args.paths) != 2:
        p.error("expected exactly two transcripts: theirs and ours")
    a, b = load(args.paths[0]), load(args.paths[1])
    sa, sb = stats(a), stats(b)

    print(f"\n{'':16s} {'units':>6} {'words':>7} {'mean':>6} {'<=3 words':>10} {'speakers':>9}")
    for name, s in (("theirs", sa), ("ours", sb)):
        print(f"  {name:14s} {s['units']:>6} {s['words']:>7} {s['mean_words']:>6.1f} "
              f"{s['tiny_pct']:>9.0f}% {s['speakers']:>9}")

    if sb["words"] and sa["words"]:
        print(f"\n  ours carries {100 * sb['words'] / sa['words']:.0f}% of the reference "
              "transcript's word count")
    if sa["tiny_pct"] > sb["tiny_pct"] + 10:
        print("  the reference is markedly more fragmented — it cuts turns at "
              "every interruption")

    if not args.reference:
        print("\n  (pass --reference to check whether commitment terms survived)")
        return 0

    items = load_reference(args.reference)
    theirs_text = " ".join(x.text for x in a.turns).lower()
    ours_text = " ".join(x.text for x in b.turns).lower()
    rows = term_survival(items, theirs_text, ours_text)

    print("\n  commitment terms present in each transcript\n")
    print(f"  {'commitment':62s} {'theirs':>7} {'ours':>6}")
    lost_total = 0
    for r in rows:
        print(f"  {r['item']:62s} {r['theirs']:>3}/{r['terms']:<3} {r['ours']:>3}/{r['terms']:<3}")
        if r["lost_by_ours"]:
            lost_total += len(r["lost_by_ours"])
            print(f"      only in theirs: {r['lost_by_ours']}")

    print(f"\n  {lost_total} commitment term(s) present in the reference transcript "
          "and absent from ours")
    if not lost_total:
        # Deliberately not "speech recognition is fine". The same comparison is
        # used to hold the recogniser constant and vary something else — a
        # capture path against a direct decode of the same file, say — and there
        # the conclusion is about the capture, not the ASR. Name the measurement,
        # not a component the tool cannot see.
        print("  Every term the commitments rest on survived the transcript under "
              "test.\n  Whatever is limiting recall, it is not transcription loss.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
