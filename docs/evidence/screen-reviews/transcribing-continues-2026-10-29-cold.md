---
surface: transcribing-continues (the meeting page's transcribing status, now saying what leaving, closing the window and quitting do)
kind: cold
build: feat/transcribing-continues at af3fd61
device: ui-harness (`apps/desktop/ui-harness/harness.html?mode=transcribing-progress` and `-none`; the shipped `ui/` frontend against `tauri-stub.js`) at 1080 by 760 CSS pixels, scaled to 800 wide by the capture; light and dark by window appearance; invented meeting; not an installed app
reviewer: one fresh subagent (Claude), given only the frames under neutral names, the capture settings and the job questions
implementer: the session that drafted the copy and its desktop-lane subagent (Claude), not the reviewer
cold: true
states: [progress-40-light, progress-40-dark, no-progress-light, no-progress-dark]
verdict: revise; both new sentences were understood and acted on correctly (leave or close, keep going; quit, start over). The one suggestion on this change, giving the quit sentence its own stronger line, is left to the operator. The rest is outside this change
---

# Transcribing status says what leaving does — cold review, 2026-10-29

**Question:** A person opens a meeting while Yawn transcribes it. Would they wait, leave the page, close the window, or quit, and what do they expect each to do?

**Before:** "Your audio is saved on this Mac and is being transcribed. The transcript will appear here when it’s ready." Three cold passes on the 2026-10-29 progress bar all asked whether the work continues if the page is left or the app quits.

**Now:** two sentences are added: "You can leave this page or close the window. If you quit Yawn, transcription starts over the next time you open it." Each is verified in code.
- **Close the window:** it only hides the window, and the menu-bar app keeps running (`main.rs` `CloseRequested` → `prevent_close`).
- **Quit:** the worker exits when its parent-liveness pipe closes. At the next launch, `recover_transcription_queue` releases the claim and the meeting is transcribed again. Tested by `a_claim_left_by_a_quit_mid_run_is_released_and_the_meeting_is_claimed_again_from_scratch`.
- **Sleep:** not mentioned. It is inferred to pause (Rust `Instant` is `CLOCK_UPTIME_RAW`; the bundled Python 3.12's monotonic clock is `mach_absolute_time()`), but no real sleep was tested.

| Pass | Frames | Verdict | Findings on this change |
|---|---|---|---|
| 1 | 4 (40% and no bar; light and dark) | revise | Leave or close: "I expect it to keep going, which is reassuring." Quit: "At 40% I would not quit." The reader called the quit sentence "the most consequential sentence on the screen". They suggested pulling it out of the grey paragraph onto its own stronger line ("Quitting loses this progress (40%)"), ideally with a confirmation on quit. |

**Not acted on:**
- **A separate, stronger quit line, or a confirm-on-quit dialog.** The reader already drew the right conclusion from the inline sentence. On this Mac a 66-minute meeting transcribes in about two minutes, so a bold warning would overstate it. A quit confirmation is new behavior. Both are the operator's call.
- **Repeated from the progress review:**
  - no moving element without a percent ("Starting…");
  - no percent in the sidebar;
  - no time estimate;
  - dark-mode contrast of the placeholder and chips;
  - no visible edit affordance on the notes;
  - "Audio kept" duplicating the sentence.
- **"40% transcribed ends a few pixels past the paragraph's right edge."** The paragraph is ragged-right text at the same measure as the bar line, so its last word stops short of the line's end. This is not a misalignment.

Frames: `transcribing-continues-2026-10-29/pass-1/`, neutral-name map in `review-map-pass1.txt`.
