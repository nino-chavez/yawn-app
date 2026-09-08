#!/usr/bin/env python3
"""Freeze a local AMI human-annotation reference before transcription runs.

The generated JSON deliberately remains outside the source tree.  It contains
manual transcript text, so it is a local research input, not a product fixture
or a shareable benchmark receipt.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import tempfile
import xml.etree.ElementTree as ET
import zipfile
from collections import defaultdict
from pathlib import Path
from typing import Any

SCHEMA = "speech-reference/1"
NITE_ID = "{http://nite.sourceforge.net/}id"
TOKEN = re.compile(r"[a-z0-9]+(?:'[a-z0-9]+)?", re.I)


def local_name(element: ET.Element) -> str:
    return element.tag.rsplit("}", 1)[-1]


def nite_id(element: ET.Element) -> str | None:
    return element.attrib.get(NITE_ID) or element.attrib.get("nite:id")


def href_ids(href: str) -> tuple[str, str | None]:
    """Return the first and optional last NITE id in a child/pointer href."""
    # NITE ranges are written ``#id(first)..id(last)``: the second endpoint
    # intentionally has no second ``#``.  Requiring one truncated every range.
    ids = re.findall(r"(?:#|\.\.)id\(([^)]+)\)", href)
    if not ids:
        raise ValueError(f"NITE href has no id: {href!r}")
    if len(ids) > 2:
        raise ValueError(f"NITE href has too many ids: {href!r}")
    return ids[0], ids[1] if len(ids) > 1 else None


def normalized_terms(text: str) -> list[str]:
    return sorted(set(match.group(0).casefold() for match in TOKEN.finditer(text)))


def xml_from_archive(archive: zipfile.ZipFile, member: str) -> ET.Element:
    try:
        with archive.open(member) as handle:
            return ET.parse(handle).getroot()
    except KeyError as exc:
        raise ValueError(f"AMI archive is missing {member}") from exc


def read_words(archive: zipfile.ZipFile, meeting: str) -> tuple[list[dict[str, Any]], dict[str, dict[str, Any]], dict[str, list[str]]]:
    words: list[dict[str, Any]] = []
    by_id: dict[str, dict[str, Any]] = {}
    track_ids: dict[str, list[str]] = {}
    for speaker in "ABCD":
        member = f"words/{meeting}.{speaker}.words.xml"
        root = xml_from_archive(archive, member)
        ids: list[str] = []
        for element in root:
            word_id = nite_id(element)
            if not word_id:
                continue
            try:
                start = float(element.attrib["starttime"])
                end = float(element.attrib["endtime"])
            except (KeyError, ValueError) as exc:
                raise ValueError(f"invalid manual word timing for {word_id}") from exc
            # Dialogue-act ranges can start or end on a gap/vocal sound. Keep
            # every timed NITE id for resolving the range, then retain only
            # lexical <w> nodes in the reference transcript and spans.
            record = {
                "text": element.text or "",
                "start": start,
                "end": end,
                "speaker": speaker,
                "id": word_id,
                "is_word": local_name(element) == "w",
            }
            by_id[word_id] = record
            ids.append(word_id)
            if record["is_word"]:
                words.append({key: record[key] for key in ("text", "start", "end", "speaker")})
        track_ids[speaker] = ids
    words.sort(key=lambda item: (item["start"], item["end"], item["speaker"], item["text"]))
    return words, by_id, track_ids


def range_words(first: str, last: str | None, by_id: dict[str, dict[str, Any]], track_ids: dict[str, list[str]]) -> list[dict[str, Any]]:
    if first not in by_id:
        raise ValueError(f"annotation refers to missing word {first}")
    speaker = by_id[first]["speaker"]
    ids = track_ids[speaker]
    start_index = ids.index(first)
    end_index = ids.index(last) if last else start_index
    if end_index < start_index:
        raise ValueError(f"reversed NITE word range {first}..{last}")
    return [by_id[word_id] for word_id in ids[start_index : end_index + 1] if by_id[word_id]["is_word"]]


def dialogue_act_words(archive: zipfile.ZipFile, meeting: str, by_id: dict[str, dict[str, Any]], track_ids: dict[str, list[str]]) -> dict[str, list[dict[str, Any]]]:
    acts: dict[str, list[dict[str, Any]]] = {}
    for speaker in "ABCD":
        root = xml_from_archive(archive, f"dialogueActs/{meeting}.{speaker}.dialog-act.xml")
        for element in root:
            if local_name(element) != "dact":
                continue
            act_id = nite_id(element)
            children = [child for child in element if local_name(child) == "child"]
            resolved: list[dict[str, Any]] = []
            for child in children:
                first, last = href_ids(child.attrib["href"])
                resolved.extend(range_words(first, last, by_id, track_ids))
            if act_id and resolved:
                acts[act_id] = resolved
    return acts


def linked_important_spans(archive: zipfile.ZipFile, meeting: str, acts: dict[str, list[dict[str, Any]]]) -> list[dict[str, Any]]:
    abstract_root = xml_from_archive(archive, f"abstractive/{meeting}.abssumm.xml")
    sentence_kind: dict[str, str] = {}
    for group in abstract_root:
        kind = local_name(group)
        if kind not in {"actions", "decisions"}:
            continue
        for sentence in group:
            sentence_id = nite_id(sentence)
            if sentence_id and (sentence.text or "").strip() and (sentence.text or "").strip() != "NA":
                sentence_kind[sentence_id] = kind[:-1]

    links_root = xml_from_archive(archive, f"extractive/{meeting}.summlink.xml")
    grouped: dict[tuple[str, str], list[str]] = defaultdict(list)
    for link in links_root:
        pointers = {pointer.attrib.get("role"): pointer.attrib.get("href", "") for pointer in link if local_name(pointer) == "pointer"}
        if "abstractive" not in pointers or "extractive" not in pointers:
            continue
        abstract_id, _ = href_ids(pointers["abstractive"])
        if abstract_id not in sentence_kind:
            continue
        extractive_id, _ = href_ids(pointers["extractive"])
        if extractive_id in acts:
            grouped[(abstract_id, sentence_kind[abstract_id])].append(extractive_id)

    spans: list[dict[str, Any]] = []
    for (abstract_id, kind), act_ids in sorted(grouped.items()):
        # A summary sentence can link to dialogue acts far apart in time.  Keep
        # each source act as a separate local span instead of taking one broad
        # enclosing interval that would make a time-local survival score vacuous.
        for act_id in sorted(set(act_ids)):
            resolved = acts[act_id]
            text = " ".join(word["text"] for word in resolved)
            spans.append({
                "id": f"{abstract_id}::{act_id}",
                "kind": kind,
                "start": min(word["start"] for word in resolved),
                "end": max(word["end"] for word in resolved),
                "text": text,
                # Terms are every normalized manual lexical token in the linked
                # extractive evidence.  Nothing is hand-picked or inferred.
                "terms": normalized_terms(text),
                "source_abstractive_sentence": abstract_id,
                "source_dialogue_act": act_id,
            })
    return sorted(spans, key=lambda span: (span["start"], span["end"], span["id"]))


def named_entity_spans(archive: zipfile.ZipFile, meeting: str, by_id: dict[str, dict[str, Any]], track_ids: dict[str, list[str]]) -> list[dict[str, Any]]:
    spans: list[dict[str, Any]] = []
    for speaker in "ABCD":
        root = xml_from_archive(archive, f"namedEntities/{meeting}.{speaker}.ne.xml")
        for entity in root:
            if local_name(entity) != "named-entity":
                continue
            entity_id = nite_id(entity)
            type_href = next((child.attrib.get("href", "") for child in entity if local_name(child) == "pointer" and child.attrib.get("role") == "type"), "")
            child = next((child for child in entity if local_name(child) == "child"), None)
            if not entity_id or child is None:
                continue
            first, last = href_ids(child.attrib["href"])
            resolved = range_words(first, last, by_id, track_ids)
            text = " ".join(word["text"] for word in resolved)
            type_id, _ = href_ids(type_href) if type_href else ("unknown", None)
            spans.append({
                "id": entity_id,
                "kind": "annotated-entity",
                "entity_type_id": type_id,
                "start": min(word["start"] for word in resolved),
                "end": max(word["end"] for word in resolved),
                "text": text,
                "terms": normalized_terms(text),
            })
    return sorted(spans, key=lambda span: (span["start"], span["end"], span["id"]))


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def prepare(archive_path: Path, meeting: str) -> dict[str, Any]:
    with zipfile.ZipFile(archive_path) as archive:
        words, by_id, track_ids = read_words(archive, meeting)
        acts = dialogue_act_words(archive, meeting, by_id, track_ids)
        important_spans = linked_important_spans(archive, meeting, acts)
        entity_spans = named_entity_spans(archive, meeting, by_id, track_ids)
    if not words:
        raise ValueError("AMI word annotations are empty")
    return {
        "schema": SCHEMA,
        "kind": "human-annotated-public-meeting",
        "meeting": meeting,
        "provenance": {
            "archive_sha256": sha256_file(archive_path),
            "word_layers": [f"words/{meeting}.{speaker}.words.xml" for speaker in "ABCD"],
            "important_span_rule": "manual abstractive actions/decisions linked through summlink to manual dialogue-act word ranges",
            "entity_rule": "manual namedEntities word ranges; entity type is retained and never assumed to mean person",
            "postfreeze_parser_correction": {
                "reason": "v1 parsed NITE range endpoints with #id only; AMI writes the second endpoint as ..id(last), which truncated ranges to their first token",
                "source_annotations": "unchanged; v2 re-resolves the same archive and NITE links",
                "v1_reference": "ami-reference.json retained unchanged; do not use its span-level results",
            },
        },
        "words": words,
        "reference_text": " ".join(word["text"] for word in words),
        "important_spans": important_spans,
        "entity_spans": entity_spans,
    }


def write_private_json(path: Path, document: dict[str, Any], overwrite: bool) -> None:
    path = path.expanduser().resolve()
    if not path.parent.is_dir():
        raise ValueError("--out parent directory does not exist")
    if path.exists() and not overwrite:
        raise ValueError("--out exists; pass --overwrite only for the explicitly named local reference")
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            json.dump(document, handle, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
            handle.write("\n")
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
        os.chmod(path, 0o600)
    except BaseException:
        Path(temporary).unlink(missing_ok=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--meeting", required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--overwrite", action="store_true")
    args = parser.parse_args()
    try:
        document = prepare(args.archive.expanduser().resolve(), args.meeting)
        write_private_json(args.out, document, args.overwrite)
    except (OSError, ValueError, zipfile.BadZipFile, ET.ParseError) as exc:
        parser.error(str(exc))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
