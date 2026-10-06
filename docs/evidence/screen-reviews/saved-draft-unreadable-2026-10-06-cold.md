# Withheld saved AI draft — cold review, 2026-10-06

**Kind:** cold screen review. The reviewer saw only the rendered frames and the job questions. They had no source code and no design rationale. The frames come from the synthetic WKWebView harness (`ui-harness/run.sh note-retirement`, mode `saved-draft-unreadable`), which drives the production frontend against a stubbed backend. They are not captures of the installed app.

**Question:** When a meeting has a saved AI draft that this install cannot read, can a person tell what the meeting has and what it doesn't show? Does anything suggest the draft was lost?

**First frame: revise.** The line "This meeting has a saved AI draft that this version of Yawn can't show.", followed only by "The transcript and your notes remain available.", read as a dead end. A reader could not tell whether the draft was safe.

**Revised frame: accept.** The detail now reads "The draft is still saved on this Mac. The transcript and your notes remain available." The reviewer found that this answers "is it lost?" without promising a fix that no current version offers, and that it adds nothing misleading. The remaining open question, "how do I get the draft back?", is answered honestly by "can't show".

Not changed, with reasons:
- **Collapsed "Full transcript" row.** It reads as sparse next to the withheld-draft line. Every meeting state shares this row, so this change did not introduce it. The reviewer accepted it as is.
- **"No recording is playing" and the audio caption's size, larger than its heading.** These are pre-existing playback-block styling. The reviewer called them minor and not blocking.

What was removed, combined, demoted or hidden: nothing was removed. The withheld draft replaces what would otherwise be a whole-library failure ("The local library is unavailable"), which hid every meeting.

![Withheld saved draft, dark](../saved-draft-unreadable-2026-10-06/saved-draft-unreadable-dark.png)
![Withheld saved draft, light](../saved-draft-unreadable-2026-10-06/saved-draft-unreadable-light.png)

Checks: `npm run test:ui` (156 pass). `./run.sh note-retirement` passed all five fixtures in dark and light. The new fixture passes only when the withheld-draft note area and the audio controls are present; the other fixtures report both absent. This review does not establish behavior in an installed build.
