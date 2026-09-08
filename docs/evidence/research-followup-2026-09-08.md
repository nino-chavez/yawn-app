# Research follow-up — 2026-09-08

The follow-up improves Settings, finding a matching passage, and microphone
failure visibility. It preserves Yawn's existing visual design and privacy
behavior. This record tracks implementation prompted by the last30days pilot
and the installed Meetily Community comparison. It does not rank transcription
or note quality.

The subsequent [whole-app layout review](layout-review-2026-09-08.md) records
additional rendered defects and their corrections across the app.

## Settings makes the next action easier to find

Source: `fd12238`.

- Recording access comes first. Transcription, Meeting notes, and Storage
  follow, with persistent section links that also move keyboard focus.
- Apple speech explains that it needs no Yawn speech-model download. Selected
  models say **Selected**, distinguishing a saved choice from work running now.
- Optional notes, download sizes, model-switching controls, and deletion rules
  remain available. Permission-check feedback appears beside recording access.
- The browser review harness loads the actual Settings markup; its old duplicate
  had drifted from the app.

The Settings interaction test passed, including switching both ways, a refused
switch, and preserving the optional note model. Browser checks passed at
720 × 720 and 560 × 480 in light and dark appearance. They checked section
focus, headings below the sticky header, horizontal overflow, and engine
switching against a synthetic bridge.

An independent reviewer opened the four retained frames and found no material
visible defect. These are **synthetic browser renders**, not installed-app
captures. They do not establish native permission behavior, real downloads, or
user understanding. The numbers in the fixture describe invented app state;
they are not storage measurements from this Mac.

Retained review evidence:

- [Recording access](screen-reviews/captures/research-followup-2026-09-08/settings-720-dark-recording.png)
- [Transcription choices](screen-reviews/captures/research-followup-2026-09-08/settings-720-light-transcription.png)
- [Optional notes](screen-reviews/captures/research-followup-2026-09-08/settings-560-light-notes.png)
- [Storage and retention](screen-reviews/captures/research-followup-2026-09-08/settings-560-dark-storage.png)
- [Browser check results](screen-reviews/captures/research-followup-2026-09-08/settings-check.json)

## Search results lead to the matching retained turn

Sources: `65f64d5` and `caa6eec`.

The existing default-off search probe now opens the matching transcript turn,
expands its disclosure, moves keyboard focus there, and highlights the turn.
Title-only results open the meeting without inventing a turn match. Withheld
turns remain redacted. Changing the query or leaving the reader invalidates
pending opens.

The reader checks that the transcript still has the digest belonging to the
search result before applying its locator. A changed or unavailable transcript
asks the reader to search again. This prevents an old turn index from
highlighting different text after a transcript retry. The result's `start` and
`end` values are character offsets, not audio timestamps.

Rendered review also found overlapping transcript controls, a highlight that
depended on keyboard-focus styling, and page scrolling that hid the app toolbar
at the minimum window size. The controls now wrap, the target has its own
highlight, and automatic scrolling stays inside the reading panes.

The system-audio correction dialog now says **Label system audio** and explains
that several people can share that channel. It preserves the original transcript
and the existing correction operation. Misleading **Live transcript** wording
was removed from the capture view; transcription still happens after stopping.

Validation:

- `npm run test:ui` passed on the combined source.
- `./ui-harness/run.sh search` passed in WKWebView with synthetic responses:
  exact and withheld matches, title-only results, changed and unavailable
  transcripts, late results after leaving, incomplete coverage, and capture
  blocking. [Run output](screen-reviews/captures/research-followup-2026-09-08/search-check.log).
- The existing editor, interaction, and sheet checks passed with
  `./ui-harness/run.sh all`; the visual-fidelity check also passed.
- Browser layout checks passed at 960 × 760, 900 × 600 (the app minimum), and
  1280 × 900 in both appearances. Buttons remained inside the transcript pane,
  heading and actions did not overlap, and the highlight and app toolbar stayed
  visible. [Measurements](screen-reviews/captures/research-followup-2026-09-08/search-layout-check.json).
- An independent cold review found no material visible defect in the corrected
  [search frame](search-match-focus.png),
  [minimum-size frame](screen-reviews/captures/research-followup-2026-09-08/search-900-light.png),
  and [system-audio correction dialog](speaker-channel-correction.png).

All search and dialog captures use invented meetings. They establish rendered
presentation with synthetic data, not installed behavior or search usefulness.
The local marker was not enabled. The existing one-week usage probe remains the
input to the ship/hold decision; this fix does not make search generally available.

## Microphone delivery now has a timeout

Sources: `1984f9e` and `f3ddb0f`.

Once both audio sources are ready, the capture coordinator watches microphone
buffer delivery with a ten-second grace period. Zero-valued samples count as
healthy delivery. Pause disables the timer; successful resume starts a fresh
grace period. Old callbacks cannot make a resumed source look ready or healthy.
An accepted audio or file-writing fault remains fatal for that take.

A stalled microphone now stops recording with a microphone-specific message
and advice to check its connection. The take is not marked complete. Simulated
failures retained partial WAVs rather than promoting them to complete captures.
The combined source passed the native `meeting-capture-self-test`; the desktop
test suite also passed with the new error messages.

**System-audio timeout detection remains disabled in the actual helper.** The
first candidate would have monitored both legs. Review found no established
native guarantee that a healthy but idle system tap emits nonempty PCM buffers.
Enabling its timeout could therefore stop a quiet meeting. The opt-in path is
covered with fake sources only.

The next recording check must establish quiet native system-tap callback cadence,
then exercise an interrupted source and an audio-route change. A metadata-only
probe should retain callback times and byte counts, never audio samples. No
new recording or physical-device interruption was performed for this change.

## Research supplies scenarios to test

The last30days pilot supplied reports about interrupted audio, forgotten
commitments, mistaken speaker ownership, and recording expectations. Those
reports are prompts for validation; they are not reproduced Yawn defects.
Its reviewed brief remains in the operator's research output at
`~/Documents/Codex/2026-09-08/orie/outputs/last30days-yawn-pilot.md`.

The Meetily comparison inspected the installed Community 0.4.0 empty library,
expanded sidebar, Settings, transcription and summary configuration, and import
dialog. It did not record a meeting or establish transcription quality. Private
comparison captures remain under
`~/Library/Application Support/yawn-research/meetily-comparison-2026-09-08/`.

Import, live transcription, and individual speaker identification remain separate
product proposals. None follows automatically from their presence in another
app. Commitment accuracy and supporting-source quality remain in the explicitly
deferred human review. Consent and retention still need a person's understanding
check; readable controls alone do not prove that understanding.

## Delivery boundary

These changes were included in the signed, notarized, installed, and publicly
released 0.6.4 artifact at `58e13a2`. See the
[release receipt](../distribution-runbook.md#064-release-receipt).
Transcript-content search remains default-off; including its implementation
in the package does not make it a generally available feature. The checks above
remain source and synthetic-render evidence. Neither the earlier progressive-setup
receipts nor package verification establish installed 0.6.4 interaction acceptance.
