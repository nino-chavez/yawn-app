# Diagnosis — Generate note "fails silently" on the build-21 preview

Date: 2026-09-03. Tree: `a371238`. Source only — the app was not run and the GUI
was not driven. Evidence is the repository, the preview's own storage tree read
read-only, and one local Python run over the retained transcript bytes.

## The answer, first

Two things were reported as one defect. They are separate, and the first one is
not true.

**The attempt was recorded.** Both Generate note clicks in the build-21 pass
wrote a complete, terminal operation receipt — request, result, and commit — in
about 23 seconds each. The receipts sit in the preview's `operations/` directory
with timestamps matching the clicks to the second. The claim that "nothing was
written anywhere" and that the failure "sits upstream of anything that writes,
logs, or forks" is false.

**Generation genuinely failed, for a reason the app is built to keep.** The
meeting's retained transcript has **zero turns**. It is a 28-second capture that
recorded clean audio on both legs and transcribed to nothing — silence. The
note validator refuses a transcript with no turns before it loads a model, the
bridge reports that as `transcript-only`, and the desktop turns that into a
rejected note. Every attempt on this meeting will fail the same way, forever,
because there is nothing in the transcript to summarize.

**What the reader sees is the real defect.** The meeting was already
`summary-failed` with no note. A rejected generation writes it back as
`summary-failed` with no note. The record is byte-identical, the reader's
response is a pure function of that record, and so the screen after a failed
retry is pixel-identical to the screen before it. The product brief requires
that "an interrupted or failed run is stated plainly." The run is stated
durably, in a place the operator never sees, and stated to the reader as
nothing at all.

## The cause, traced

The chain, in order, with the line that does each step.

**1. The transcript has no turns.**
`meetings/784c623d-…/transcript/d33a67cc….json` is `capture-transcript/1` with
`"turns": []`. `capture_health.usable` is `true`: both legs ran 28 s, no
dropouts, no tap errors, no blockers. The capture was healthy; nobody spoke.

**2. The validator refuses before any model is loaded.**
`worker/note_validator.py:956-957`

```python
if not transcript.turns:
    raise GenerationRefused("no-generatable-transcript", True)
```

This sits after the transcript digest check and the prompt-overlay build, and
**before** `_classify_candidates` at line 958 — which is the first thing that
would call `ask`.

Confirmed empirically against the real retained bytes, using `notes/transcript.py`
(the module the validator bundle carries as `transcript`):

```
turns = 0 | gated = 0 | attribution = channel
overlay built OK, no exception -> PromptOverlay
guard fires (not transcript.turns): True
```

`load_capture` (`notes/transcript.py:437-450`) maps the JSON `turns` array
one-for-one onto `Transcript.turns`; nothing derives or filters them beyond the
gated split, and there were no gated turns.

**3. No generator subprocess is spawned — by design, not by failure.**
`worker/note_bridge.py:1345-1355`: the `ask` closure constructs
`_GeneratorSession` lazily, on the first request, explicitly so that a refusal
that needs no model does not pay a multi-gigabyte load. The line-957 guard fires
before the first `ask`, so the mlx child is never created. This is the source-level
explanation for "no note-projector child was ever spawned". The *bridge* child did
run — a `transcript-only` frame is what the Rust side parsed.

**4. The refusal becomes a content-free product outcome.**
`worker/note_bridge.py:1363-1365` catches `validator.GenerationRefused` and calls
`_transcript_only(request_id, exc.code, exc.recoverable, session_receipt())`.
The frame carries `code="no-generatable-transcript"`, `recoverable=true`, and an
empty receipt (no session existed).

**5. The desktop discards the code.**
`apps/desktop/src-tauri/src/product_coordinator.rs:327-341`. The `code` is read
only inside a `note_trace_enabled()` guard and is otherwise dropped, along with
the receipt. The bridge's own docstring (`note_bridge.py:1198-1206`) says this is
deliberate: one outcome for every failure class, so a caller cannot tell them
apart by shape. The desktop returns
`NoteWorkerResult::Rejected(NoteCreateWorkerFailure { code: NoteRejected, recoverable, artifact_digests: {} })`.

**6. The durable result records the collapsed code.**
`crates/session-core/src/note_generation.rs:386-390` → `rejected_result`
(`:704-717`) → `failure_code: "note-rejected"`. `recoverable` is validated
(`operations.rs:467-478`) and then not carried into the result schema.

**7. The meeting record is rewritten to the state it already had.**
`note_generation.rs:548-556`: on `Rejected` with no prior note, lifecycle becomes
`SummaryFailed` and `current_note` becomes `None` — which is what they already
were. `write_meeting` then writes the same bytes.

**8. The reader's response is a pure function of that record.**
`apps/desktop/src-tauri/src/library_reader.rs:1413-1437` builds the
`summary-failed` response from the meeting record alone. It carries no attempt
count, no attempt timestamp, no failure code, no "we tried again". Two attempts
produce identical responses, so the document renders identically.

## On-disk confirmation

Preview storage, `com.ninochavez.local-meeting-notes.preview`. Read-only.

| Operation | Request written | Commit written | Meeting | Outcome |
|---|---|---|---|---|
| `a28f37b0-…` | Sep 2 15:28:53 | Sep 2 15:29:18 | 784c623d | note-rejected |
| `a1b2a83f-…` | Sep 3 09:28:52 | Sep 3 09:29:15 | 784c623d | note-rejected |
| `191f0465-…` | Sep 3 09:43:52 | Sep 3 09:44:15 | 784c623d | note-rejected |
| `c5a95793-…` | Sep 2 18:55:40 | Sep 2 18:56:35 | 40644bc2 | **accepted** |

The two Sep 3 request times are the two Generate note clicks in `capture.log`
(09:28:52 and 09:43:52) exactly. Each run took 23 seconds, not five minutes; the
frames captured 6.5 and 4.5 minutes after the click, and the `ps` read at ~09:48
was about four minutes after the second bridge child had already exited, which is
why it saw only the standing workers.

All three rejected commits carry the same `committed_meeting_sha256`:
`4d2e611ddfb39b4655f1d962e9ee239fd7b139b939b57e5c8638f11816fa81c0`. That is
direct proof that the reader-facing meeting record did not change across three
attempts spanning two days and two app launches.

`c5a95793` matters as a control: the same build generated and published a note
for meeting `40644bc2`, whose transcript has nine turns. The pipeline works. The
23 s / 55 s split is consistent with the failing runs verifying the 7.5 GB model
tree and stopping, while the accepted run also loaded the model and generated.

**The `pre_meeting_context` correlation is a red herring.** All three rejected
requests carry `"pre_meeting_context": " "` and the accepted one has no such
field, which looks like a lead. It is not: `pre_meeting_context` is read only in
`synthesize_note` (`note_validator.py:962` → `:846`), which is called at line
960 — downstream of the line-957 guard. A single space is also valid under both
bounds checks (`note_projector_process.rs:2094-2105`,
`note_bridge.py:1031-1051`). It cannot reach the failure on this path.

## Why each place that could have recorded the attempt did not

| Place | Why it did not fire |
|---|---|
| The bridge child's stderr | Nothing was written there. The refusal is a normal, structured result frame, not an error. The `[note-trace]` lines that would have named the stage require `YAWN_NOTE_TRACE` in the app's environment (`note_projector_process.rs:76-78`), which the directly-launched run did not set. Zero bytes of stderr is the expected output of a successful, correct run that produced a rejection. |
| `diagnostics/` | Nothing in note generation writes there. The directory is used by capture, transcription-queue recovery, and retention quarantine. A rejected note is a product outcome, not a fault, so no code path exists. |
| `attempt.json` | It is the capture-attestation artifact (consent, headphones, operator-alone, build hash), written once at capture. It has no relationship to note generation and correctly did not change. |
| The unified log | The app emits nothing to it on this path. |
| The meeting record | It was rewritten, to identical bytes (step 7). A `find` by mtime would see the write; a content comparison sees nothing. |
| `operations/` | **It did fire.** Three full receipts. |

I did not reproduce the `find` that reported zero files modified in the
surrounding two hours. The directory mtimes above contradict it. I do not know
whether it used a wrong time window or ran against
`com.ninochavez.local-meeting-notes` rather than the `.preview` domain — both
exist in Application Support.

## Nothing was fixed, and why

There is no swallowed exception and no discarded `Result` on this path. Every
discard is deliberate and carries a comment saying so. The three places that
could change are all product decisions, in descending sharpness:

1. **`library_reader.rs:1376-1382` offers regeneration without checking that the
   transcript has any turns.** `regeneration_source_sha256` is set for
   `Ready | TranscriptReady | SummaryFailed` with no turn-count condition, so
   Generate note is offered on a meeting that cannot produce one. This is the
   most contained change — Rust-side, one condition, no frontend file. The
   decision is whether the product should refuse up front ("this recording has
   no speech to summarize") or keep offering and report honestly afterwards.
   Both are defensible; the copy is a content-read either way.

2. **The child's failure code and receipt are dropped**
   (`product_coordinator.rs:327-341`). The bridge states the receipt is "closed
   and content-free by construction", which is the argument that keeping it is
   safe. What a durable diagnostic should contain, and whether the collapsed
   single outcome is still the right contract, is a product call.

3. **A repeat failure leaves the reader's record unchanged**
   (`note_generation.rs:548-556`, `library_reader.rs:1413`). Fixing this means
   deciding what the operator is told on a second failure — an attempt count, a
   timestamp, a reason, or a different state altogether.

## One adjacent hazard, not this bug

`NoteCreateWorkerFailure::validate` (`operations.rs:467-473`) rejects any
failure with `recoverable == false`. Several of the child's codes are
unrecoverable by construction — `response-json-syntax`, `keep-budget-exceeded`,
`citation-locator`, `note2-validation-failed`. On one of those,
`rejected_result` returns `Malformed`, `execute_request` propagates, and the
operation is left **nonterminal**: request written, no result, no commit. The
next Generate note on that meeting is then refused by the "another nonterminal
operation already exists" guard (`note_generation.rs:251-258`) — which is the
gap the roadmap already records under D-NOTE-STAGE, reachable by a second route.
Not observed on this device; raised from source only.

## What would falsify this

- A `transcript-only` frame carrying a code other than `no-generatable-transcript`
  for meeting `784c623d`. Set `YAWN_NOTE_TRACE=1` and click Generate note once:
  the child's code prints, and the whole chain above is either confirmed by name
  or refuted in one run.
- A turn count other than zero for `d33a67cc…`. It was read twice, from the
  retained file, once by hand and once through the validator's own loader.
- An operation receipt for either Sep 3 click that is missing, nonterminal, or
  carries a different outcome.
