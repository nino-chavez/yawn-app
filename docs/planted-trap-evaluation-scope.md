# Scope — a planted-trap check for exact names and numbers

Status: scoped on 2026-11-11. Slice 1 is built: the operator chose the narrow
gate exemption the same day. Slices 2 and 3 need their own go-ahead.

**Recommendation:** build a small, synthetic test script with deliberate traps,
and score transcripts against it. It extends `notes/compare_transcripts.py`; it
is not a new evaluation system. Score the transcript only. A note-side arm waits
until note generation returns to the product.

## Why this, and why now

An XDA comparison of three open-source Otter alternatives
([article](https://www.xda-developers.com/i-tried-3-open-source-alternatives-to-otter-for-my-meeting-notes-and-one-runs-entirely-on-my-pc/),
read 2026-11-11) used one scripted three-minute briefing with 15 planted traps,
mostly exact product and version names. The fully local tool, Meetily (Parakeet
TDT 0.6B plus a 4B summary model), scored lowest on the transcript: it turned
"RTX 4070 Ti Super" into "470T Super", and its summary repeated the error.

Yawn already measures something close. `compare_transcripts.py` checks whether
the terms a meeting's commitments rest on survive, and the
[engine comparison](./transcription-engine-comparison.md) reports term retention
for decision evidence, entities, and the final tenth of a meeting. Those terms
come from whatever a reference meeting happened to contain. Three things are new:

1. **Adversarial terms.** Model numbers, version strings, spelled digits,
   near-homophones and run-together names. These are the strings a speech
   engine gets wrong and a reader cannot recover from context.
2. **A source that is safe to commit.** The script is invented, so the answer
   key and its expected transcript can live in Git. Every engine run gets
   exact ground truth without private audio or hand annotation.
3. **Vocabulary fixability.** Yawn's local vocabulary is a list of exact,
   case-sensitive replacements (`crates/session-core/src/local_vocabulary.rs`).
   It can repair an error only when the engine makes the same mistake every
   time. The traps measure what share of failures is consistent, and therefore
   repairable by a vocabulary entry.

## What it would decide

| Result | Decision it moves |
| --- | --- |
| Trap survival per engine (Whisper, Apple, Parakeet) | The open engine choice in the engine comparison. Today Parakeet is fastest, and Whisper stays the shipping baseline. |
| Direct-decode vs capture-path survival | Whether the two-leg tap and resampling lose exact terms. Earlier term checks found no loss; traps are a harder test. |
| Share of failures that repeat identically across runs | Whether local vocabulary is the right repair, or whether a decoder-level hint is needed. |

**Falsifier.** If Whisper keeps every trap on both arms, the traps cannot
separate the engines. The fixture then becomes a regression check and settles
no open decision. Say so in the result rather than adding harder traps until
something fails.

## Slices

### 1. Script and answer key

- A two-speaker script of about three minutes, with 15 to 20 traps. Categories:
  product and model numbers, version strings, dates and deadlines, currency
  and quantities, proper names, and one near-homophone pair.
- Each trap records the exact spoken form, the accepted written forms (for
  example `26H2` and `26 H2`), its speaker, and its position in the script.
- **Privacy gate — decided 2026-11-11: narrow exemption.** `privacy_gate.py`
  now skips a JSON file only when it sits directly in `notes/fixtures/synthetic/`
  **and** its top-level object has `"synthetic": true`. Either condition alone
  is refused, as is a subdirectory, a header that is not literally `true`, or a
  staged copy without the header. The gate's self-test covers each case, and
  two deliberately broken copies of the gate each failed it. The OK line counts
  the files judged by their header, so the exemption is visible.
- The gate already refuses `origin/main` at `8195de7`: about 500 locations,
  including UI-harness capture fixtures and `apps/desktop/package.json`. It is
  not running as a commit gate today. Restoring it is separate work, but an
  exemption added for the trap key should not widen what already fails.
- **Built.** [`notes/fixtures/synthetic/trap-briefing.json`](../notes/fixtures/synthetic/trap-briefing.json)
  holds the script (15 lines, 282 words, about two minutes spoken) and 18
  traps in nine categories. Script and key are one file, so they cannot drift.
  Frozen SHA-256:
  `2c8e19db634c041428f1b448d26c0cf9cabe6af6db6f5d033e00d9c1b112102c`.
  If this hash changes, results from before and after are not comparable.
- The same file carries the decisions, open question, action items and
  note-level traps for a future note arm, under `note_claims_deferred`, so the
  script need not change if generation returns.
- `notes/test_trap_fixture.py` checks that every trap is spoken in its line and
  accepted, that IDs are unique, and that the key is exempt only because of
  its header. A planted misspelling made it fail.

### 2. Audio

- **Direct-decode arm.** Render each speaker's lines with a local voice
  (`say` or Kokoro), one WAV per leg. This measures the engine alone, with no
  person involved.
- **Capture-path arm.** Play the far leg as system audio through headphones
  while the operator reads the near leg, using the capture steps in
  [real-meeting-handoff.md](./real-meeting-handoff.md). The recordings are the
  operator's voice: they stay outside Git, with only hashes recorded.
- Limit to state in the result: synthetic voices are cleaner than people, so
  direct-decode survival is an upper bound.

### 3. Scorer

- Add a `--traps <key>` mode to `notes/compare_transcripts.py`. For each trap,
  report kept (an accepted form appears at its position), wrong (a different
  string appears there, recorded verbatim), or missing.
- Run each engine three times per arm and record whether each wrong form
  repeats. A form that repeats across all three runs counts as
  vocabulary-fixable.
- **Negative control.** A copy of the expected transcript with every trap
  mangled must score 0 kept and exit non-zero. A clean copy must score every
  trap kept. Run both before trusting any engine result.

### 4. Note arm — deferred

New note generation is retired in source:
`product_facade.rs` refuses `regenerate_note` with `NoteGenerationRetired`,
and `docs/product-brief.md` says the same. The article's other test, whether a
summary catches decisions, owners and deadlines, and whether it promotes an
open question into a decision, has no product path to measure. If generation
returns, reuse `check_owner_grounding`, `check_recall`, and `check_numbers` in
`notes/summarize.py`. Add one category the current checks miss: a claim that is
faithful to a wrong transcript passes grounding but fails against the key.

## Not in scope

- Any change to the shipping engine, the app, or the minimum OS.
- Speaker diarization; Yawn attributes by channel only.
- Ranking Yawn against Otter or the article's tools. The article's test was one
  speaker on Windows, which is not Yawn's setup.

## Size

Slices 2 and 3 are one short branch: rendered audio, a scorer flag with tests,
and one result section appended to this document. The capture-path arm needs
about ten minutes of the operator's time at the Mac.
