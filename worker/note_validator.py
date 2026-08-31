"""FD-pinned, read-only product note reinspection for the one-shot bridge."""

from __future__ import annotations

import hashlib
import json
import os
import re
import stat
import tempfile
import unicodedata
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

MAX_ARTIFACT_BYTES = 16 * 1024 * 1024
# Transport-advisory, not registered. `PRODUCT_RUN` pins no context width
# because the product transport is in-process MLX-LM: the child sizes its own
# context from the model's own configuration, so nothing here can set it. The
# field stays on the request packet because the packet's shape is the
# transport's contract and a provider that does honour a context width (a
# server-backed one) must still be told a number wide enough for the ±2 window
# at batch size 1. A generator that ignores it is behaving correctly.
TRANSPORT_NUM_CTX = 32768
SYNTHESIS_NUM_PREDICT = 1800
SYNTHESIS_MAX_OVERVIEW = 4
SYNTHESIS_MAX_ITEMS = 12
SYNTHESIS_MAX_CLAIM_CHARS = 280

SYNTHESIS_SYSTEM = """You write a useful meeting note from selected transcript excerpts.

Return only one compact JSON object with exactly these keys in this order:
{"overview":[...],"items":[...]}

overview is an array of 2 to 4 plain sentences explaining the main subjects and context. It must be descriptive only: never say something was agreed, decided, committed, prioritized, or will happen. Each overview entry has exactly {"text":"...","evidence_ids":["..."]}. Use 1 to 3 evidence_ids from the supplied excerpts.

items is an array. Each item has exactly {"type":"decision|action|proposal|question","text":"...","evidence_ids":["..."]}. Use 1 to 3 evidence_ids from the supplied excerpts.

Rules:
- Decisions are only things explicitly settled or agreed. A suggestion, possibility, preference, or idea is a proposal, never a decision.
- Actions are only explicit commitments to do something. Discussion of possible work is a proposal, never an action.
- Questions are unresolved questions, not rhetorical questions.
- Keep proposals separate so ideas do not become commitments.
- Ignore greetings, recording setup, audio checks, "we're good to go", and other meeting logistics unless the meeting was specifically about them.
- Do not turn general discussion into an outcome.
- Every text must be supported by all and only its evidence_ids.
- Use the supplied ID strings exactly. Never invent an ID.
- Do not copy long transcript passages. Write concise note language.
- Do not identify a speaker unless the excerpt itself names the person.
- Prefer a short useful note over filling every category. Include every clear action or decision that is actually present.
- Omit empty categories. Do not add preamble, markdown, or a code fence.
"""


class ArtifactFailure(ValueError):
    def __init__(self, code: str, recoverable: bool):
        super().__init__(code)
        self.code = code
        self.recoverable = recoverable


class GenerationRefused(ValueError):
    """Untrusted generator output failed a check; the caller falls back.

    Separate from `ArtifactFailure` because the two answer different questions.
    An artifact failure says the retained bytes on this Mac are missing, unsafe,
    or changed. A generation refusal says the retained bytes were fine and the
    model's proposal did not survive the note/2 evidence rules. Only the second
    is allowed to produce `transcript-only`; collapsing them would let a storage
    fault be reported as a quiet quality outcome.
    """

    def __init__(self, code: str, recoverable: bool):
        super().__init__(code)
        self.code = code
        self.recoverable = recoverable


@dataclass
class _DirectoryLink:
    parent_fd: int
    name: str
    child_fd: int
    identity: os.stat_result


@dataclass
class _FileLink:
    parent_fd: int
    name: str
    file_fd: int
    identity: os.stat_result


def _same_identity(left: os.stat_result, right: os.stat_result) -> bool:
    return (
        left.st_dev == right.st_dev
        and left.st_ino == right.st_ino
        and left.st_mode == right.st_mode
        and left.st_uid == right.st_uid
        and left.st_size == right.st_size
        and left.st_nlink == right.st_nlink
    )


def _open_directories(root_fd: int, names: list[str]) -> tuple[int, list[_DirectoryLink]]:
    current = os.dup(root_fd)
    links: list[_DirectoryLink] = []
    try:
        for name in names:
            child = os.open(
                name,
                os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                dir_fd=current,
            )
            identity = os.fstat(child)
            if (
                not stat.S_ISDIR(identity.st_mode)
                or stat.S_IMODE(identity.st_mode) != 0o700
                or identity.st_uid != os.geteuid()
            ):
                os.close(child)
                raise ArtifactFailure("artifact-invalid", False)
            links.append(_DirectoryLink(current, name, child, identity))
            current = os.dup(child)
        return os.dup(current), links
    except FileNotFoundError as exc:
        raise ArtifactFailure("artifact-missing", True) from exc
    except OSError as exc:
        raise ArtifactFailure("artifact-invalid", False) from exc
    finally:
        os.close(current)


def _open_file(parent_fd: int, name: str) -> _FileLink:
    try:
        descriptor = os.open(name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=parent_fd)
    except FileNotFoundError as exc:
        raise ArtifactFailure("artifact-missing", True) from exc
    except OSError as exc:
        raise ArtifactFailure("artifact-invalid", False) from exc
    identity = os.fstat(descriptor)
    if (
        not stat.S_ISREG(identity.st_mode)
        or stat.S_IMODE(identity.st_mode) != 0o600
        or identity.st_uid != os.geteuid()
        or identity.st_nlink != 1
        or identity.st_size > MAX_ARTIFACT_BYTES
    ):
        os.close(descriptor)
        raise ArtifactFailure("artifact-invalid", False)
    return _FileLink(parent_fd, name, descriptor, identity)


def _read_file(link: _FileLink) -> bytes:
    os.lseek(link.file_fd, 0, os.SEEK_SET)
    remaining = link.identity.st_size
    chunks: list[bytes] = []
    while remaining:
        chunk = os.read(link.file_fd, min(remaining, 1024 * 1024))
        if not chunk:
            raise ArtifactFailure("artifact-changed", False)
        chunks.append(chunk)
        remaining -= len(chunk)
    if not _same_identity(link.identity, os.fstat(link.file_fd)):
        raise ArtifactFailure("artifact-changed", False)
    return b"".join(chunks)


def _require_links(directories: list[_DirectoryLink], files: list[_FileLink]) -> None:
    for link in directories:
        try:
            current = os.stat(link.name, dir_fd=link.parent_fd, follow_symlinks=False)
        except OSError as exc:
            raise ArtifactFailure("artifact-changed", False) from exc
        if not _same_identity(link.identity, current):
            raise ArtifactFailure("artifact-changed", False)
    for link in files:
        try:
            current = os.stat(link.name, dir_fd=link.parent_fd, follow_symlinks=False)
        except OSError as exc:
            raise ArtifactFailure("artifact-changed", False) from exc
        if not _same_identity(link.identity, current):
            raise ArtifactFailure("artifact-changed", False)


def _require_snapshot(link: _FileLink, expected: bytes, expected_digest: str) -> None:
    current = _read_file(link)
    if current != expected or hashlib.sha256(current).hexdigest() != expected_digest:
        raise ArtifactFailure("artifact-changed", False)


def _write_snapshot(path: Path, data: bytes) -> None:
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(data)


def _validate_snapshot(
    meeting_id: str,
    note_id: str,
    transcript_id: str,
    markdown_id: str,
    note_bytes: bytes,
    transcript_bytes: bytes,
    markdown_bytes: bytes,
) -> None:
    from summarize import structured_artifact_citations, validate_artifact_pair
    from transcript import load

    with tempfile.TemporaryDirectory(prefix="lmn-note-inspect-") as temporary:
        root = Path(temporary)
        root.chmod(0o700)
        note_path = root / f"{note_id}.json"
        transcript_path = root / f"{transcript_id}.json"
        markdown_path = root / f"{markdown_id}.md"
        _write_snapshot(note_path, note_bytes)
        _write_snapshot(transcript_path, transcript_bytes)
        _write_snapshot(markdown_path, markdown_bytes)
        try:
            document = json.loads(note_bytes)
            if not isinstance(document, dict):
                raise ArtifactFailure("artifact-invalid", False)
            if document.get("schema") != "note/2" or document.get("passed") is not True:
                raise ArtifactFailure("artifact-invalid", False)
            meeting = document.get("meeting")
            if not isinstance(meeting, dict) or meeting.get("id") != meeting_id:
                raise ArtifactFailure("artifact-invalid", False)
            if document.get("transcript") != f"../transcript/{transcript_id}.json":
                raise ArtifactFailure("artifact-invalid", False)
            render = document.get("render")
            if not isinstance(render, dict) or render.get("path") != f"{markdown_id}.md":
                raise ArtifactFailure("artifact-invalid", False)
            transcript = load(transcript_path)
            validated_markdown = validate_artifact_pair(document, note_path, transcript)
            if validated_markdown != markdown_path:
                raise ArtifactFailure("artifact-invalid", False)
            citations = structured_artifact_citations(document, transcript)
            checks = document.get("checks")
            if not isinstance(checks, dict) or checks.get("citations") != citations:
                raise ArtifactFailure("artifact-invalid", False)
        except ArtifactFailure:
            raise
        except (
            UnicodeError,
            ValueError,
            KeyError,
            TypeError,
            IndexError,
            SystemExit,
        ) as exc:
            raise ArtifactFailure("artifact-invalid", False) from exc


def forbidden_in_claim(character: str) -> bool:
    """Characters a displayed claim may not contain, matching `note_projection.rs`.

    Unicode category Cc is exactly Rust's `char::is_control()` — U+0000..U+001F
    and U+007F..U+009F — and the two separators are category Zl and Zp, so they
    are named rather than derived. The rule lives on both sides of the boundary
    and neither is the other's fallback.

    Why a divergence here costs more than a refusal, which is the reason the
    rule is stated rather than left to the other side. Refusing is cheap: a
    declared `artifact-invalid` becomes `ProjectionError::ArtifactInvalid`, then
    `MeetingInspectionError::Quarantine`, and the rebuild counts one quarantined
    meeting and continues. Diverging is not: a claim this validator *accepts*
    and the Rust parser rejects returns `ProjectionError::Unavailable`, which
    becomes `LibraryReadError::ArtifactUnavailable` and ends the whole rebuild,
    content-free, naming no meeting. The failure worth engineering against is
    therefore not refusing too much here — it is admitting something the far
    side will not parse.

    That paragraph is a claim about code this file does not own. Verified
    2026-08-14 at `note_projection.rs:220` for the refusal, and
    `library_read.rs:1383-1389` and `:484-487` for both mappings. Re-derive it
    before trusting it; nothing here fails when it goes stale.

    Category Cs — a lone surrogate, which `json.loads` will happily produce from
    a `\\ud800` escape — is refused here as a rule rather than as an accident.
    It was already refused, but only because computing the claim digest encodes
    to UTF-8 and that raises, so the refusal depended on where the digest check
    sat in a boolean chain. Rust reaches the same end by a different mechanism:
    serde_json rejects the escape at parse, so a surrogate never becomes a
    `char` there. Same behaviour, two unrelated enforcement points, worth naming
    because neither would move if the other changed.
    """
    return (
        unicodedata.category(character) in {"Cc", "Cs"}
        or character in "\u2028\u2029"
    )


def validate_locators(evidence_refs: list, transcript) -> list[dict]:
    """Re-derive one point's locators against the transcript it cites.

    The note/2 evidence rule, in one place: one to three references, in order,
    without duplicates, each bounds-checked against the loaded turns and each
    `text_sha256` recomputed from the transcript's own bytes. Nothing supplied
    is trusted, so a reference naming a turn or a span the transcript does not
    have cannot pass.

    The one-to-three bound is enforced twice, by two owners. This is the note/2
    artifact contract, and `note_projection.rs` re-parses it independently at
    `parse_claim`, failing closed with a content-free `Unavailable`. Relaxing it
    here would not relax it there: a wider view motivating more locators has to
    move both, and the Rust side is the one that will not follow a Python edit.
    """
    locators = []
    for reference in evidence_refs:
        turn = reference["turn"]
        start = reference["char_start"]
        end = reference["char_end"]
        if (
            type(turn) is not int
            or type(start) is not int
            or type(end) is not int
            or turn < 0
            or start < 0
            or end <= start
            or turn >= len(transcript.turns)
            or end > len(transcript.turns[turn].text)
        ):
            raise ArtifactFailure("artifact-invalid", False)
        text_sha256 = hashlib.sha256(
            transcript.turns[turn].text[start:end].encode("utf-8")
        ).hexdigest()
        if reference.get("text_sha256") != text_sha256:
            raise ArtifactFailure("artifact-invalid", False)
        locators.append(
            {
                "turn": turn,
                "start": start,
                "end": end,
                "text_sha256": text_sha256,
            }
        )
    if not 1 <= len(locators) <= 3 or locators != sorted(
        locators, key=lambda locator: (
            locator["turn"], locator["start"], locator["end"], locator["text_sha256"]
        )
    ) or len({tuple(locator.items()) for locator in locators}) != len(locators):
        raise ArtifactFailure("artifact-invalid", False)
    return locators


def validate_claim_rows(cited: list, transcript) -> list[dict]:
    """Re-derive every stored claim and its locators against the transcript.

    No length cap on `claim`: candidate-first claims are verbatim transcript
    excerpts (`summarize.py`'s `validate_candidate_evidence`), unbounded by
    `candidate_first.py`'s fragment spans, and write time never caps them
    either -- a cap here only reproduced the older LLM-extraction contract's
    `MAX_STRUCTURED_CLAIM_CHARS`, which is not this contract's rule.
    """
    claims = []
    for ordinal, row in enumerate(cited):
        claim = row["claim"]
        claim_type = row["type"]
        if (
            not isinstance(claim, str)
            or not claim
            or any(forbidden_in_claim(character) for character in claim)
            or claim_type not in {
                "summary", "decision", "action", "proposal", "question", "point"
            }
            or row.get("claim_sha256") != hashlib.sha256(claim.encode("utf-8")).hexdigest()
        ):
            raise ArtifactFailure("artifact-invalid", False)
        claims.append(
            {
                "claim_ordinal": ordinal,
                "claim_sha256": row["claim_sha256"],
                "claim_type": claim_type,
                "evidence_state": "located",
                "claim": claim,
                "locators": validate_locators(row["evidence_refs"], transcript),
            }
        )
    return claims


def _project_snapshot(
    meeting_id: str,
    note_id: str,
    transcript_id: str,
    markdown_id: str,
    note_bytes: bytes,
    transcript_bytes: bytes,
    markdown_bytes: bytes,
) -> dict:
    """Re-derive a note/2 claim projection without writing private bytes to disk."""
    from summarize import (
        reconcile_capture_provenance,
        structured_artifact_citations,
        validate_note_render,
        validate_stored_verdict,
    )
    from transcript import load_bytes

    try:
        document = json.loads(note_bytes)
        if not isinstance(document, dict):
            raise ArtifactFailure("artifact-invalid", False)
        if document.get("schema") != "note/2" or document.get("passed") is not True:
            raise ArtifactFailure("artifact-invalid", False)
        meeting = document.get("meeting")
        if not isinstance(meeting, dict) or meeting.get("id") != meeting_id:
            raise ArtifactFailure("artifact-invalid", False)
        if document.get("transcript") != f"../transcript/{transcript_id}.json":
            raise ArtifactFailure("artifact-invalid", False)
        render = document.get("render")
        if not isinstance(render, dict) or render.get("path") != f"{markdown_id}.md":
            raise ArtifactFailure("artifact-invalid", False)
        transcript = load_bytes(transcript_bytes, source=f"transcript:{transcript_id}")
        checks = document.get("checks")
        validate_stored_verdict(checks, document.get("passed"), "note candidate")
        reconcile_capture_provenance(document, transcript, where="note candidate")
        validate_note_render(document, markdown_bytes.decode("utf-8"))
        citations = structured_artifact_citations(document, transcript)
        if not isinstance(checks, dict) or checks.get("citations") != citations:
            raise ArtifactFailure("artifact-invalid", False)
        claims = validate_claim_rows(citations["cited"], transcript)
        return {
            "schema": "note-claim-projection/1",
            "note_json_sha256": note_id,
            "note_markdown_sha256": markdown_id,
            "transcript_sha256": transcript_id,
            "claims": claims,
        }
    except ArtifactFailure:
        raise
    except (
        UnicodeError,
        ValueError,
        KeyError,
        TypeError,
        IndexError,
        SystemExit,
    ) as exc:
        raise ArtifactFailure("artifact-invalid", False) from exc


def _response_refusal(raw: str) -> str:
    """Name which half of the response contract failed, without content."""
    try:
        json.loads(raw)
    except (UnicodeError, ValueError):
        return "response-json-syntax"
    return "response-contract"


def _present_classification_user(
    user: str,
    transcript,
    overlay,
) -> str:
    """Replace only excerpt values in an already source-bound request.

    Candidate IDs, cue offsets, fragment spans, and the manifest digest remain
    those of the retained transcript. This is the source-to-presentation
    translation boundary: model text may change length, but evidence offsets
    never do.
    """
    from summarize import build_fragment_map

    prefix, encoded = user.split("\n\n", 1)
    packets = json.loads(encoded)
    fragments = {
        row["source_fragment_id"]: row
        for row in build_fragment_map(transcript)["fragments"]
    }
    for packet in packets:
        for row in packet["fragments"]:
            fragment = fragments[row["source_fragment_id"]]
            turn = fragment["turn"]
            row["text"] = overlay.text(turn, fragment["char_start"], fragment["char_end"])
            speaker = overlay.speaker(turn)
            if transcript.attribution != "none" and speaker:
                row["speaker"] = speaker
            else:
                row.pop("speaker", None)
        cue = packet["cue"]
        if cue is not None:
            # Candidate-first's cue offsets are source offsets. Render the
            # model-only value through the same mapping, without mutating the
            # offset fields that the manifest already authenticated.
            anchor = next(
                item for item in packet["fragments"] if item["anchor"]
            )
            fragment = fragments[anchor["source_fragment_id"]]
            cue["text"] = overlay.text(
                fragment["turn"], cue["char_start"], cue["char_end"]
            )
    return prefix + "\n\n" + json.dumps(
        packets, ensure_ascii=False, separators=(",", ":")
    )


def _classify_candidates(transcript, overlay, ask: Callable[[dict], str]) -> tuple[dict, list[dict]]:
    """Enumerate candidates locally, then take one verdict per offered candidate.

    This is the measured task, not a paraphrase of it. Deterministic local code
    builds the candidate manifest and every request packet; the model's entire
    output surface is one KEEP or ABSTAIN per candidate it was offered. It
    cannot name a row it was not shown, invent a locator, or decide how many
    points exist — `decode_classification` refuses an unknown, duplicated,
    reordered, or miscounted verdict before anything reaches the transcript.

    The configuration is `candidate_first.PRODUCT_RUN`, the registration the
    operator adopted for the product lane: the ±2 visible window, batch size 1,
    temperature 0, and the contiguous-run pruning stage. Two consequences that
    are easy to get wrong. Raw keeps legitimately run into the hundreds at
    batch size 1 — the measured cells kept 71–152 of 165–300 — so a cumulative
    keep budget checked as verdicts accumulate would refuse every real meeting;
    the registered gate is `maximum_keep_after_prune`, applied once, to the
    pruned set. And the pruner needs one verdict per candidate, so every
    decision is retained rather than only the keeps.
    """
    import candidate_first
    from summarize import StructuredOutputError

    registered = candidate_first.PRODUCT_RUN["classifier"]
    budget = candidate_first.PRODUCT_RUN["gates"]["maximum_keep_after_prune"]
    pruner = candidate_first.PRODUCT_RUN["pruner"]
    batch_size = registered["batch_size"]
    try:
        manifest = candidate_first.generate_manifest(
            transcript,
            candidate_first.STRATEGY_BROAD,
            contract=candidate_first.PRODUCT_CONTRACT,
        )
        candidate_first.validate_manifest(manifest, transcript)
        offered = candidate_first.offered_candidates(
            manifest["candidates"], registered["offer_stride"])
        batches = candidate_first.candidate_batches(offered, batch_size)
    except (StructuredOutputError, ValueError, KeyError, TypeError, IndexError) as exc:
        raise GenerationRefused("no-generatable-transcript", True) from exc
    if not batches:
        raise GenerationRefused("no-generatable-transcript", True)
    decisions: list[dict] = []
    for batch in batches:
        candidate_ids = [row["candidate_id"] for row in batch]
        try:
            schema, system, user = candidate_first.classification_request(
                transcript, manifest, batch, batch_size,
                offer_stride=registered["offer_stride"],
            )
            user = _present_classification_user(user, transcript, overlay)
        except (StructuredOutputError, ValueError, KeyError, TypeError, IndexError) as exc:
            raise GenerationRefused("request-contract", False) from exc
        raw = ask(
            {
                "schema": "note-classification-request/1",
                "system": system,
                "user": user,
                "response_format": schema,
                "num_predict": candidate_first.classification_num_predict(len(batch)),
                "num_ctx": TRANSPORT_NUM_CTX,
                "temperature": registered["temperature"],
            }
        )
        try:
            decoded = candidate_first.decode_classification(raw, candidate_ids)
        except (StructuredOutputError, UnicodeError, ValueError, KeyError, TypeError) as exc:
            # `decode_classification` reports one error type for both "this is
            # not JSON" and "this is JSON that breaks the contract". The
            # research taxonomy separates them, so the discriminator is a
            # parse attempt — not a second copy of the decoder.
            raise GenerationRefused(_response_refusal(raw), False) from exc
        decisions.extend(decoded["items"])
    if not any(row["verdict"] == "KEEP" for row in decisions):
        raise GenerationRefused("no-model-candidates", True)
    try:
        pruned = candidate_first.prune_keeps(
            offered, decisions,
            budget=pruner["budget"], stride_floor=pruner["stride_floor"],
            max_gap=pruner["max_gap"])
    except (StructuredOutputError, ValueError, KeyError, TypeError) as exc:
        # Not a model refusal. `decode_classification` already guaranteed exact
        # single coverage of every offered locator, batch by batch, so a pruner
        # that rejects the assembled decision set means local code built the
        # batches or the manifest wrongly — the same class as a request the
        # contract could not build.
        raise GenerationRefused("request-contract", False) from exc
    if pruned["counts"]["pruned_keep"] > budget:
        raise GenerationRefused("keep-budget-exceeded", False)
    rows = {row["candidate_id"]: row for row in manifest["candidates"]}
    # `pruned_candidate_ids` is already in ordinal order — the pruner sorts
    # keeps by ordinal, walks runs in that order, and emits one representative
    # per run — so the points that follow are in transcript order.
    kept = [rows[candidate_id] for candidate_id in pruned["pruned_candidate_ids"]]
    return manifest, kept


def locate_kept_candidates(manifest: dict, kept: list[dict], transcript) -> list[dict]:
    """Resolve every kept candidate to its anchor locator, and verify it here.

    The excerpts are exact by construction — they are transcript slices the
    enumerator produced — and they are re-derived anyway. `validate_locators`
    recomputes each span's digest from the loaded turns, so a fragment id that
    no longer resolves to the bytes it names refuses rather than rendering.

    A point cites its **anchor**, not the whole window it was classified in.
    `visible_fragment_ids` is what the model was shown for context; the anchor
    is what the candidate is about, which is why the deterministic control arm
    cites the anchor alone. Two consequences follow. The point never cites text
    the model merely saw nearby, and the locator count stays 1 whatever the view
    is — a ±2 window would offer five fragments and blow note/2's three-locator
    rule if the window were the citation.
    """
    from summarize import build_fragment_map

    fragment_map = build_fragment_map(transcript)
    lookup = {row["source_fragment_id"]: row for row in fragment_map["fragments"]}
    if fragment_map["transcript_view_sha256"] != manifest["transcript_view_sha256"]:
        raise GenerationRefused("citation-locator", False)
    points = []
    for ordinal, candidate in enumerate(kept):
        try:
            anchor = lookup[candidate["anchor_fragment_id"]]
            locators = validate_locators(
                [
                    {
                        "turn": anchor["turn"],
                        "char_start": anchor["char_start"],
                        "char_end": anchor["char_end"],
                        "text_sha256": anchor["text_sha256"],
                    }
                ],
                transcript,
            )
        except ArtifactFailure as exc:
            # The retained transcript is intact; the candidate no longer
            # resolves against it. Not a storage code.
            raise GenerationRefused("citation-locator", False) from exc
        except (ValueError, KeyError, TypeError, IndexError) as exc:
            raise GenerationRefused("citation-locator", False) from exc
        points.append(
            {
                "point_ordinal": ordinal,
                "candidate_id": candidate["candidate_id"],
                "evidence_state": "located",
                "locators": locators,
            }
        )
    return points


def _strip_json_fence(content: str) -> str:
    """Accept the model's common fenced-JSON wrapper without trusting prose."""
    value = content.strip()
    if value.startswith("```"):
        lines = value.splitlines()
        if lines and lines[0].strip().lower() in {"```", "```json"}:
            lines = lines[1:]
        if lines and lines[-1].strip() == "```":
            lines = lines[:-1]
        value = "\n".join(lines).strip()
    return value


def _synthesis_source_rows(manifest: dict, kept: list[dict], transcript, overlay) -> list[dict]:
    from summarize import build_fragment_map

    fragments = {
        row["source_fragment_id"]: row
        for row in build_fragment_map(transcript)["fragments"]
    }
    rows = []
    for index, candidate in enumerate(kept, 1):
        anchor = fragments[candidate["anchor_fragment_id"]]
        rows.append(
            {
                "alias": f"E{index:02d}",
                "candidate_id": candidate["candidate_id"],
                "text": overlay.text(
                    anchor["turn"], anchor["char_start"], anchor["char_end"]
                ),
            }
        )
    return rows


def _usable_synthesis_text(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    text = " ".join(value.split()).strip()
    if (
        not text
        or len(text) > SYNTHESIS_MAX_CLAIM_CHARS
        or any(forbidden_in_claim(character) for character in text)
    ):
        return None
    return text


def _setup_only_claim(claim_type: str, text: str) -> bool:
    if claim_type not in {"decision", "action"}:
        return False
    normalized = re.sub(r"[^a-z0-9 ]+", " ", text.lower())
    normalized = " ".join(normalized.split())
    return bool(
        re.fullmatch(
            r"(?:the team |the meeting |we |everyone )?(?:is |are |was |were )?good to go",
            normalized,
        )
        or re.search(r"\b(?:start|stop|check|test)(?:ing|ed)? (?:the )?(?:recording|audio|microphone)\b", normalized)
    )


def _outcome_evidence_supports(claim_type: str, source_texts: list[str]) -> bool:
    """Keep decisions and actions only when the cited words carry the outcome.

    Summary, proposal, and question rows may synthesize a topic across excerpts.
    A decision or action changes what the reader believes happened, so a model
    label alone is not enough: the cited transcript must include an explicit
    agreement or commitment signal.
    """
    if claim_type not in {"decision", "action"}:
        return True
    evidence = " ".join(source_texts).lower()
    if claim_type == "decision":
        return bool(re.search(
            r"\b(?:agree(?:d)?|decid(?:e|ed)|approv(?:e|ed)|settle(?:d)?|"
            r"locked in|go with|going with|move forward with|that's the plan)\b",
            evidence,
        ))
    return bool(re.search(
        r"\b(?:(?:i|we)\s+(?:will|'ll|am going to|are going to)|let me)\b",
        evidence,
    ))


def _decode_synthesis(raw: str, sources: list[dict], points: list[dict]) -> list[dict]:
    """Admit useful rows from untrusted model prose and resolve their evidence.

    One malformed row does not erase the rest of a meeting note. Unknown IDs
    are discarded, repeated IDs collapse, and the first three valid IDs become
    the note/2 evidence bound. The whole proposal still refuses when no
    evidence-linked overview survives.
    """
    try:
        envelope = json.loads(raw)
        if not isinstance(envelope, dict) or set(envelope) != {"content"}:
            raise ValueError
        content = envelope["content"]
        if not isinstance(content, str):
            raise ValueError
        proposal = json.loads(_strip_json_fence(content))
    except (UnicodeError, ValueError, TypeError) as exc:
        raise GenerationRefused("response-json-syntax", False) from exc
    if not isinstance(proposal, dict) or set(proposal) != {"overview", "items"}:
        raise GenerationRefused("response-contract", False)
    if not isinstance(proposal["overview"], list) or not isinstance(proposal["items"], list):
        raise GenerationRefused("response-contract", False)

    source_by_alias = {row["alias"]: row for row in sources}
    point_by_candidate = {row["candidate_id"]: row for row in points}
    claims: list[dict] = []
    seen: set[tuple[str, str]] = set()

    def admit(row: object, claim_type: str) -> None:
        if not isinstance(row, dict):
            return
        expected = {"text", "evidence_ids"} if claim_type == "summary" else {
            "type", "text", "evidence_ids"
        }
        if set(row) != expected:
            return
        text = _usable_synthesis_text(row.get("text"))
        if text is None or _setup_only_claim(claim_type, text):
            return
        aliases = row.get("evidence_ids")
        if not isinstance(aliases, list):
            return
        valid_aliases: list[str] = []
        for alias in aliases:
            if (
                isinstance(alias, str)
                and alias in source_by_alias
                and alias not in valid_aliases
            ):
                valid_aliases.append(alias)
            if len(valid_aliases) == 3:
                break
        if not valid_aliases:
            return
        if not _outcome_evidence_supports(
            claim_type,
            [source_by_alias[alias]["text"] for alias in valid_aliases],
        ):
            return
        key = (claim_type, text.casefold())
        if key in seen:
            return
        seen.add(key)
        candidate_ids = [source_by_alias[alias]["candidate_id"] for alias in valid_aliases]
        locators = []
        locator_keys = set()
        for candidate_id in candidate_ids:
            for locator in point_by_candidate[candidate_id]["locators"]:
                locator_key = (
                    locator["turn"], locator["start"], locator["end"], locator["text_sha256"]
                )
                if locator_key not in locator_keys:
                    locator_keys.add(locator_key)
                    locators.append(locator)
        locators.sort(key=lambda value: (
            value["turn"], value["start"], value["end"], value["text_sha256"]
        ))
        claims.append(
            {
                "claim_ordinal": len(claims),
                "claim_type": claim_type,
                "claim": text,
                "candidate_ids": candidate_ids,
                "evidence_state": "located",
                "locators": locators,
            }
        )

    for row in proposal["overview"][:SYNTHESIS_MAX_OVERVIEW]:
        admit(row, "summary")
    overview_count = len(claims)
    for row in proposal["items"]:
        if len(claims) - overview_count >= SYNTHESIS_MAX_ITEMS:
            break
        if not isinstance(row, dict):
            continue
        claim_type = row.get("type")
        if claim_type in {"decision", "action", "proposal", "question"}:
            admit(row, claim_type)
    if overview_count == 0:
        raise GenerationRefused("no-model-summary", True)
    return claims


def synthesize_note(
    transcript,
    manifest: dict,
    kept: list[dict],
    points: list[dict],
    overlay,
    ask: Callable[[dict], str],
    pre_meeting_context: str | None = None,
) -> list[dict]:
    """Roadmap intake I3. `pre_meeting_context` is the operator's own labeled
    framing, typed before or during capture -- never transcript-backed, and
    absent on every call site that predates this parameter. When absent the
    `user` prompt this builds is byte-identical to what it was before this
    parameter existed; the section below is prepended only when present.

    The governing constraint, verbatim: context is a labeled operator input;
    it never appears as transcript-backed generated content. This function
    cannot enforce that by itself -- the model still receives free text and
    could ignore the instruction below -- so the actual enforcement is
    `_decode_synthesis`'s alias resolution and, one layer up, Rust's
    `note_projection.rs::parse_claim`: every claim still needs 1-3 locators
    whose `text_sha256` verifies against the retained transcript, and context
    never enters `sources` (the only alias-bearing rows `ask` is offered), so
    it can supply no `evidence_ids` a claim could cite. A context-only
    assertion has nothing to resolve against and is dropped exactly like any
    other unsupported row.
    """
    sources = _synthesis_source_rows(manifest, kept, transcript, overlay)
    context_section = ""
    if isinstance(pre_meeting_context, str) and pre_meeting_context:
        context_section = (
            "OPERATOR-PROVIDED MEETING CONTEXT (framing only -- never meeting"
            " content, never evidence, never quotable as either; use only to judge"
            " which excerpts below matter more):\n" + pre_meeting_context + "\n\n"
        )
    user = context_section + "SELECTED TRANSCRIPT EXCERPTS:\n" + "\n".join(
        json.dumps(
            {"id": row["alias"], "text": row["text"]},
            ensure_ascii=False,
            separators=(",", ":"),
        )
        for row in sources
    )
    raw = ask(
        {
            "schema": "note-synthesis-request/1",
            "system": SYNTHESIS_SYSTEM,
            "user": user,
            "num_predict": SYNTHESIS_NUM_PREDICT,
            "num_ctx": TRANSPORT_NUM_CTX,
            "temperature": 0,
        }
    )
    return _decode_synthesis(raw, sources, points)


def generate(
    root_fd: int,
    arguments: dict,
    *,
    ask: Callable[[dict], str],
    after_open: Callable[[], None] | None = None,
) -> dict:
    """Select transcript evidence, then synthesize a usable meeting note.

    The transcript is opened, digest-checked, and held open exactly as the
    read-only paths do. `ask` is the injected model seam: it takes one built
    classification request and returns the raw response. The caller owns the
    transport and may add transport-only fields to it — the bridge attaches the
    verified model directory to every request — but nothing it adds is read
    back here. No note, markdown, or product record is read or written, and the
    transcript's identity is re-checked after classification, so a swap mid-run
    is caught.

    The first stage preserves the measured candidate-selection lane. The
    second stage proposes overview and outcome prose against aliases for those
    selected excerpts. Local code resolves every admitted alias back to exact
    transcript locators before anything can be persisted.
    """
    from transcript import load_bytes

    meeting_id = arguments["meeting_id"]
    transcript_id = arguments["transcript_id"]
    directories: list[_DirectoryLink] = []
    files: list[_FileLink] = []
    open_directories: list[int] = []
    try:
        transcript_fd, transcript_links = _open_directories(
            root_fd, ["meetings", meeting_id, "transcript"]
        )
        open_directories.append(transcript_fd)
        directories.extend(transcript_links)
        transcript_file = _open_file(transcript_fd, f"{transcript_id}.json")
        files.append(transcript_file)
        if after_open is not None:
            after_open()
        transcript_bytes = _read_file(transcript_file)
        _require_links(directories, files)
        if hashlib.sha256(transcript_bytes).hexdigest() != transcript_id:
            raise ArtifactFailure("artifact-changed", False)
        try:
            transcript = load_bytes(transcript_bytes, source=f"transcript:{transcript_id}")
        except (UnicodeError, ValueError, KeyError, TypeError, IndexError) as exc:
            raise ArtifactFailure("artifact-invalid", False) from exc
        # Build the prompt-only view only after retained bytes have passed
        # their digest check. It retains original source spans even when a
        # vocabulary replacement changes text length; note artifacts and
        # locators keep naming the immutable `transcript_id` source.
        from transcript import PromptOverlay
        try:
            overlay = PromptOverlay.from_transport(
                transcript,
                arguments.get("speaker_label_overrides"),
                arguments.get("vocabulary_replacements"),
            )
        except (KeyError, TypeError, ValueError, UnicodeError) as exc:
            raise GenerationRefused("vocabulary-overlay", False) from exc
        if not transcript.turns:
            raise GenerationRefused("no-generatable-transcript", True)
        manifest, kept = _classify_candidates(transcript, overlay, ask)
        points = locate_kept_candidates(manifest, kept, transcript)
        claims = synthesize_note(
            transcript, manifest, kept, points, overlay, ask,
            pre_meeting_context=arguments.get("pre_meeting_context"),
        )
        _require_links(directories, files)
        _require_snapshot(transcript_file, transcript_bytes, transcript_id)
        _require_links(directories, files)
        return {
            "schema": "note-generation/2",
            "transcript_sha256": transcript_id,
            "manifest_sha256": manifest["manifest_sha256"],
            "candidates": len(manifest["candidates"]),
            "claims": claims,
        }
    finally:
        for link in files:
            os.close(link.file_fd)
        for descriptor in open_directories:
            os.close(descriptor)
        for link in reversed(directories):
            os.close(link.child_fd)
            os.close(link.parent_fd)


def inspect(
    root_fd: int,
    arguments: dict,
    *,
    after_open: Callable[[], None] | None = None,
) -> dict[str, str]:
    """Inspect one coherent snapshot of the three content-addressed artifacts."""
    meeting_id = arguments["meeting_id"]
    note_id = arguments["note_id"]
    transcript_id = arguments["transcript_id"]
    directories: list[_DirectoryLink] = []
    files: list[_FileLink] = []
    open_directories: list[int] = []
    try:
        notes_fd, notes_links = _open_directories(root_fd, ["meetings", meeting_id, "notes"])
        open_directories.append(notes_fd)
        directories.extend(notes_links)
        transcript_fd, transcript_links = _open_directories(
            root_fd, ["meetings", meeting_id, "transcript"]
        )
        open_directories.append(transcript_fd)
        directories.extend(transcript_links)
        note = _open_file(notes_fd, f"{note_id}.json")
        transcript = _open_file(transcript_fd, f"{transcript_id}.json")
        files.extend((note, transcript))
        note_bytes = _read_file(note)
        try:
            document = json.loads(note_bytes)
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise ArtifactFailure("artifact-invalid", False) from exc
        if not isinstance(document, dict):
            raise ArtifactFailure("artifact-invalid", False)
        render = document.get("render")
        render_path = render.get("path") if isinstance(render, dict) else None
        if (
            not isinstance(render_path, str)
            or len(render_path) != 67
            or not render_path.endswith(".md")
            or any(character not in "0123456789abcdef" for character in render_path[:-3])
        ):
            raise ArtifactFailure("artifact-invalid", False)
        markdown_id = render_path[:-3]
        markdown = _open_file(notes_fd, render_path)
        files.append(markdown)
        if after_open is not None:
            after_open()
        transcript_bytes = _read_file(transcript)
        markdown_bytes = _read_file(markdown)
        _require_links(directories, files)
        if hashlib.sha256(note_bytes).hexdigest() != note_id:
            raise ArtifactFailure("artifact-changed", False)
        if hashlib.sha256(transcript_bytes).hexdigest() != transcript_id:
            raise ArtifactFailure("artifact-changed", False)
        if hashlib.sha256(markdown_bytes).hexdigest() != markdown_id:
            raise ArtifactFailure("artifact-changed", False)
        _validate_snapshot(
            meeting_id,
            note_id,
            transcript_id,
            markdown_id,
            note_bytes,
            transcript_bytes,
            markdown_bytes,
        )
        _require_links(directories, files)
        _require_snapshot(note, note_bytes, note_id)
        _require_snapshot(transcript, transcript_bytes, transcript_id)
        _require_snapshot(markdown, markdown_bytes, markdown_id)
        _require_links(directories, files)
        return {
            "note": note_id,
            "note-markdown": markdown_id,
            "transcript": transcript_id,
        }
    finally:
        for link in files:
            os.close(link.file_fd)
        for descriptor in open_directories:
            os.close(descriptor)
        for link in reversed(directories):
            os.close(link.child_fd)
            os.close(link.parent_fd)


def project(
    root_fd: int,
    arguments: dict,
    *,
    after_open: Callable[[], None] | None = None,
) -> dict:
    """Return a coherent, fully re-derived projection from retained descriptors."""
    meeting_id = arguments["meeting_id"]
    note_id = arguments["note_id"]
    transcript_id = arguments["transcript_id"]
    directories: list[_DirectoryLink] = []
    files: list[_FileLink] = []
    open_directories: list[int] = []
    try:
        notes_fd, notes_links = _open_directories(root_fd, ["meetings", meeting_id, "notes"])
        open_directories.append(notes_fd)
        directories.extend(notes_links)
        transcript_fd, transcript_links = _open_directories(
            root_fd, ["meetings", meeting_id, "transcript"]
        )
        open_directories.append(transcript_fd)
        directories.extend(transcript_links)
        note = _open_file(notes_fd, f"{note_id}.json")
        transcript = _open_file(transcript_fd, f"{transcript_id}.json")
        files.extend((note, transcript))
        note_bytes = _read_file(note)
        try:
            document = json.loads(note_bytes)
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise ArtifactFailure("artifact-invalid", False) from exc
        if not isinstance(document, dict):
            raise ArtifactFailure("artifact-invalid", False)
        render = document.get("render")
        render_path = render.get("path") if isinstance(render, dict) else None
        if (
            not isinstance(render_path, str)
            or len(render_path) != 67
            or not render_path.endswith(".md")
            or any(character not in "0123456789abcdef" for character in render_path[:-3])
        ):
            raise ArtifactFailure("artifact-invalid", False)
        markdown_id = render_path[:-3]
        markdown = _open_file(notes_fd, render_path)
        files.append(markdown)
        if after_open is not None:
            after_open()
        transcript_bytes = _read_file(transcript)
        markdown_bytes = _read_file(markdown)
        _require_links(directories, files)
        if hashlib.sha256(note_bytes).hexdigest() != note_id:
            raise ArtifactFailure("artifact-changed", False)
        if hashlib.sha256(transcript_bytes).hexdigest() != transcript_id:
            raise ArtifactFailure("artifact-changed", False)
        if hashlib.sha256(markdown_bytes).hexdigest() != markdown_id:
            raise ArtifactFailure("artifact-changed", False)
        projection = _project_snapshot(
            meeting_id,
            note_id,
            transcript_id,
            markdown_id,
            note_bytes,
            transcript_bytes,
            markdown_bytes,
        )
        _require_links(directories, files)
        _require_snapshot(note, note_bytes, note_id)
        _require_snapshot(transcript, transcript_bytes, transcript_id)
        _require_snapshot(markdown, markdown_bytes, markdown_id)
        _require_links(directories, files)
        return projection
    finally:
        for link in files:
            os.close(link.file_fd)
        for descriptor in open_directories:
            os.close(descriptor)
        for link in reversed(directories):
            os.close(link.child_fd)
            os.close(link.parent_fd)
