# Scoring an iPad Ink meeting on the Mac — specification

The iPad Ink spike records in-person meetings. This kit answers one question
about a real two-person recording: does an automatic speaker labeler say who
spoke each turn, compared with the operator's own listening? It reuses the
Sept 28 method in `docs/transcription-engine-comparison.md` ("Retained
in-person Yawn recordings") and adds the rule learned since: review clips never
outlive the meeting's audio keep period.

Research only. Nothing here changes `apps/desktop`, the iPad app, or any
product doc.

## Files

- `spike/speech-engine-comparison/ipad_meeting_review.py`: one CLI with the
  subcommands below. Standard library only.
- `spike/speech-engine-comparison/test_ipad_meeting_review.py`: `unittest`,
  loading the module by path like `test_scoring.py` does. Synthetic data only:
  generated WAVs, made-up segments, temporary folders.
- A short "Scoring a recording on the Mac" section in `spike/ipad-ink/README.md`.

Reuse `run_fluidaudio_diarization.py` (load it by path) for running FluidAudio.
Do not change `score_diarization.py`.

## Content rules

No command prints or writes transcript text, except clip audio inside a
private packet folder. Summaries and scores are counts, durations, hashes and
labels. Every output folder is given explicitly on the command line; there are
no default paths inside the repository.

## Subcommands

### `check MEETING_DIR`

Validates an exported iPad Ink meeting folder and prints a content-free JSON
summary. Exit non-zero, naming every problem, when:

- `meeting.json` is missing, its `schema` is not `ipad-ink-meeting/1`,
  `consent_everyone_audible` is not `true`, `keep_days` is not 1, 7 or 30, or
  `created_at` is not ISO 8601;
- `audio.wav` is missing or is not 16 kHz, mono, 16-bit PCM;
- `strokes.json` is not a list of `{index, start_seconds, end_seconds}` with
  numbers or explicit `null`;
- `transcript.json` is not a list of `{start, end, text, final}` with every
  `final` true.

Summary fields: audio seconds and SHA-256, keep days, `audio_delete_after`
(`created_at` plus `keep_days`), stroke counts timed and untimed, transcript
turn count and total turn seconds.

### `label-speakers MEETING_DIR --cli FLUIDAUDIOCLI --out DIR`

Runs two arms of `run_fluidaudio_diarization.py` on `audio.wav`:
`nemotron3-c128-split-w8a8` and `pipeline-offline`. Writes each arm's
`ami-full.json` style segments under `DIR/<arm>/`. Tests use a fake CLI script
that writes a small RTTM or JSON file; no real model runs in tests.

### `packet MEETING_DIR --segments ARM_JSON --out PACKET_DIR [--seed N] [--order-seed N]`

Builds a blind listening packet from one arm's segments.

- **Predicted set for a turn**, the Sept 28 rule: a predicted speaker is
  present when the union of its active time inside the turn is at least
  `max(0.5 s, 10% of the turn)`. Predicted overlap counts for both speakers.
  No speaker qualifies → empty set.
- **Identity clips**, labelled `I1`, `I2`, …: for each of the two speakers with
  the most active time, two 5–8 s windows where only that speaker is active,
  one from each half of the meeting where possible.
- **Balanced clips**, labelled `A`, `B`, …: final transcript turns 2.5–9 s
  long, grouped by predicted set (speaker 1 only, speaker 2 only, both,
  neither). With `--seed`, pick up to three per group, spread across the
  meeting. Shuffle the listening order with `--order-seed`.
- Cut each clip from `audio.wav` as 16-bit PCM WAV into `PACKET_DIR/clips/`.
- `PACKET_DIR/manifest.json` (private): schema `ipad-ink-blind-review/1`,
  seeds, the rule text, meeting audio SHA-256, `delete_after` copied from the
  meeting, and per clip its label, kind (identity or balanced), times, SHA-256,
  predicted set and selection group.
- `PACKET_DIR/labels.json`: for each clip label, `{"speaker_set": null,
  "note": ""}`. It must not contain predictions, groups or times. The operator
  fills `speaker_set` with `["me"]`, `["other"]`, `["me", "other"]` or
  `"unclear"`.

The same inputs and seeds must produce the same packet byte for byte.

### `score PACKET_DIR --labels LABELS_JSON --out SCORE_JSON`

- Maps predicted speakers to `me` and `other` from the identity clips only. If
  the identity labels disagree, or both speakers map to the same person, write
  `{"status": "identity_unresolved"}` and exit non-zero.
- For each clearly labelled balanced clip (not `"unclear"`), compare the
  mapped predicted set with the label as an exact set match.
- Also score the baseline Yawn uses today: `["me"]` for every turn.
- Write a content-free score: counts of labelled, unclear, model matches and
  baseline matches, a per-group breakdown, and a `limit` saying this is a
  stratified sample of selected turns, not whole-meeting accuracy or DER.

### `sweep ROOT [--now ISO8601]`

Finds every `manifest.json` of schema `ipad-ink-blind-review/1` under `ROOT`
and deletes the packet's `clips/` folder when `delete_after` has passed. Report
what was removed. A packet whose manifest cannot be read is reported, not
skipped silently.

## Tests

Cover at least: a valid meeting passes `check` and each listed fault fails it;
the 0.5 s floor and the 10% rule at their boundaries, including overlap
counting for both speakers; identical packets from identical seeds; a
`labels.json` free of predictions, groups and times; identity mapping,
including the unresolved case; exact-set scoring with unclear labels excluded,
plus the baseline; `sweep` removing an expired packet's clips and keeping an
unexpired one (this test must fail if the sweep is broken); and no transcript
text in any summary or score.

## Commands

```sh
cd spike/speech-engine-comparison
python3 -m unittest test_ipad_meeting_review -v
../../.venv/bin/python -m unittest test_scoring -v
```

`test_scoring` needs RapidFuzz from `spike/speech-engine-comparison/requirements.txt`
in the research `.venv`. Without it, the scorer tests skip, and `-v` shows
the install command.
