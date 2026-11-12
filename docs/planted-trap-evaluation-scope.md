# Scope — a planted-trap check for exact names and numbers

Status: slices 1 to 3 built and the direct-decode arm run on 2026-11-11.
[Results](#results--direct-decode-arm-2026-11-11) are below. The capture-path
arm needs the operator at the Mac; the note arm stays deferred.

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
  `40faa4d244b9c621ca3a2c6596b7c81a70719a2a7b17f1019cd641ca5f4bb6b3`.
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
- **Built.** [`notes/render_trap_audio.py`](../notes/render_trap_audio.py)
  voices `Me` with Samantha (en_US) and `Them` with Daniel (en_GB), writes one
  16 kHz mono leg per speaker plus a mix, and refuses to write inside the
  repository. Its `manifest.json` is the `speech-engine-corpus/1` shape that
  `spike/speech-engine-comparison/run_comparison.py` already accepts.

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
- **Built.** `python notes/compare_transcripts.py --traps <key> [--speaker Me]
  run.json ...` reads probe results (every run) or transcript files. Each
  transcript is aligned to the script by its words; a trap is looked for only
  in the stretch aligned to its own line, and the words heard in its place are
  recorded positionally, not by similarity. `notes/test_trap_scoring.py` covers
  the clean and mangled controls, a form said in another line not counting,
  dropped lines and dropped words, and one leg scoring only its speaker. The
  first test run caught two scorer bugs (a garble at a line's first word was
  credited to the line before, and the "heard" words were picked by
  similarity); both are fixed and tested.

### 4. Note arm — deferred

New note generation is retired in source:
`product_facade.rs` refuses `regenerate_note` with `NoteGenerationRetired`,
and `docs/product-brief.md` says the same. The article's other test, whether a
summary catches decisions, owners and deadlines, and whether it promotes an
open question into a decision, has no product path to measure. If generation
returns, reuse `check_owner_grounding`, `check_recall`, and `check_numbers` in
`notes/summarize.py`. Add one category the current checks miss: a claim that is
faithful to a wrong transcript passes grounding but fails against the key.

## Results — direct-decode arm, 2026-11-11

Three runs per engine on the rendered audio, Apple-silicon Mac on macOS 27.2.
Whisper is `whisper-large-v3-turbo` full model, revision `a4aaeec0`, the
catalog's "Full model" (the 4-bit shipping default is not on this Mac).
Parakeet is FluidAudio 0.15.6, Parakeet TDT 0.6B v3, the model Meetily ships.
Apple is SpeechTranscriber. Audio SHA-256 prefixes: `trap-me` `3516fe23e863`,
`trap-them` `f12868b55163`, `trap-mixed` `dca7e39ae85e`. Raw output and
receipts stay in the owner's
`~/Library/Application Support/yawn-research/trap-check-2026-11-11`.

| Engine | Traps kept per run (of 18) | Without T12 (of 17) | Missed, as heard |
| --- | ---: | ---: | --- |
| Whisper | 15 | 15 | `4070T Super`, `post-GurSQL` |
| Parakeet | 13 | 13 | `Shavan`, `When` (Nguyen), `4070T Super`, `1124 H2` |
| Apple | 12 | 12 | `When` (Nguyen), `4070 T Super`, `1124 H2`, `post-Ger SQL`, `230 PM` |

**What it means**

- **The traps separate the engines,** so the falsifier did not fire. Whisper
  kept the most. That supports keeping it as the shipping baseline; it does
  not by itself settle the open engine decision, which also weighs waiting
  time.
- **Parakeet loses names.** It wrote "Shavan" for Siobhan and "When" for
  Nguyen, the same failure class the XDA test saw in Meetily. Apple shares the
  Nguyen error and also merges numbers ("Windows 11 24H2" became "1124 H2",
  "2:30" became "230").
- **Every engine lost "Ti".** "RTX 4070 Ti Super" came back as "4070T" or
  "4070 T" from all three.
- **Mixing the legs cost nothing here.** Each engine's per-leg scores sum to
  its mixed score. That holds for clean synthetic legs only.
- **Vocabulary fixability is not established.** Every miss repeated
  identically across the three runs, but each run decoded the same audio, so a
  repeat is expected and proves little. Whether "4070T" recurs across
  different recordings and voices is the real question, and only more
  recordings (the capture-path arm, or more voices) can answer it.

**Corrections made before these numbers**

- **T10 key widened.** All three engines wrote "the 3rd of December". The
  British voice reads the date day-first, and that transcript is correct, so
  "3rd of December" and "third of December" were added as accepted forms. The
  script text did not change, so the rendered audio still matches; the key's
  frozen hash moved to `40faa4d2…b6b3` and the fixture test pins it.
- **T12 is a voice artifact, kept in the table but excluded from the second
  column.** All three engines heard "N4417", so the voice said "en", not
  "E-N-G". The capture-path arm, read by a person, tests it properly.

**A separate finding, diagnosed the same day: the app is not affected.** The
first Whisper attempts were killed (exit 137) because they ran the bundle's
`bin/python3`. In the installed 0.6.19 bundle, `python3` and `python` are
separate copies of the interpreter, signed with the Hardened Runtime but with no
entitlements. Importing `numba` runs llvmlite's JIT self-check
(`check_jit_execution`), and macOS kills a hardened process that executes
JIT-written code without `com.apple.security.cs.allow-unsigned-executable-memory`.
`scripts/sign-notarize.sh` grants that entitlement to `bin/python3.12` only,
and `app-runtime.json` points the app at `python-runtime/bin/python3.12`. From
the same shell, `bin/python3.12` imports `mlx_whisper.transcribe` and `numba`
and passes the JIT check. A transcription inside the app was not run. Research
runs against the installed runtime must use `bin/python3.12`.

**Still open:** the capture-path arm (about ten minutes of the operator's
time) and the note arm (deferred until generation returns).

## Not in scope

- Any change to the shipping engine, the app, or the minimum OS.
- Speaker diarization; Yawn attributes by channel only.
- Ranking Yawn against Otter or the article's tools. The article's test was one
  speaker on Windows, which is not Yawn's setup.

## Size

Slices 2 and 3 shipped as one branch. The capture-path arm needs about ten
minutes of the operator's time at the Mac.
