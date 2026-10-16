# Mac Rail captures, 2026-10-16

## harness/ — WKWebView, 1080 by 900 at 2x, stub data

Frames of the real UI code (`apps/desktop/ui` at the commit this review
names) rendered by the harness's WKWebView runner (`ui-harness/run.sh still
<mode>`, `still-settings`) against `tauri-stub.js` data. The renderer is the
one the installed app uses; the data and the window chrome are the stub's.
These are not captures of the installed build: the Fixture app was built and
its root seeded with the 23-minute ledger meeting, but driving it needed the
operator at the Mac (the Computer Use approval dialog went unanswered and
the app would not come forward), so the installed pass is still owed.

| File | State | Appearance |
|---|---|---|
| ledger-{light,dark}.png | after the meeting: seven timed lines, nearest turns, folds, one line after Stop, transport with ticks, one pause | both |
| capture-{light,dark}.png | during a recording: the live page, the toolbar's badge, Pause and Stop | both |
| interrupted-{light,dark}.png | a recording that did not finish: attention surface, one timed line, the stop mark | both |
| summary-failed-{light,dark}.png | a transcript with no AI draft: the plain note-area detail, the ledger without moments | both |
| first-run-{light,dark}.png | no meetings yet: the first-run page | both |
| settings-{light,dark}.png | the single-form Settings window, 580 by 640 | both |

Large text and increased contrast were not captured here: the harness runner
has no switch for either, and the installed pass owes them.

The cold packet used neutral names: m1 ledger-light, m2 ledger-dark, m3
capture-light, m4 capture-dark, m5 interrupted-light, m6 interrupted-dark, m7
summary-failed-light, m8 summary-failed-dark, m9 first-run-light, m10
first-run-dark, m11 settings-light, m12 settings-dark.
