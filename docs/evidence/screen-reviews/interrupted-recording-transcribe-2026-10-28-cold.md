---
surface: interrupted-recording-transcribe (the page and sidebar row of an interrupted meeting whose recording finished, with its new Transcribe action)
kind: cold
build: c449655 plus the pass-1 and pass-2 changes, uncommitted at capture
device: ui-harness (`apps/desktop/ui-harness/harness.html?mode=interrupted-finished`, the shipped `ui/` frontend against `tauri-stub.js`) at 1080 by 760 CSS pixels, scaled to 800 wide by the capture; light and dark by emulated color scheme; not an installed app
reviewer: three fresh subagents (Claude), one per pass, each with the frames under neutral names, the capture settings and the job questions only
implementer: the session that built and captured it (Claude), not any reviewer
cold: true
states: [finished-interrupted-light, finished-interrupted-dark]
verdict: revise on every pass; pass 3 found no contradiction and its reader would press Transcribe before the audio deadline; remaining findings are outside this change (below)
---

# Transcribe an interrupted recording that finished — cold review, 2026-10-28

**Question:** A person opens a meeting whose recording ran to its end, but which Yawn never transcribed. Can they tell what happened, what Transcribe will do, and by when to act? Does anything on the page contradict anything else?

**Before:** a meeting like this read exactly like one cut off mid-recording. The panel said "This recording did not finish. It was not transcribed." The line under the title and the sidebar row said "Interrupted". The rail ended in an amber "Recording stopped here" mark. Move to Trash was the only action. Updating Yawn did not help: there was no way to transcribe it.

| Pass | Frames | Verdict | Findings on this change |
|---|---|---|---|
| 1 | light, dark | revise | "Interrupted" under the title and "interrupted" in the sidebar contradict the panel's "The recording finished". The amber mark at the end of the rail reads as an unexplained cut-off. The panel says the audio is kept but not until when. |
| 2 | dark, light | revise | "No text contradicts other text." The audio deadline sits only in the small caption, so the panel reads as if the audio is kept indefinitely. |
| 3 | light, dark | revise | "None" contradict. The reader would press Transcribe "soon, and certainly before Nov 3". Nothing left on this change except that the page does not say what happens on that date. |

**Changed:**
- Pass 1. For a finished recording, the caption under the title reads "Not transcribed" and the sidebar row reads "not transcribed". The rail has no early-stop mark. The row learns this from a new `recordingFinished` field. It is computed from the meeting's records only, with no audio read, so library refreshes stay cheap.
- After the passes (house commit review). The page's Transcribe handle, and with it the caption, panel and rail, is issued only after the audio is hashed against its record when the meeting is opened. The sidebar row stays on the records-only check. The library's own revalidation already reads changed audio as stale, so this is a second guard, not a reachable fix.
- Pass 2. The panel names the deletion date when the meeting has one: "Its audio is kept on this Mac until Nov 3, 2026." The date is kept on one line.

Frames: `interrupted-recording-transcribe-2026-10-28/pass-1/`, `pass-2/`, `pass-3/`, with neutral-name maps in `review-map-pass*.txt`. Pass 3's frames are the final state.

**Raised but not changed, with reasons:**
- **What happens on the deadline.** Pass 3 asked the panel to say the audio is then deleted and the notes stay. That is true (retention releases audio only). But the same caption sits on every meeting with retained audio, so this belongs to the retention copy everywhere, not to one panel.
- **Why Yawn stopped.** Pass 3 asked for the cause. The app knows only that the finishing step failed, and its diagnostics folder records the code. "Yawn stopped before transcribing it" is the most the page can say without guessing.
- **Duration "6:41" beside a clock time; "1:35" beside a note; "Recording to play"; the unlabeled row dot; "Move to Trash…" not saying what moves.** All are existing Rail elements shared by every meeting, outside this change.
- **Muted secondary text in dark, and the player row overhanging the panel.** These are existing styles.
- **Installed-build frames.** These frames come from the harness, not an installed build, because the fixture seeder makes only the stopped-partway shape. A seed for the finished shape, for an installed-build review, is left for a later pass.

**What was removed, combined, demoted or hidden:**
- **Hidden:** for a finished recording only, the amber "Recording stopped here" rail mark and the "Interrupted" labels. They were untrue for it.
- **Unchanged:** a recording that stopped partway keeps all of its existing copy and offers nothing new.
- **Demoted:** Move to Trash moves from the only action to the secondary one beside Transcribe.

Checks:
- `npm run test:ui`: 197 pass. They cover the action and the handle wiring, the caption and row labels differing from the stopped-partway case, and the dated detail.
- `cargo test -p local-meeting-notes-desktop`: 281 unit tests and 6 shell-contract tests pass, including the reader tests for the handle, the row flag and the copy.
- `cargo test -p local-meeting-notes-session-core`: 552 pass.
- `cargo clippy --all-targets -D warnings`: clean on both crates.
- **Real meeting, cloned.** The session-core hand-back ran on a private clone of the affected meeting. The clone became captured, the next recovery kept it as awaiting transcription, queue discovery listed it, and its capture files were byte-identical. The clone was deleted after the run.
