# Yawn for iPad: build on the time rail, with three changes

2026-10-14 · chosen by Nino Chavez, the operator · judged-screen pattern § 2c

## Decision

Build the iPad in-person mode on **Concept C, Time rail**, with three changes.
The operator preferred C, asked to be challenged on it, and accepted the three
changes below in the same session.

C is the only concept that shows the link between a note and the talk around
it without a tap. A rail down the page's left edge marks when each line was
written and is the scrubber; after the meeting, the transcript sits beside the
ink written at the same moment. That link is the product's reason to exist on
iPad, so the selected direction is the one that makes it visible.

## The three changes

1. **Fold the transcript between notes.** At a real talking pace the
   time-aligned column falls minutes behind the ink beside it: the concept's
   invented transcript runs at about 6 words a minute, and the iPad test
   recording ran at about 153. Between two notes, show only the turn spoken
   nearest each note, and fold the rest into a count that opens them. The full
   transcript stays one tap away (product brief). Evidence:
   [the real-pace frame](../docs/design/concepts/ipad-2026-10-14/C-3-light-real-pace.jpg).
2. **Open to a page, not the meeting list.** The product brief says the list
   "is not the home-screen subject", and that rule applies on iPad unchanged.
   The app opens to the most recent meeting's page, or on first run to a blank
   page with Record. The meeting list moves into a sidebar beside the page,
   hidden while recording.
3. **Test the rail against real writing before building it.** The rail assumes
   the operator writes top to bottom in time order. The two-person table
   recording keeps each stroke's position and time; plot one against the other.
   If the operator often goes back up to add to an earlier note, the rail
   cannot live in the page margin, and this decision is reopened.

Which page the app opens to (the most recent meeting, or a blank page on first
run) is the agent's reading of change 2; the operator approved "open to a page,
with the list one tap away".

## What C keeps

- The rail as the scrubber, with a mark and a time for each line.
- The transcript aligned with the ink, folded as in change 1.
- Consent as a full-screen step of its own. Others at the table can see the
  iPad, so this is the clearest signal that recording is about to start.
- A narrow tool column on the right while recording, and the tools in the top
  bar afterwards.
- Strokes added after Stop below the rail's end, under "After the meeting".
- A full-transcript control in the meeting view.

## Taken from the other concepts

- **From A, Notebook:** the meeting list as a sidebar beside the page, hidden
  while recording. It replaces C's full-screen list. C's list rows (the date,
  a small rail of when notes were written, the first lines) move into it.
- **From B, Pad:** a blank page with Record as the first-run screen.

## Left out, and why

- **A's transcript column and toolbar replay controls.** The aligned, folded
  transcript and the rail do both jobs and keep the time link visible.
- **B's transcript ribbon.** It shows about five turns at once and has the
  same scaling problem without the alignment.
- **B's inline consent card.** It is lighter, but a full-screen step is a
  clearer signal to the room.
- **A's blue ink and B's margin mark for strokes added after Stop.** C's
  "After the meeting" divider says the same thing in words.
- **B's grid of page pictures.** Pages of handwriting differ little at
  thumbnail size; the date and the small rail identify a meeting faster.

## Change 3, closed 2026-10-15

The operator writes top to bottom. The table recording agrees, though thinly:
its 11 strokes formed three rows, each lower and later than the last, all in
the final 14 s ([result](../docs/evidence/ipad-table-test-2026-10-14.json)).
The rail stays in the page's left margin. The build follows the
[iPad build plan](../docs/ipad-build-plan-2026-10-15.md).

## What would change this

- The table recording shows the operator's writing does not run top to bottom.
- A folded transcript, tried on a real meeting, hides the line the operator
  needed.
- The table recording shows speaker labels are reliable, and the aligned column
  has no room for them.

## Order of work

1. The recorder keeps running through screen lock and says plainly when an
   interruption stops it (in progress).
2. Erase and undo, with a stable identity for each stroke, so the rail's times
   stay attached to the right lines (in progress).
3. The operator's two-person table recording.
4. C's meeting screen with the three changes, then a cold review of iPad
   captures.

## Sources

- Concepts, frames and their limits: [docs/design/concepts/ipad-2026-10-14](../docs/design/concepts/ipad-2026-10-14/README.md).
- Experience brief: [docs/experience-brief-ipad-2026-10-14.md](../docs/experience-brief-ipad-2026-10-14.md).
- Product brief, "The product, from first principles" and "iPad in-person mode".
- iPad test recording, 2026-10-13: 79 words in 30.9 seconds.
