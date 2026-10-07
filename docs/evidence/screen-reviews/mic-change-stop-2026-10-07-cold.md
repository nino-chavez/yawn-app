# Recording stopped by a microphone setup change — cold review, 2026-10-07

**Kind:** cold screen review, one pass. A fresh reviewer saw only the four rendered frames and the questions below, with no source code, no design rationale and no description of the fix. The frames come from the synthetic WKWebView harness (`ui-harness/run.sh mic-change`, modes `mic-change-stop` and `mic-change-failed`), which drives the production frontend against a stubbed backend using the backend's copy verbatim. They are not captures of the installed app, and no real microphone change occurred.

**Questions:** Situation A — the person was recording and did not press Stop; this is the screen moments later. Situation B — the person had just pressed Record; this is the screen moments later. For each: what happened, is the audio saved and how sure can they be, what happens next and what must they do, what is confusing, contradictory, alarming, easy to miss or redundant, and is every message readable in dark and light.

| Situation | New copy under review | Verdict | Finding attributable to this change |
|---|---|---|---|
| A, automatic stop, take saved | toast: "Recording stopped because the microphone setup changed. Your audio was saved and queued for transcription." | revise | The cause appears only in the dismissible toast; the persistent caption says "Recording stopped. Audio saved on this Mac." without why. Suggested: carry the cause into the persistent caption. |
| B, change while arming, fails | context line: "Recording stopped because the microphone setup changed. Check your audio input before starting another recording. Nothing was marked complete." | revise | "Nothing was marked complete" does not say whether any audio exists. Suggested: state the outcome plainly. |

**Status: operator decision.** Both findings are copy choices. A: whether the persistent capture caption should name the cause. B: "Nothing was marked complete." is the ending every capture failure message shares today (for example the stalled-audio messages), so changing it here alone would make one failure read differently from the rest.

Raised but not attributable to this change, with reasons:
- A: the same "saved" fact stated three times, and "Transcribing on this Mac" repeated in header, sidebar and status line — the existing stopping/transcribing surface (cold-reviewed 2026-09-29), unchanged here. "Waiting for transcription status" — existing.
- A: the sidebar duration reads 0:00 in one theme and 0:01 in the other — a harness artifact; the stub's recording lasts about a second and the two captures ran separately.
- B: "Review what survived before you start another recording." shown twice, the dimmed Record button, and "Back to Meetings" — the existing interrupted-capture surface, unchanged here.
- B: the sidebar shows the first-run "Press Record" text — a harness artifact; this mode's stub has an empty library.

All messages were judged readable in both themes.

What was removed, combined, demoted or hidden: nothing. The change adds one notice (A) and one failure message (B). Before it, situation A was a failed recording whose audio was not saved, with a generic message.

![Automatic stop, dark](mic-change-stop-2026-10-07/mic-change-stop-dark.png)
![Automatic stop, light](mic-change-stop-2026-10-07/mic-change-stop-light.png)
![Change while arming, dark](mic-change-stop-2026-10-07/mic-change-failed-dark.png)
![Change while arming, light](mic-change-stop-2026-10-07/mic-change-failed-light.png)

Checks: `./run.sh mic-change` passed both modes in dark and light. The `mic-change-stop` mode failed with `main`'s `ui/main.js` (the notice never appeared) and passed with this change, each on a harness port not used before; reusing a port can serve a cached `main.js` from the runner's WebKit cache. This review does not establish behavior in an installed build or during a real call.
