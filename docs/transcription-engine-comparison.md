# Transcription engine comparison

Status: comparison executed on 2026-09-07. Research only. The plan below was
recorded before inference; the annotation-parser correction is disclosed below.
The authorized integration pilot is recorded at the end of this document.

Keep the current transcription engine and macOS 14.4 requirement until a
replacement preserves the meeting record with less waiting or setup. This
experiment compares the existing MLX Whisper engine with Apple's
SpeechTranscriber and Parakeet through FluidAudio. The original benchmark did
not change product source. The later pilot adds a worker entry point; the app
default, release configuration, and existing roadmap order remain unchanged.

**Recommendation:** keep Whisper as the shipping baseline. Keep Apple and
Parakeet in the open decision. Parakeet was the fastest transcription candidate,
but its short-call replay took longer to produce a validated note. Apple reduced
that wait slightly. Neither result establishes better meeting notes or warrants
raising the minimum OS.

The [video](https://www.youtube.com/watch?v=IMQw3aHjf2Q) raises a useful implementation question. It does not change Yawn's
job: preserve a private meeting record and produce notes whose claims lead back
to their sources. A push-to-talk tool can succeed when it inserts useful text;
Yawn must also preserve both recording legs, attribution limits, retention,
recovery, vocabulary corrections, and evidence links. This review assesses that
product and engine comparison, not visual similarity to the demo.

## Plan

1. Freeze the source files and reference annotations before seeing engine
   output. Use the retained, consented three-minute two-leg capture, the
   twelve-minute capture underlying the existing thirteen-event ledger, and
   the public AMI ES2004c meeting already named by Yawn's evaluation corpus. Add
   synthetic silence as a hallucination control. Record source hashes and
   data-use limits. Private source and derived text stay outside Git.
2. Prepare model assets separately. Run every engine against identical PCM
   audio, one engine at a time. Measure the first run in a fresh process and
   a repeat within that process. The baseline is the installed active Whisper
   Q4 model, revision `660c343bbf4e52ac257f0b7d952e5388e6f93bef`, verified
   against the installed catalog. Report model setup, decoding, and whole
   process time separately. A fresh process is not a cold operating-system
   cache. Do not call an Apple client-process memory figure total engine memory.
3. Compare transcript errors against independent manual reference text where
   available. Treat differences from an earlier Whisper transcript as
   disagreement, not an accuracy score. Check important terms in their local
   source context, timing bounds and alignment, missing tail content, and
   invented speech during silence. Lexical survival is a diagnostic, not proof
   that a commitment was understood or correctly summarized.
4. Inspect how each result can enter Yawn's existing source-bound note and
   vocabulary paths. Run the existing note evaluation only when the required
   audio, reference, model, and approval bindings are present. Reuse the
   current generator and validator for a bounded three-minute replay to measure
   time to validated note output, with the same note model for every engine.
   That is not time to a human-accepted useful note. Never invent a replacement
   approval or call an unscored generation a passed ship gate.
5. Record measurements, failures, source identities, and a recommendation here.
   Correct the roadmap's factual premises without promoting the open decision
   into a scheduled migration.

## What would justify a change

A candidate must preserve decisions, commitments, names, numbers, negation,
and source location at least as well as the current path on the same material.
It must produce no text for the silence control, retain the full meeting tail,
and expose usable timing. A latency improvement does not excuse a worse record.

The existing operator-locked note gate remains authoritative. Engine changes
produce new transcript and candidate identities; a time-mapped comparison to
the original locked events is a proxy and cannot reuse their candidate IDs as
a newly passed ship gate. Missing private ledger material likewise means that
gate is unmeasured, not passed. A public
mixed-room recording supplements the real two-leg sample; it does not prove
capture, microphone permissions, interruption recovery, or physical-device
acceptance. One machine cannot establish the effect on users of macOS 14.4–15.

## Architecture facts to carry into the decision

- Apple manages speech-model assets; a missing locale can still download on
  first use. Its API supplies `audioTimeRange`. The experiment must establish
  whether the returned ranges meet Yawn's needs. [Apple's implementation
  walkthrough](https://developer.apple.com/videos/play/wwdc2025/277/).
- Parakeet has a Swift/CoreML integration through
  [FluidAudio](https://github.com/FluidInference/FluidAudio). It remains a new
  dependency and requires measurement; absence of an MLX conversion alone is
  not grounds to exclude it.
- Replacing transcription does not remove Yawn's separate MLX-LM note runtime,
  note-model downloads, or evidence validation. The
  [note-runtime decision](note-runtime-decision.md) still governs that work.
- SpeechTranscriber can be evaluated on macOS 26 without first raising the
  product's minimum OS. Keeping a fallback would add a second supported engine;
  choosing that tradeoff requires evidence about the users it serves.

## Sources and limits

External sources above were fetched on 2026-09-07. AMI audio and manual
annotations are published under CC BY 4.0 through the
[AMI corpus download page](https://groups.inf.ed.ac.uk/ami/download/).
The private capture is governed by the [real-meeting
handoff](real-meeting-handoff.md). This protocol is written for Nino to decide
whether to invest in an engine migration; implementation details support that
decision rather than imply a release.

## Results

All measurements below come from the [content-free experiment
receipt](evidence/transcription-engine-comparison-2026-09-07.json). Runs used one
M3 Max Mac with 36 GiB RAM and macOS 26.6.2, build 25G83. These are observations
on that machine, not population estimates. Audio hashes matched before and
after every engine run and transcript replay.

### Native engines reduced transcription time

| Recording | Whisper Q4 | Apple SpeechTranscriber | Parakeet v3 Core ML |
|---|---:|---:|---:|
| Three-minute remote call, both legs summed | 5.78 s | 2.82 s | 2.70 s |
| Twelve-minute in-person meeting, both legs summed | 24.22 s | 11.31 s | 4.14 s |
| Full 38:54 AMI meeting, mixed headset audio | 46.37 s | 31.56 s | 11.74 s |
| AMI repeat in the same process | 53.17 s | 32.50 s | 12.12 s |

The first three rows measure the first decode in a fresh process, after model
preparation. Each includes reading the WAV. They exclude downloads, model setup,
capture finalization, note generation, and app display. Both legs ran
sequentially. The in-person sample carried speech on the microphone leg; it
does not supply a second long remote-call test. Fresh processes did not clear
the operating system's caches. Two repetitions do not establish a stable
latency distribution.

On the AMI case, separate setup took 2.70 s for Whisper, 0.11 s for Apple,
and 0.11 s for Parakeet with assets already present. Parakeet's initial asset
preparation took 258.76 s including download and load. Apple's missing-asset
download was not measured. Client-process memory is recorded for diagnosis;
Apple performs speech work in a separate service, so those numbers cannot
rank total engine memory.

**What it means:** both native routes merit consideration. Their decoding
advantage is measured; an equally large improvement in the user's wait is not.

### The note generator remained the larger wait

The three-minute remote call was replayed through current capture filters,
immutable transcript creation, the same installed Gemma note model, and Yawn's
current claim and evidence validator. Each engine received one note replay.

| Note replay, excluding transcription | Whisper transcript | Apple transcript | Parakeet transcript |
|---|---:|---:|---:|
| Note generation | 99.02 s | 95.30 s | 147.43 s |
| Whole research process, including verification and setup | 109.96 s | 103.89 s | 155.57 s |
| Validator result | Validated output | Validated output | Validated output |

The outputs contained seven, six, and five claims respectively. Those counts
describe different outputs; they are not quality scores. Human usefulness and
event recall were not scored. Generated text stayed private and was not saved
as an app note. Network access was denied before loading the note model.

Whisper supplied decoded segments. Apple supplied native final utterances.
Parakeet supplied words, so its research adapter grouped them at sentence
punctuation, a gap over one second, or a span over fifteen seconds. It retained
native word endpoints. This grouping changes candidate boundaries and model
work. The Parakeet replay therefore measures that concrete adapter, not an
intrinsic penalty of its speech model. A different grouping needs a new
comparison. No voice profile was supplied; current voicing and bleed filters
ran, but operator voice gating was not tested.

**What it means:** faster transcription alone did not solve time to a useful
note. The native adapter and note path must be evaluated together.

### Accuracy diagnostics support a pilot, not a replacement

AMI provides independent manual annotations. Its overlapping speakers do not
form a verified single-speaker transcript order. The exact edit distance below
compares each output with the manual words sorted by time; it is deliberately
not reported as a validated word error rate.

| AMI diagnostic | Whisper | Apple | Parakeet |
|---|---:|---:|---:|
| Edits divided by 7,084 manual lexical tokens; lower is better | 25.66% | 22.20% | 20.00% |
| Local terms retained in annotated decision evidence | 101/123 | 100/123 | 106/123 |
| Local terms retained in annotated entities | 114/132 | 102/132 | 122/132 |
| Distinct terms retained in the final tenth of the meeting | 240/293 | 245/293 | 231/293 |

Term checks require exact normalized tokens near their annotated audio spans,
with two seconds of tolerance. Coarser segments can receive more generous
credit than word ranges. Entities include multiple annotation types, not only
people's names. These checks do not establish preservation of meaning,
negation, ownership, or commitments. All engines reached the closing part of
the meeting, but none retained all checked tail terms. Parakeet's aggregate
result did not make it best on every diagnostic.

For the private twelve-minute sample, the existing thirteen-event draft
reference was mapped to its original audio times. After Yawn's filters, mean
best-alternative local term survival was 96.68% for Whisper, 98.74% for Apple,
and 98.64% for Parakeet. The reference derives from an earlier Whisper
transcript, not independent human ASR truth. The benchmark did not locate the
original lock. The subsequent pilot recovered matching bytes in a superseded
evaluation cycle, but that lock does not bind the current registration and its
reference did not validate under the current rules. The reference remains a
draft proxy. No original approval was transferred to a new transcript, and the
locked ship gate was not run or passed.

**What it means:** the replacements are credible candidates, but this sample
does not establish that either preserves the meeting record well enough to ship.

### Timing and vocabulary can fit the existing contract

Apple returned timed attributed spans and final utterance ranges. Parakeet
returned token timings that FluidAudio grouped into words. Every native output
token matched the timed segment text in the public meeting, and the native
runs had no missing, nonfinite, reversed, or out-of-bounds ranges. This proves
API availability and basic integrity, not perceptually exact playback seeking.

Whisper emitted text for synthetic silence and for the quiet system leg of
the in-person sample. Two raw endpoint ranges extended past the files. Yawn's
existing voicing filter removed those outputs: synthetic silence retained no
text, and the in-person replay retained no system-leg turns. Apple and Parakeet
returned no raw text on those inputs. This is evidence to keep the filters,
not a claim that Yawn displayed Whisper's raw hallucinations.

The existing vocabulary projection operates on immutable transcript spans and
preserves original citation offsets. Its focused tests passed, including
changed-length replacements and refusal of stale or invalid overlays. The
research replay also created new transcript hashes and validated note
citations against those revisions. It did not run a vocabulary replacement
through each engine's complete app workflow. See
[PromptOverlay](../notes/transcript.py),
[note validation](../worker/note_validator.py), and
[transcript creation](../worker/transcription.py).

**What it means:** timing availability is no longer an open API question.
Playback accuracy, app integration, and the combined vocabulary workflow still
need acceptance checks.

### The setup decision is smaller than the original roadmap implied

The installed Whisper model used here totals **463,665,005 bytes**, verified
against its catalog hashes. It is the Q4 revision, not a roughly 2 GB speech
download. The Parakeet cache totals **483,257,242 logical bytes**. These are
model-file sizes, not total installation size or bytes reclaimed by a migration.
FluidAudio is pinned to 0.15.6, revision
`4dbf4f9f9a5ff3a53ade848d7ba4e3df13db859b`; the receipt records observed hashes
for its model files because the download API did not expose a source revision.

The installed note model separately totals **8,063,332,687 bytes**. Swapping
speech engines leaves that model and the MLX-LM note runtime in place. Apple's
speech assets are system-managed and can download when missing; their storage
and updates are not under the same application-owned hash pin. Keeping current
transcript revisions immutable matters under either model lifecycle.

**What it means:** native speech may simplify one part of setup, but it does
not eliminate Yawn's main note-model download or all model management.

## What would change the recommendation

A replacement becomes a shipping choice when an approved comparison shows
that its notes preserve the required events and evidence at least as well as
the baseline, and the complete app delivers a worthwhile reduction in waiting
or setup. That comparison must include the adapter's utterance grouping,
vocabulary replacements, playback seeking, failures, and recovery.

Before raising the OS floor, establish the affected user population or record
explicitly that it is unknown. Apple can be explored behind an availability
check while that decision remains open; shipping two engines would bring its
own maintenance cost. Parakeet's Core ML route must be evaluated against the
supported OS range rather than excluded for lacking an MLX conversion.

The next decision is whether to invest in that bounded integration and an
approved note comparison. This research does not move it ahead of roadmap
Orders 0–4.

## Reproduction and provenance

Research code lives in [spike/speech-engine-comparison](../spike/speech-engine-comparison/).
Raw inputs, outputs, logs, and generated notes remain under the owner's private
`~/Library/Application Support/yawn-research/transcription-engine-comparison-2026-09-07`
directory. The shareable receipt contains metadata and hashes only.
The worktree is based on `c9dfcdb`; no product source or release configuration
was changed.

Use `run_comparison.py --help` for the sequential engine runner and
`replay_notes.py --help` for the bounded replay inputs. The native probe builds
with `swift build -c release` in its `native` directory. Whisper runs with the
installed Yawn Python runtime and the installed catalog/model paths. Results
must go outside app and repository storage. `summarize_results.py` derives the
content-free receipt from those saved artifacts; it needs `rapidfuzz==3.14.3`.
Note replay commands were bounded externally to 600 seconds each.

The pre-inference AMI extraction incorrectly read only the first endpoint of
NITE ranges such as `#id(first)..id(last)`. The corrected parser resolved all
944 ranged references in the selected dialogue-act and entity layers. The
original archive, audio, and word layer were unchanged. The original derived
reference is retained; its span scores are discarded. Final scores use
`ami-reference-v2.json`, whose hash and correction are in the receipt. This was
a mechanical parser correction after inference, not a new selection of favorable
reference passages. The scorer also avoids segment-dependent time-window edit
distance, which would disadvantage engines with coarser segment boundaries.

Validation: fifteen research parser/scorer/adapter tests passed, plus five
focused existing vocabulary/bridge tests. All three engines completed both
repetitions on every frozen input. All three short note replays produced
validator-accepted output. No physical-device, packaged-app, cancellation,
first-download failure, battery, older-OS, or human note-quality acceptance is
claimed. The source video remains the reason for the comparison, not its
benchmark evidence.

## Integration pilot dispatched — 2026-09-07

The operator authorized planning and dispatch after reviewing these results.
Build a local path that accepts native transcription without requiring an
unused Whisper model, then check that its notes remain reviewable. Keep the
shipping default, macOS floor, note model, retention rules, and existing gates.
No commit, push, installation, release, or new human approval is included.

Three implementation shapes were considered:

| Shape | Effect on the real cases | Decision |
|---|---|---|
| Keep injecting results into the current Whisper entry point | Fast to test, but native replay still requires an unrelated Whisper installation | Retain only as historical benchmark evidence |
| Add a validated segment entry point sharing current filters and transcript creation | Native results enter the same evidence path; current callers keep their behavior | Use for the pilot |
| Replace the worker or add an app engine selector now | Expands packaging, recovery, and user-facing choices before note quality is accepted | Defer |

```text
Current Whisper call ──────────────────────────┐
Native result + verified audio/result receipts │
  → validate text and timing                   │
  → preserve utterances / explicit word grouping
  → shared voicing, bleed, and optional voice gate
  → immutable transcript → existing note validator
```

The native boundary accepts Apple and Parakeet result schema
`speech-engine-result/1`. It rejects unsuccessful results, wrong engines,
missing text coverage, invalid timing, or a result not bound to the supplied
recording. It never repairs timing by guessing. Apple retains final utterances;
Parakeet keeps the existing explicit research grouping until alternatives are
compared. The original benchmark outputs and receipts remain unchanged.

| Assignment | Owner | Owned files | Acceptance |
|---|---|---|---|
| Native transcript integration | native_speech_probe | `worker/speech_results.py`, `worker/transcription.py`, `worker/tests/test_speech_results.py`, `spike/speech-engine-comparison/replay_notes.py` | Native replay needs no Whisper model; both legs retain current filters and source protections; legacy tests pass |
| Private human-review preparation | yawn_approach | `spike/speech-engine-comparison/prepare_note_review.py`, `test_note_review.py` in the same folder | Saved claims resolve to their own transcript; private review and response template stay pending; old approval is never copied |
| Independent contract and grouping checks | corpus_scoring | `worker/tests/test_speech_pilot_contract.py`, `spike/speech-engine-comparison/compare_segmentation.py`, `test_segmentation_comparison.py` in the same folder | Source links and changed-length vocabulary survive; wrong-source and malformed results refuse; grouping comparison reports workload without claiming quality |

Each assignment runs in its own `.worktrees/pilot/` checkout based on
`c9dfcdb`, with the uncommitted research inputs copied from this worktree.
Workers must not edit another assignment's files. Parent integration owns this
plan, the roadmap, cross-review, and the combined receipt.

Shared interface for the two implementation lanes:

- `worker.speech_results.validated_native_segments(result, *, engine, duration_seconds)`
  returns native-bound `[{start, end, text}, ...]` or raises a clear refusal.
- `worker.transcription.create_transcript_revision_from_segments(capture_dir,
  transcript_dir, *, mic_segments, system_segments, voicing_filter=None,
  bleed_filter=None, gate_filter=None)` returns the same `(digest, path)` as
  the existing entry point. It shares the existing filter/write path.
- Existing `create_transcript_revision` and all app callers remain compatible.

The review lane first verifies the existing receipt-to-transcript-to-generated
note hashes. It creates a private Markdown comparison plus an unanswered review
template, using neutral variant labels and a separate engine key. The reader
checks missing commitments, incorrect meaning, and source support. It does not
call the existing bulk-accept helper or mint a passed ledger. Locate any old
lock only within the known private evaluation root, validate its actual bytes
and bindings, and report its original scope separately from fresh approvals.

The grouping lane compares native grouping, the current sentence/gap grouping,
and one documented bounded grouping on the same saved words. It records token
preservation, timing bounds, candidate counts, and prompt size. These are
workload diagnostics, not latency or quality claims. Synthetic contract tests
cover both legs, empty speech, malformed results, evidence offsets, and a
changed-length vocabulary replacement; fixture success remains fixture evidence.

After dispatch, combine only owned changes into the research worktree. Run
existing transcription and vocabulary tests plus the new contract tests.
Replay retained audio into a fresh private research directory only after
checking that retention and source hashes still allow it. Model inference,
if a new run is necessary, is serialized and bounded by the parent; workers
must not run concurrent models. Run the new boundary against the saved native
outputs before spending time on another inference pass.

Completion of this pilot means a tested local integration and a concrete,
unapproved review packet. Shipping remains conditional on human note review,
the applicable locked gate, packaged-app behavior, and the OS-floor decision.


## Integration pilot results — 2026-09-07

The local native path works without a Whisper installation. Apple and Parakeet
both replayed the retained short call and the twelve-minute call through Yawn's
existing filters and immutable transcript writer. These four replays used saved
speech results; they did not run transcription or note inference again.

All transcript fields matched the earlier replays except `source`, which the
existing writer labels with the creation minute. That label changes the
content-addressed transcript ID. Text, timing, attribution, and filter results
matched; the earlier generated notes remain bound to their earlier transcript
IDs. The pilot does not copy those notes onto the new IDs.

The validator also accepted both repetitions of all six saved corpus cases for
each native engine. It rejects wrong engines, missing text, invalid bounds,
unordered timing, or malformed repeats. Synthetic contract checks cover silent
input, microphone and system filtering, bleed-related label withdrawal, and
changed-length vocabulary corrections with evidence offsets preserved.

**What it means:** native transcription can enter the existing evidence path.
This is local worker evidence. Packaging, actual audio seeking, cancellation,
recovery, and human note quality remain unaccepted.

### The private review is ready and unanswered

The review packet presents the original short-call notes as neutral variants.
It includes each claim's cited text, source times, full transcript, and local
audio links. Its separate engine key reduces name-based anchoring; this is not
a randomized blind trial. All 18 claims resolve to their own saved transcript.
The response template remains `pending`, with no answers or approval. The packet
and all meeting content stay outside Git with private file permissions.

A bounded search also recovered the exact earlier lock digest from a
superseded evaluation cycle. Its manifest and lock digest rederive, but its
registration differs from the current registration. Reference validation did
not pass, so downstream ledger validation remains unestablished. Finding the
old file corrects the benchmark's earlier missing-file observation; it does not
approve any of the new transcripts or notes.

**What it means:** the next product decision needs a person to compare the
notes against the recording. An old lock and a passing source validator cannot
supply that judgment.

### Larger groups reduce request volume; note quality is unmeasured

The grouping diagnostic used the current classifier's offer stride, batch
construction, and vocabulary presentation without calling a model. It compared
raw recording legs before Yawn merges or filters them. The following counts are
workload diagnostics, not measured note-generation speed.

| Input | Engine | Current groups → bounded groups | Current classifier requests → bounded requests | Total prompt bytes, current → bounded |
|---|---|---|---|---|
| Public AMI meeting | Apple | 498 → 170 | 253 → 85 | 517,227 → 225,968 |
| Public AMI meeting | Parakeet | 534 → 171 | 269 → 86 | 543,720 → 226,259 |
| Short private call, sum of separate legs | Apple | 45 → 18 | 23 → 10 | 43,986 → 21,189 |
| Short private call, sum of separate legs | Parakeet | 49 → 19 | 25 → 10 | 47,431 → 21,339 |

The bounded rule joins at most 40 lexical tokens or 15 seconds and never splits
a source segment. The saved words remain in order and native time endpoints are
preserved. The current pilot keeps Apple final utterances and Parakeet's
sentence/gap grouping. No new grouping was adopted.

The first diagnostic omitted the classifier's offer stride and mislabeled raw
Parakeet words as the current pilot. Those reports remain preserved but are
superseded by private v2 and public v3. Exact prompt construction for the public
raw word-per-turn representation was stopped after several minutes of CPU work.
That unimplemented path has shape data and an explicit unmeasured workload;
all implemented and bounded options have exact request measurements.

**What it means:** grouping can reduce requests by changing what the classifier
sees. It can also change which commitments survive. A smaller request count is
not enough to choose it; compare generated notes against the recording first.

### Verification and limits

The combined run passed 37 worker tests, 30 research-helper tests, and five
existing vocabulary/bridge tests. After the final workload-skip change, all
seven grouping tests passed again, replacing that run's six grouping tests. The worker tests used the installed Python
runtime; the bridge tests used system Python because the embedded interpreter
cannot start that test harness's scrubbed child environment. No packaged-app
acceptance is inferred from these tests. `git diff --check` passed.

The [pilot receipt](evidence/transcription-engine-pilot-2026-09-07.json) records
source hashes, replay identities, review state, and workload diagnostics without
meeting content. The original benchmark receipt is unchanged. Its original
research source bytes are preserved in the private `benchmark-source` archive;
the tracked baseline remains `c9dfcdb`.

The work is uncommitted in the isolated research worktree. Whisper remains the
shipping default and macOS 14.4 remains the minimum. Human review is the next
gate before choosing further engine or grouping changes.


## Matched grouping comparison dispatched — 2026-09-07

Run the same short call through current and bounded grouping for each native
engine. This measures whether fewer classifier requests reduce the wait for a
validated note, and prepares the resulting notes for human comparison. No
engine or grouping is selected by this experiment.

Freeze four runs before inference, in this order: Apple current, Parakeet
bounded, Parakeet current, Apple bounded. Each run uses the same saved native
results, two recording legs, installed note-model revision, filters, vocabulary
configuration, and validator. Each starts a fresh process with a 600-second
limit. One agent owns all model inference and runs it serially. Record every
attempt, including refusal or timeout; do not rerun to obtain a better result.
A single run per condition is descriptive evidence, not a causal speed estimate.

The bounded option uses the previously declared 40-token/15-second rule on the
validated native word segments before the existing recording filters. It must
preserve all source words and endpoints. Current remains the default. The flag
exists only in the research replay helper, with no worker or app default change.

| Assignment | Output ownership | Acceptance |
|---|---|---|
| Matched replay and serialized inference | `spike/speech-engine-comparison/replay_notes.py`, `test_replay_notes.py`; private run outputs | Explicit `--grouping current\|bounded`; reject bounded Whisper; validate native input before grouping; receipts name grouping and all source identities; no simultaneous models |
| Private comparison packet | `spike/speech-engine-comparison/prepare_note_review.py`, `test_note_review.py`; private `review/` output | Optional versioned variants manifest; four neutral labels with separate key; original packet remains intact; failed conditions are explicit; all human answers remain pending |
| Independent result checks | `spike/speech-engine-comparison/summarize_grouping_pilot.py`, `test_grouping_pilot.py`; private content-free summary | Check attempts, transcript/note hashes, engine/grouping/model/source bindings and claim locators; compare actual calls and elapsed time; no quality score inferred from counts |

The private variants manifest uses schema `speech-note-review-variants/1` with
`variants: [{directory, engine, grouping, label}]`. Each directory is a single
relative name under the new private run root. Each replay receipt adds
`grouping: current|bounded`; its outer attempt records engine, grouping,
return code, elapsed seconds, source hashes, and receipt hash when available.
The manifest is an identity contract, not a human approval.

All three agents get isolated worktrees with the completed pilot source copied
in. They must not revert each other's work. The replay agent alone can run local
models. The other agents can implement and test against synthetic fixtures while
it works. Private audio and text stay on the Mac and out of chat and Git. None
of this authorizes a commit, push, release, app installation, new approval, or
migration of the existing evaluation ledger.

After the agents finish, integrate only their owned code and validate the
combined source. Recheck the private packet and publish only a content-free
receipt and measured findings here. The human review stays unanswered until
the operator supplies it; an agent cannot substitute its judgment for that gate.


## Matched grouping results — 2026-09-07

Larger groups reduced the observed wait for a validated note on this short
call. Both engines used fewer model requests. The resulting notes contain
different claims, so timing alone does not justify adopting the grouping.

| Engine | Current total replay | Bounded total replay | Current note generation | Bounded note generation | Model requests, current → bounded |
|---|---|---|---|---|---|
| Apple | 99.78 s | 54.21 s | 90.29 s | 45.74 s | 22 → 9 |
| Parakeet | 111.82 s | 58.97 s | 103.26 s | 50.38 s | 23 → 9 |

Total replay includes filters, model verification/loading, note generation,
and process overhead. It reuses saved speech results, so it excludes fresh
transcription. The four conditions ran once each, serially, in the frozen
order. This is a descriptive result for one call. It does not establish a
causal speed improvement across meetings, and the order can affect caching.

The parent checked the saved attempt and preflight hashes against the frozen
plan and variants manifest. The actual capture bytes, corpus, catalog, and
producer source matched their recorded identities. All four runs returned
success with unchanged sources. The independent checker also validates each
note against its own content-addressed transcript and canonical source spans.

The private review presents all four notes as neutral variants, with source
excerpts, times, full transcripts, and recording links. It remains unanswered.
Human review must check missing commitments, owners, deadlines, negation, and
whether each claim means what was said. A valid citation does not answer those
questions.

**What it means:** bounded grouping is worth reviewing as a way to reduce the
wait. Keep current grouping and Whisper as defaults until that review and the
app-level gates are satisfied. No engine, OS-floor, or grouping change was
adopted by this experiment.

The [matched grouping receipt](evidence/transcription-grouping-pilot-2026-09-07.json)
contains the verified measurements and source identities without meeting text.
The original private attempts and earlier summaries remain preserved. The
first summary incorrectly named an older checker-local replay file as the
producer; its replacement takes the producer hash from the bound run receipts
and checks all nine relevant source files plus the frozen plan and manifest.
The prior integration pilot's source is preserved privately in `pilot-source`
before the research helpers change.


Verification of the integrated helpers passed 30 focused tests. Replay CLI
checks used the installed Python runtime because the scoring-only environment
lacks NumPy. The parent independently rehashed all 15 labeled source files per
condition, including the original microphone/system audio and capture receipt;
this resolves the checker's inability to map those labels to paths on its own.
The private `review-v2` packet has four available conditions and verified local
links. Its response template remains pending and unanswered. `git diff --check`
passed. All changes remain uncommitted in the research worktree.


## Local review suggestions planned — 2026-09-07

Use the installed note model to propose passages for human inspection across
the four matched notes. The model reads each note against its own transcript,
with engine and grouping names removed. It does not hear the audio. Reusing
the same model family can repeat the generator's errors, so this is a way to
focus review, not an independent quality verdict.

Freeze the prompt, input hashes, model revision, and a 2,048-token response cap
before inference. Run four requests serially in one network-denied process,
with a 600-second outer limit and no retry for favorable output. Keep all
text and suggestions private. Each suggestion must reference an existing
claim or a possible omission, identify an existing transcript turn, and quote
an exact substring. Reject invalid references; matching a quote validates only
its location, not the model's concern. Never turn an empty result into a pass.

The deliverable is a private list of suggested passages plus a content-free
receipt. The original four-note packet and all human responses remain intact
and pending. This does not select an engine/grouping, change product code,
transfer an old approval, or authorize shipping.


## Local review suggestions completed — 2026-09-07

The private passages-to-check document is ready. The local model reviewed
each of the four notes against its own transcript in one serialized pass.
No audio was supplied to the reviewer. The original human review packet and
its answers remain unchanged.

The model returned six suggestions that passed quotation and reference checks;
three other proposals failed validation and were excluded from the reader
document. These counts describe the review helper's output, not note quality.
The concerns themselves remain unverified. In particular, fewer suggestions
for a variant do not establish that its note is better.

The parent rechecked each retained quotation against its transcript offset and
each referenced claim against the original note. All source hashes and private
file permissions matched. Five focused validator tests and `git diff --check`
passed. The [local review receipt](evidence/transcription-local-review-2026-09-07.json)
records the prompt/model/input identities and diagnostic results without text.

**What it means:** the operator now has specific passages to inspect alongside
the original four notes and recordings. This pass cannot accept an engine or
grouping. It shares a model family with the note generator and may repeat its
errors. Human review remains pending.


## Progressive setup implementation — 2026-09-07

The operator authorized Apple speech first, a smaller downloaded model when
needed, and other models later in Settings. That direction is implemented in
source on `feature/progressive-setup`. It keeps the macOS 14.4 floor and the
separate, optional note-model flow. At this stage it was not installed or released;
the installed checks are recorded below.

The new Apple-only helper uses the native API behind a macOS 26 availability
check. Startup checks capability without downloading language files. Preparing
those files requires an explicit action. Existing downloaded-model choices
survive startup, and model installation selects Whisper explicitly. New queue
requests record the selected producer; mismatched queued work is refused.
Native transcript provenance records the helper digest, locale, operating-system
version, and capture/result digests. Apple manages its assets and exposes no
immutable model revision through this implementation.

The real adapter created a transcript from two short public AMI clips with no
Whisper model. The capture files remained byte-identical. This is execution
evidence, not an accuracy result. A subprocess test also started the native
worker without a Whisper receipt and verified that it exits when its parent
connection closes. UI interaction checks exercised Apple preparation, the Q4
fallback, and Settings switching in both directions.

The [verification record](evidence/progressive-setup-2026-09-07.json) separates
source tests, real helper execution, and synthetic browser checks. Packaged-app
startup and switching, older-macOS execution, real asset-install recovery, and
the full transcript-to-note workflow still need acceptance. The existing human
quality review remains unanswered.

A follow-up review found that engine changes could overlap note generation or
transcript retry. Speech and note-model changes now reserve the same operation slot until
their background task exits. A setup recording also refuses to start during a
speech-model change. Failure paths now report a retryable error or an unsupported
runtime instead of leaving startup waiting. Eight facade tests, the focused
setup-recording test, the Apple capability test, and 154 UI tests passed.

An independent reviewer opened the browser-harness setup and Settings captures.
The review found unclear download buttons and no introduction to optional notes.
The buttons now name the download, and setup explains that notes can be added
later in Settings. The primary author and independent reviewer opened the updated frame; the
reviewer confirmed the wording findings were resolved. These captures establish browser presentation only; packaged-app behavior is still open.


The local app bundle now exists from source commit `6659ead`, version `0.6.3`.
The runtime build ran 248 tests: 171 passed and 77 were skipped by existing test
conditions. Strict signed-bundle verification passed for 200 arm64-compatible
Mach-O files. The bundled runtime also passed its source-freshness check.

Using only the packaged Python modules and signed Apple helper, the worker
started without a Whisper download, exited on parent disconnect, and produced
a transcript with provenance from the same short public AMI clips. Temporary
capture bytes remained unchanged. The four hosted speech objects returned the
cataloged lengths using Yawn's installer user agent; this was a reachability
check, not a full remote-object digest check.

The verification record identifies the built source commit and signed executable
and manifest digests. This bundle is for local testing; actual first-run setup,
Settings switching, older-macOS behavior, and real-meeting quality remain separate
acceptance checks.


## Installed test build — 2026-09-08

Apple accepted the app and installer. Strict signatures, attached notarization
tickets, Gatekeeper, installer layout, and runtime verification passed. Full
downloads of all four hosted speech-model files matched the cataloged byte counts
and SHA-256 hashes.

The app was installed from the verified disk image. Its version remains `0.6.3`;
the source commit and binary digests distinguish this test build. The previous
app is retained for rollback. No public artifact or repository branch was pushed.

The installed worker and Apple helper started without Whisper, transcribed the
short public AMI clips, preserved capture bytes, and wrote provenance. Parent
disconnect stopped the worker. This exercised installed components; it did not
launch the app interface or assess meeting quality.

The operator then confirmed being away with the Mac unlocked. The installed
Settings test found that reopening a saved transcript blocked speech-model
changes and left Record disabled. Switching to Apple also failed when startup
tried to restore the already-open transcript. A regression test reproduced that
exact failure before the fix.

Source commit `3a3d935` corrects transcript restoration, model-change admission,
and Record admission. Settings now refreshes speech and note availability when
an engine change finishes. The corrected app and installer were signed,
notarized, verified, and installed. The installer SHA-256 is
`e17687eb752326334790e35647334dca317eb0141a8f899874b3238a3725091a`.
Desktop unit tests, reducer tests, permission contracts, and UI tests passed.
The corrected signed bundle also transcribed public sample audio with provenance
and clean worker shutdown.

The corrected installed app now passes the existing-account Settings checks.
Apple speech remained selected after a complete quit and reopen. The smaller
speech model was then restored and remained selected after another complete
quit and reopen. Settings controls became available after each engine change.
The speech-model and note-model selection files match their original hashes.
The note-model files still predate this test; no note download was started.

The capture guard interrupted the first pass when Chrome became foreground and
the Mac showed recent input. The operator subsequently said go, and the remaining
checks were completed. **The original smaller speech model is selected.**

The explicit system-audio check in installed Settings succeeded. Both required
audio sources showed Authorized. This helper check creates and destroys an audio
tap without reading buffers. Record then opened the consent sheet with all three
attestations unchecked and Start recording disabled. Cancel returned to the
saved meeting. No recording or participant attestation was made, and no active
capture helper remained after the check.

Private screen captures stay outside Git. The content-free installed-test record
is in `docs/evidence/progressive-setup-2026-09-07.json`; the cold screen review is
in `docs/evidence/screen-reviews/progressive-installed-2026-09-08-cold.md`.
Fresh-account setup, older macOS, Apple asset-download recovery, and human
meeting-quality acceptance remain open. This pilot was not pushed or published
at the time of this receipt. Progressive setup later shipped in 0.6.4 at
`58e13a2`; see the [release receipt](distribution-runbook.md#064-release-receipt).
That release does not close these outstanding acceptance checks.

## Retained Apple transcript-to-note replay — 2026-09-08

The existing 12-minute two-leg meeting completed note generation using saved
Apple speech results, current Yawn filters and grouping, and the installed
generation runtime. The existing downloaded note model matched the installed
catalog. Networking was denied before model loading, and all output went to a
fresh private research directory. Source audio and session metadata were unchanged.

An independent artifact check verified the generated-note digest, its exact
transcript binding, and all 20 source locators across eight claims. The referenced
timestamps fall within the recording. These are structural checks; they do not
establish that the claims preserve meaning or capture every commitment.

Note generation took 537.521 seconds in this one run. That is an observed timing,
not a comparison or a general performance claim. The first invocation stopped
before inference because it pointed to the base Python packages; the completed
run used the installed generation packages. No product code changed.

The content-free receipt is in
`docs/evidence/progressive-setup-2026-09-07.json`. Private outputs remain outside
Git. This completes the longer research replay only. Installed-app retry,
vocabulary, playback, and human speech/note quality acceptance remain open.
The existing four-variant human review packet is still unanswered; all six
unique audio and transcript links were checked and exist.

## Nemotron 3 local diarization trial — 2026-09-28

This was a completed, local research run of NVIDIA NeMo-Speech.cpp built from
main source commit `97a15afa5caa9bce5baaa86c1184103877af4101`, using the
`metal-diar` preset on this Apple Silicon Mac. The model GGUF SHA-256 was
`08456d9e22cd9a323c0364d98375f3746d6e68507ebb705cd46438c534c7a3a1`.
The scorer independently rehashed the frozen corpus manifest, all four source
audio inputs, and all four saved JSON outputs against the content-free run
receipt. Every hash, output byte count, and zero exit status matched.

| Case | Segments | Estimated speakers | Predicted overlap | Approx. wall time | Audio seconds / wall second | Behavior |
|---|---:|---:|---:|---:|---:|---|
| Synthetic silence (10.0 s) | 0 | 0 | 0 intervals, 0.000 s | 0.65 s | 15.38 | No segments |
| Short private microphone leg (179.8 s) | 13 | 1 | 0 intervals, 0.000 s | 1.09 s | 164.95 | Speech segments present |
| Short private system leg (180.0 s) | 23 | 1 | 0 intervals, 0.000 s | 1.09 s | 165.14 | Speech segments present |
| Public AMI meeting (2334.368 s) | 711 | 4 | 295 intervals, 220.620 s | 9.20 s | 253.74 | Speech segments present |

Wall times are receipt-recorded approximations for this one machine and run.
They are neither real-time performance nor a product latency measurement.
The silence result is useful control evidence: this saved run produced no
diarization segments. It does not establish all silence or noise behavior.

### Public AMI attribution proxy

The AMI manual-word and audio time bases were admissible for this proxy: all
manual word timings were within the frozen audio duration, and the first and
last diarizer extents were each within 15 seconds of the reference extent.
That rejects an obvious clock shift; it does not prove sample-clock alignment.
The global one-to-one mapping, selected once across the full meeting, was
`P1 -> B`, `P2 -> D`, `P3 -> A`, and `P4 -> C`, where `P1` through `P4` are
anonymous diarizer channels and `A` through `D` are AMI's anonymous reference
channels.

The strict midpoint proxy scored 5,691 correct channel assignments out of
6,151 eligible manual word annotations: **92.52%**. The denominator excludes
1,699 manual word annotations whose midpoint had simultaneous reference
speakers. Of the 460 eligible mismatches, 206 had more than one predicted
speaker at the midpoint and were counted incorrect rather than credited; 229
had no predicted speaker at the midpoint. The remaining 25 were single-channel
mapping mismatches. This is a word-midpoint, global-mapping proxy only. It is
not official diarization error rate, word-attribution accuracy, or an
acceptance score.

The reusable content-free scorer at
`spike/speech-engine-comparison/score_diarization.py` refuses the public score
when the reference is out of audio bounds or its broad time extent is shifted.
Its synthetic self-check fails for an obviously wrong global mapping, for a
predicted overlap treated as a single speaker, and for a shifted time base.

### Paired speaker-label probe on saved Apple transcripts

The 2026-09-28 paired check held Apple's saved timed transcript fixed and
changed only its speaker-label interpretation. No speech or note model was
rerun. Each Apple output matched the SHA-256 in its saved receipt; the
frozen AMI and short private audio still matched the corpus manifest.
Nemotron's saved output came from the same audio. This is an offline label
probe, not an installed-app A/B or a newly captured meeting.

On public AMI mixed audio, 5,218 of Apple's 5,963 timed units had exactly one
manual reference speaker at their midpoint. One system-leg `Them` label
cannot distinguish the four people; even optimistically mapping that one label
to the most frequent reference person would cover only 1,760/5,218 (33.73%).
Using one global anonymous mapping for Nemotron's four clusters, 4,921/5,218
(94.31%) had exactly one predicted cluster matching that reference person.
Both the single-label choice and the cluster mapping were selected against
this meeting's manual reference, so these are optimistic retrospective scores.
The other 297 eligible units had no predicted cluster (151), competing
predicted clusters (135), or one wrong cluster (11). A further 620 Apple units
were excluded because manual speakers overlapped at the midpoint; 125 had no
manual speaker at that point. This compares label resolution at timestamps,
not word recognition, official diarization error rate, or note quality. The
single-label number is a deliberately optimistic interpretation of `Them`,
which is a valid group label in Yawn rather than a claim to identify a person.
AMI's mixed headset is not Yawn's two-leg capture.

On the consented three-minute Yawn capture, Nemotron found one cluster on the
microphone leg and one on the system leg. It therefore added no distinction
to Yawn's `Me`/`Them` labels. Of Apple's 119 timed microphone units, 108 had a
Nemotron cluster at the midpoint; 281 of 298 system units did. Replacing
source labels with cluster coverage alone would leave 11 microphone and 17
system units unlabeled. Source-leg labels would need to remain the fallback.
These coverage counts have no independent speaker annotation and are not an
accuracy score. The frozen corpus still lacks a system-leg recording with
multiple independently annotated remote speakers.

### Retained in-person Yawn recordings — 2026-09-28

The frozen research corpus was not an inventory of the installed app. A later
check of Yawn's current meeting storage found two complete microphone
recordings outside that corpus. Nino reports that each contains two consented
people speaking in person. Yawn's current transcripts label every turn in
both recordings `Me`, because both voices entered through the microphone.
The original audio and current transcripts matched the hashes in each saved
meeting record. The pinned Nemotron model read the original microphone files
without copying them. Later listening checks made short local audio extracts
under the ignored research directory; the original app files were not changed.

| In-person recording | Current Yawn turns | Nemotron segments / clusters | Turn midpoints with one cluster | Turn midpoints with overlap / no cluster |
|---|---:|---:|---:|---:|
| September 10, 478 s | 90, all `Me` | 112 / 2 | 46 in cluster 1; 33 in cluster 2 | 3 / 8 |
| September 16, 374 s | 75, all `Me` | 93 / 2 | 38 in cluster 1; 28 in cluster 2 | 4 / 5 |

Nino listened to separate original-audio samples from each recording. He
identified his voice at 219–226 and 372–379 seconds in the September 10
recording, both predicted cluster 2, and at 117–123 and 219–226 seconds in
the September 16 recording, both predicted cluster 1. The second pair of
samples was selected at different points before he listened. This supports
cluster consistency at those sampled points; the clips were chosen from
single-cluster regions, so they cannot measure errors at transitions or
overlap. If each cluster remains consistent for its whole recording, 33 of
the September 10 turn midpoints and 38 of the September 16 turn midpoints
fall in the operator's candidate cluster; 46 and 28 fall in the other
anonymous cluster. Those are candidate assignments, not human-verified
speaker labels for every turn. The other voice remains unnamed.

Six previously selected transcript turns near predicted boundaries or overlap
were then played without model labels. Nino's listening reports and the
model's broad sequence compare as follows. These turns were deliberately
selected for difficulty, so the table is a case review, not a random sample
or an accuracy rate.

| Clip | Nino's listening report | Nemotron's broad sequence | Comparison |
|---|---|---|---|
| A, September 10, 27.82–37.50 s | Other person, with a few of Nino's words overlapping | Mostly other, with two short overlap intervals | Supports the mixed-speaker reading |
| B, September 10, 325.42–332.02 s | Both: Nino, then other | Mainly Nino, then other, with overlap at the change; a 0.26 s other-cluster fragment appears at the start | Main order agrees; initial fragment unresolved |
| C, September 10, 447.36–455.40 s | Other, then Nino | Other, then Nino | Order agrees |
| D, September 16, 244.74–251.88 s | Mostly Nino, then other near the end | Nino, then other near the end | Order agrees |
| E, September 16, 307.80–315.74 s | Nino only | Mostly Nino, but also 1.05 s of other-only and 0.75 s of predicted overlap | Other-speaker detection conflicts with listening report |
| F, September 16, 350.74–353.76 s | Mostly Nino; presence of the other person not specified | Nino throughout, with predicted overlap for 1.94 s | Overlap remains unverified |

Yawn's current `Me` label misses the other person's speech reported in A–D.
Nemotron represents both voices and their main order in those four selected
turns, but E shows why its output cannot be applied blindly. F does not
resolve whether its predicted overlap is real. The listening reports describe
whole turns and rough order, not precise word or boundary timestamps; no
diarization error rate can be calculated from them. The local review clips
and content-free labels stay in the ignored research directory.

A second, ten-turn listening packet broadened the check on September 28.
Before hearing Nino's labels, the selection used a fixed seed and picked,
from each recording, one early and one late turn where Nemotron strongly
favored the operator, one early and one late turn where it strongly favored
the other cluster, and one turn with no cluster at its midpoint. The packet
excluded clips already heard. Nino reported five operator-only turns, four
other-person-only turns, and one turn with the other person plus a brief
operator interjection. The scoring rule was also saved before he listened:
a speaker counts for a turn when the union of that speaker's predicted active
time reaches the greater of 0.5 seconds or 10% of the turn. Predicted
overlap counts for both speakers; Yawn's existing label predicts only `Me`.

On these **ten selected turns**, Nemotron's speaker set matched Nino's report
in **9/10** and Yawn's `Me` label matched in **5/10**. The one Nemotron miss
was the turn with the brief interjection: it predicted 0.59 seconds of
operator speech, below the pre-set 0.65-second presence threshold, while
correctly detecting the other person. This is a check of the selected turns,
not a whole-meeting accuracy estimate: the packet was stratified by
Nemotron's own predictions rather than drawn uniformly from all turns.
Neither packet supplies word-level speaker labels or precise boundaries.

Both predicted clusters recur through the recordings. The model also marked
20.6 and 16.0 seconds of overlapping clusters, respectively. This paired
comparison shows where a two-cluster interpretation would change Yawn's
single `Me` label. It does not establish that the clusters track the two real
people correctly throughout: the selected listening reports do not provide
a complete, time-aligned speaker reference, and a turn can cross a speaker
boundary even when its midpoint has one cluster. Timing is approximate: the
September 10 transcript's
last end is 478.580 seconds, 0.582 seconds past its 477.998-second microphone
WAV, and the capture record has no transcript timing reconciliation. The 11
and 9 unresolved turn midpoints also rule out simply replacing every `Me`
label with a cluster ID. A broader turn-level human reference, including
precise transition and overlap boundaries, is the next accuracy check. The
content-free input/output receipt stays under the ignored
`.artifacts/nemotron/yawn-in-person-20260928/` directory.

The installed voice profile was also checked on September 28. Its
`voiceprint.json` is zero bytes, which Yawn treats as no profile installed,
and both saved transcripts record `voiceprint: null`. The sample identity
reports above came from Nino listening; the app has no saved operator-voice
check to make that identification automatically.

These recordings test two people sharing Yawn's microphone, outside its
current one-person-near-the-microphone capture contract. They do not fill the
separate system-leg evidence gap for multiple remote participants.

### Integration boundary and next research step

Yawn currently assigns microphone segments to `Me` and system segments to
`Them`. Its durable correction sidecar may only project replacements for
`Me`, `Them`, or unattributed turns, and it is bound to the immutable source
transcript digest. It cannot safely turn an arbitrary diarizer cluster into a
person or make multiple system-leg clusters into new product labels.

The narrow next step is one off-sandbox, consented system-leg research run with
at least two independently annotated anonymous remote speakers. Score it with
the same frozen-input and explicit-overlap rules, then decide whether a separate
source-group contract is warranted. Do not map clusters to names or replace the
current channel attribution yet.

The frozen local corpus has no such Yawn system-leg reference. The short private
system leg is present, but its manifest has no independent speaker annotation;
the older ledger system leg is absent at its frozen path. AMI has independent
annotations, but it is public mixed-headset audio rather than a Yawn two-leg
capture. The model's estimated speaker count does not fill that reference gap.

The published NeMo-Speech.cpp v0.1.0 binary was not usable for this model on
either Metal or CPU: it stopped at `sortformer: pre_ln transformer variant is
not supported`. Main source contains the needed v3 exception. Separately, the
sandbox could not initialize Metal (`failed to create command queue` / `failed
to allocate context`), so no inference was run there. These are runtime and
dispatch constraints, not model-quality findings.

This trial changes no product source, contract, release state, or ASR decision.
The raw private outputs, audio, transcripts, names, and private input locations
remain ignored local artifacts and are not reproduced in Git.

## Desert Ant Voz on the public AMI meeting — 2026-10-09

A [video demo](https://www.youtube.com/watch?v=-89RZXTvY1k) of Desert Ant Labs'
on-device models prompted one bounded check: run its speech model, Voz, through
this comparison's AMI diagnostics. The video is the reason for the check, not
its evidence. Voz's [model card](https://huggingface.co/desert-ant-labs/voz)
states that Voz is NVIDIA's Parakeet TDT 0.6B v3, converted and compressed, with
the weights otherwise unchanged. It is the same model as the Parakeet column
above, delivered through a different runtime. That runtime cuts audio into
15-second windows at pauses and joins neighboring windows where their words
agree. Any difference below comes from conversion, windowing, and joining, not
from a different model.

The run used `desertant` 0.2.1 (desert-ant-core 3.5.0) from the vendor's
Homebrew tap, on an Apple M3 Max running macOS 27.0. The input was the same
public `ES2004c.Mix-Headset.wav` (2334.368 s). ES2004 is in AMI's official
[unseen evaluation partition](https://groups.inf.ed.ac.uk/ami/corpus/datasets.shtml).
The reference was rebuilt from `ami_public_manual_1.6.2.zip` with the unchanged
`prepare_ami.py` and scored with the unchanged `scoring.py`. The rebuilt
reference reproduces every denominator below. The 2026-09-07 reference hash is
not in Git, so the match is structural rather than byte-for-byte. Voz's
sentence segments were the primary scoring input.

| AMI diagnostic | Whisper | Apple | Parakeet (FluidAudio, 09-07) | Voz (10-09) |
|---|---:|---:|---:|---:|
| Edits divided by 7,084 manual lexical tokens; lower is better | 25.66% | 22.20% | 20.00% | 17.12% |
| Local terms retained in annotated decision evidence | 101/123 | 100/123 | 106/123 | 112/123 |
| Local terms retained in annotated entities | 114/132 | 102/132 | 122/132 | 123/132 |
| Distinct terms retained in the final tenth of the meeting | 240/293 | 245/293 | 231/293 | 251/293 |

**Correction, same day:** the Parakeet column was scored on FluidAudio's word
segments, so the matched Voz term figures are the word-segment ones, 110/123 and
122/132, not the sentence-segment figures in the table. The same-machine replay
below uses matched segments.

Scored on Voz's finer caption or word segments, decision and entity retention
fell to 110/123 and 122/132. Edits and tail retention did not change. Voz's
three runs produced identical text. The scorer separated a perfect transcript
(0.00% edits, every term retained) from Voz output with every third sentence
removed (44.35% edits, 76/123 decision terms).

Other observations:

- On 10 seconds of synthetic silence and 10 seconds of low-level noise, Voz
  printed `no speech found`, exited with status 1, and wrote no file. It
  invented no text. An adapter would have to treat that exit as an empty leg,
  not a failed run, because a quiet system leg is normal.
- With network access denied, a run with the model already cached produced
  identical text. During an ordinary online run, 54 socket samples found no
  open network connection. Sampling can miss a brief connection, so this is
  evidence, not proof.
- For the 386 tokens that occur exactly once in both the manual words and
  Voz's word timestamps, the median start offset was 0.07 s and the 90th
  percentile 0.19 s; 385 fell within 0.5 s. This comes from the Parakeet
  model, not from Desert Ant's packaging.
- The model cache occupied 475 MB on disk.

Not measured:

- Speed. Unrelated work held the machine at a load average of 11 to 19, and
  Voz's reported processing time rose from 6.1 s to 9.8 s to 13.3 s across
  identical runs. These figures are not comparable with the timing table above.
- A same-machine Whisper baseline. The installed runtime's `import numba`
  exited with SIGKILL when launched from this session's command environment.
  The cause was not established.
- A same-day FluidAudio rerun, the private two-leg captures, note generation,
  and any approval. Only the accuracy columns are compared with the earlier run.

Voz ships under the Desert Ant Labs Source-Available License 1.0. It is free
below 100,000 monthly active devices per platform for each model, requires
in-app attribution, and prohibits training a competing model. FluidAudio is
Apache-2.0, and the Parakeet weights are CC BY 4.0.

**What it means:** Voz does not add a new engine to the open decision. It shows
the same Parakeet weights scoring better on this meeting under different
windowing and joining, most visibly in the final tenth (251 versus 231 terms).
That points at the FluidAudio path's chunking as the thing to test. Adopting
Desert Ant would add a device cap and an attribution requirement for weights
already available under CC BY 4.0.

The bounded next step is one same-day, same-machine replay of the FluidAudio
Parakeet path and Voz on this meeting. If the gap holds, try pause-aligned
windows with agreement-based joining in the FluidAudio adapter and rescore.
This record changes no product source, contract, release state, or ASR
decision. The content-free receipt is
[`docs/evidence/voz-ami-2026-10-09.json`](evidence/voz-ami-2026-10-09.json).
Raw outputs, including derived transcript text, remain owner-local.

## FluidAudio Parakeet and Voz on the same machine — 2026-10-09

The previous section compared Voz with the 2026-09-07 FluidAudio run. This
replay ran both engines on the same Apple M3 Max on the same day, alternating
them so both saw the same background load. FluidAudio ran through the
unchanged `run_comparison.py` and the native probe built from `57d5fc6` with
FluidAudio 0.15.6 (revision `4dbf4f9`). It used the int8 encoder and the
Parakeet v3 Core ML models already cached by Yawn's setup. Voz ran through
`desertant` 0.2.1. Each engine made four passes over the meeting in two rounds.

The FluidAudio passes reproduced the 2026-09-07 Parakeet results exactly. That
confirms the rebuilt reference matches the original for scoring, not only in
its denominators. Each engine produced identical text on every pass. Both
columns are scored on word segments, so the term checks use the same
granularity.

| AMI diagnostic, same machine and day | Parakeet via FluidAudio | Voz |
|---|---:|---:|
| Edits divided by 7,084 manual lexical tokens; lower is better | 20.00% | 17.12% |
| Deletions / substitutions / insertions | 1,080 / 283 / 54 | 854 / 295 / 64 |
| Local terms retained in annotated decision evidence | 106/123 | 110/123 |
| Local terms retained in annotated entities | 122/132 | 122/132 |
| Distinct terms retained in the final tenth of the meeting | 231/293 | 251/293 |
| Output tokens wholly inside manual annotation gaps of 10 s or more | 1 | 9 |
| Transcription time per pass, four passes | 13.1–16.2 s | 6.1–6.9 s |
| Model load | 22.9 s first, 0.1 s cached | 0.4 s per invocation |

Unrelated work held the load average at 10 to 14 during these runs. Both
engines saw the same conditions, so the ratio is the useful figure: Voz took
about half as long. FluidAudio's pass time includes opening and validating the
WAV. Voz's excludes its model load; a whole Voz invocation took 7.1 to 7.7 s.
Neither changes the larger wait: the earlier note replay took 95 to 147 s.
Manual annotation gaps are not verified silence, so the gap-token row is a
diagnostic, not a hallucination count.

Voz's advantage is almost entirely fewer deletions. FluidAudio's output went
silent for at least five seconds in seven places where the manual words show
speech, covering 180 manual tokens. Voz had two such gaps, covering 63. Five of
FluidAudio's six largest gaps fall between 34:09 and 38:19, the dense,
overlapping close of the meeting. That is why the final tenth shows the largest
difference.

FluidAudio's `ChunkProcessor` decodes fixed windows of about 15 s with 2 s of
overlap and merges them by token timing. Voz's model card describes 15-second
windows cut at pauses and joined where neighbors agree. The gap positions were
not checked against FluidAudio's exact window grid, so this replay shows where
words are lost, not why.

**What it means:** the gap from the previous section holds under matched
conditions. It is a deletion problem concentrated in dense speech, worth about
20 terms in the final tenth of this meeting. At matched granularity the
decision-evidence difference is small (110 versus 106) and entities tie. The
next step is unchanged: adjust the FluidAudio adapter's windowing, or
FluidAudio's own long-audio options, and rescore against this replay. Getting
Voz's behavior does not require adopting Desert Ant, because the model is the
same.

This record changes no product source, contract, release state, or ASR
decision. The content-free receipt is
[`docs/evidence/voz-fluidaudio-same-day-2026-10-09.json`](evidence/voz-fluidaudio-same-day-2026-10-09.json).
Raw outputs, including derived transcript text, remain owner-local.
