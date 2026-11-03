---
kind: cold
surface: Yawn desktop review prototype
build: Uncommitted study on fc23a11 before the Settings-host fix; superseded by 917c136
source_capture: Original manifest and blank Settings frames superseded; meeting frames retained
reviewer: Codex worker 01a1a104-9c51-71c0-94c7-f9757d8d129c via Operator dispatch 2c230efa-4a07-492e-9f86-e7c9be83d340
implementer: Codex parent chat 01a198a4-399c-7363-b9d1-babb0e76f147
cold: true
device: Offscreen WKWebView content on this Mac; 1080x900 main and 580x640 Settings; default text; OS contrast unverified
states: 01 resting, 02 related, 03 expanded, 04 recording, 05 empty, 06 interrupted, 07 Settings; light and dark
verdict: revise; meeting states accepted, original Settings frames blank; see focused verification
---

# Cold screen review

## Evidence limits

Reviewed only the original 14 PNG frames and `frames/manifest.json`. The current `07-*` files were replaced after this review; the original blank Settings images were not retained. This is the historical finding, not a verdict on the corrected files. See [focused verification](desktop-revisit-20261103-settings-fix.md) for the corrected capture verdict. Frames are offscreen WKWebView captures with synthetic data, not installed-app acceptance. This review does not verify interaction, accessibility settings, motion, audio, storage, or real-meeting behavior.

## Overall

The populated, recording, empty, and interrupted states communicate their primary condition and the obvious next action. Light and dark treatments remain readable and correspond closely.

One mechanical capture failure prevents acceptance: both 580 × 640 frames are entirely blank.

## Neutral-frame verdicts

| Frames | Verdict | What is visible |
|---|---|---|
| `01-light.png`, `01-dark.png` | Accept | A completed meeting with date, time, duration, audio-retention date, notes, nearby transcript context, playback, search, and recording controls. The next actions and named people are legible. |
| `04-light.png`, `04-dark.png` | Accept | Recording is plainly underway; elapsed time plus Pause and Stop make the immediate choices clear. The frame does not identify other participants or a place. |
| `05-light.png`, `05-dark.png` | Accept | Empty-state hierarchy is clear: there are no meetings yet, the app is local to this Mac, and Record is the next action. |
| `06-light.png`, `06-dark.png` | Accept | Interrupted status, retained audio, absent transcript, and the available disposal action are all explicit. |
| `07-light.png`, `07-dark.png` | Revise | Both frames are completely blank. No state, context, or action is visible. |

`02-*` and `03-*` are non-neutral selected/revealed variants of the completed-meeting view. Their selection and expanded transcript context remain visually connected to the selected note; no additional material issue is visible.

## Material fixes

1. **Mechanical visible defect — `07-light.png` and `07-dark.png`, entire 580 × 640 viewport:** render the intended narrow-window content. These frames are blank, so none of the five review questions can be answered.

## Baseline product judgment

The visible desktop baseline answers what is happening, what comes next, when, and what can be done in every nonblank state. In the completed state it also names people through notes and transcript excerpts. The active-recording frame does not show other participants or a meeting location; that is a visible limitation of this state, not a mechanical defect or an interaction finding.