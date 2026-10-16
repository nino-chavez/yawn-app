---
surface: mac meeting page (an interrupted recording; a meeting whose personal note cannot be read)
kind: cold
build: the commit that adds this record; frames in mac-interrupted-copy-2026-10-16/ are from its tree
device: WKWebView harness, 1080 by 900 at 2x, stub data, light and dark (not the installed build)
reviewer: two fresh subagents (Claude), one per pass, frames and job questions only
implementer: the parent design session (Claude), not either reviewer
cold: true
states: [interrupted-light, interrupted-dark, note-unreadable-light, note-unreadable-dark]
verdict: revise (the remaining items are the operator's open decision or unbuilt actions; the three findings this change answers are resolved)
---

# Interrupted recording and unreadable note, copy fixes — cold review, 2026-10-16

**What this answers.** Three findings from
`mac-fixture-interrupted-2026-10-16-cold.md`: the unverifiable-pause sentence on
an interrupted meeting, the hedged banner detail ("Any retained audio remains
available"), and the unreadable-note line that gave no consequence at caption
contrast.

**Frames.** `mac-interrupted-copy-2026-10-16/`: the harness's `interrupted`
mode and a new `note-unreadable` mode, both appearances. Harness frames of the
real UI code against stub data; the stub's interrupted message is the new
library_reader text.

| Pass | Verdict | What it found |
|---|---|---|
| 1 | both revise | The banner said "interrupted" in its headline and its body; "personal note" differs from the heading "Your notes"; the note line was caption size, about 120 characters wide, and gave no meaning for the reader; the banner gave no next step; Move to Trash is the only action |
| 2 | both revise | No hedge; "did not finish" once in the main pane plus the meta line's state word; the note line is comfortable in size and contrast and says what happened and what it means; "above" points at the layout; still no action beyond Trash; "Not editable" repeats the line; no "show file" action |

**Changed.** The pause fact no longer shows on an interrupted meeting whose
pauses could not be checked (a recorded pause still shows). The banner detail
is definite and states the consequence once: "It was not transcribed. Its audio
is kept on this Mac and can still be played." (Rust and the UI fallback agree.)
The unreadable-note line reads "Yawn could not read your notes for this
meeting, so they are not shown and nothing will be written over them. The file
stays on this Mac, and the rest of this meeting is not affected.", at body size,
Label color and the reading measure.

**After pass 2, not re-reviewed.** "can be played above" became "can still be
played" (the reviewer flagged the spatial pointer), and the message is now
chosen by whether a playback handle exists, with "It was not transcribed, and
its audio cannot be played." for a retained meeting with no playable file. No
reviewer saw these two wordings.

**Not changed, and why.** The interrupted banner's only action (Move to Trash)
and whether a transcript can ever be made are the operator's open decision.
The rail's stop mark is decided design. "Not editable" is the note section's
state word, shared with every meeting. A "show file" action is not built.
The meta line's state word stays: it is the page's one-line summary.

**Found along the way, fixed here.** The harness rendered a blank page on
main since `appearance.mjs` and `text-size.mjs` landed: `run.sh` served a
hand-kept file list. It now serves every UI module. With the harness working,
the fold gate's `note-does-not-move` failed about half its runs on main: the
editor kept a mirror height one wrapped line short. The ledger now re-lays out
when the mirror's height drifts from the one it used, checked one frame later
and by a timer, because a hidden window (the harness; a minimized app) runs no
frames or resize observers. 12 of 12 gate runs pass after the fix. Why the
mirror grows after the layout reads it was not found; the recheck works
around it, it does not explain it.

**Removed, combined, demoted, or hidden.** Hidden: the unverifiable-pause fact
on an interrupted meeting. Removed: the banner's second "interrupted". Nothing
combined or demoted.
