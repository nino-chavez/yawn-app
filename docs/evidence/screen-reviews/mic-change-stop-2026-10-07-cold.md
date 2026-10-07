# Recording stopped by a microphone setup change — cold review, 2026-10-07

**Kind:** cold screen review, two passes. Each pass used a fresh reviewer who saw only four rendered frames and the questions below, with no source code, no design rationale and no description of the fix. The frames come from the synthetic WKWebView harness (`ui-harness/run.sh mic-change`, modes `mic-change-stop` and `mic-change-failed`), which drives the production frontend against a stubbed backend using the backend's copy verbatim. They are not captures of the installed app, and no real microphone change occurred.

**Questions:** Situation A — the person was recording "Harness meeting" and did not press Stop; this is the screen moments later. Situation B — the person had just pressed Record to start a new meeting; this is the screen moments later. For each: what happened, is the audio saved and how sure can they be, what happens next and what must they do, what is confusing, contradictory, alarming, easy to miss or redundant, and is every message readable in dark and light.

| Pass | Frames | Verdict | Main finding |
|---|---|---|---|
| 1 (superseded) | A stubbed as "transcribing" with the capture pane still shown; B with an empty library | revise / revise | Judged a state the backend never produces: after any finalized take it steps Captured → Idle and clears the meeting projection. Caught by the automated commit review. Its findings are not carried forward. |
| 2 | A as the real idle state: the just-recorded meeting opens with no transcript yet (the reader's default `transcript-only` branch), background transcription active; B with a populated library and a new meeting id | revise / revise | See below |

**Pass 2, situation A** — new copy: toast "Recording stopped because the microphone setup changed. Your audio was saved and queued for transcription."
- The reviewer read the audio as saved and the next step as automatic.
- Attributable to this change: the toast is the only place the cause appears, and it is dismissible.
- Not introduced by this change, but made more visible by it: the meeting page for a still-queued take says "Automatic note generation is no longer available. Your notes and transcript remain available.", its caption reads "Transcript", and the sidebar row reads "note only" — while no transcript exists yet. A normal Stop lands on the same page. Suggested: while transcription is pending, show a status such as "Transcribing your saved audio. It will appear here when ready."

**Pass 2, situation B** — new copy: context line "Recording stopped because the microphone setup changed. Check your audio input before starting another recording. Nothing was marked complete."
- Attributable to this change: "Recording stopped" describes a recording that never started (the change came while arming), and "Nothing was marked complete." does not say whether any audio exists. Suggested: "Recording did not start because the microphone setup changed. No audio was captured. Check your input, then press Record." The same message is also used when a paused recording cannot resume, where "stopped" fits, so the wording choice covers both.
- Not introduced by this change: "Review what survived before you start another recording." shown twice, the disabled Record button, and the near-empty interrupted page.

**Status: operator decision** on three copy choices: (1) whether the cause should persist beyond the toast; (2) the arming/resume failure wording, including whether to depart from the "Nothing was marked complete." ending every capture failure shares; (3) whether to fix the queued-meeting page now or separately — it affects every stop, not only this one.

Harness artifacts, not findings: sidebar times are synthetic (an "earlier" meeting can show a later clock time); the meeting is named "Harness meeting".

All messages were judged readable in both themes.

What was removed, combined, demoted or hidden: nothing. The change adds one notice (A) and one failure message (B). Before it, situation A was a failed recording whose audio was not saved, with a generic message.

![Automatic stop, dark](mic-change-stop-2026-10-07/mic-change-stop-dark.png)
![Automatic stop, light](mic-change-stop-2026-10-07/mic-change-stop-light.png)
![Change while arming, dark](mic-change-stop-2026-10-07/mic-change-failed-dark.png)
![Change while arming, light](mic-change-stop-2026-10-07/mic-change-failed-light.png)

Checks: `./run.sh mic-change` passed both modes in dark and light. The `mic-change-stop` mode failed with `main`'s `ui/main.js` (the notice never appeared) and passed with this change, each on a harness port not used before; reusing a port can serve a cached `main.js` from the runner's WebKit cache. The queued-meeting page is stubbed from the reader code (`library_reader.rs` default branch), not observed in an installed build. This review does not establish behavior during a real call.
