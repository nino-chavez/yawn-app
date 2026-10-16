# Yawn on Mac and iPad — three concepts, 2026-10-15

Three whole-screen concepts for the family rethink. They are drawn from
[the experience brief](../../../experience-brief-family-2026-10-15.md) and
rendered on its nine representative states, both platforms, both appearances.
They run in the harness at `apps/desktop/ui-harness/family-concepts/`. The 54
frames here are the record. `compare.html` lays them out by state beside the
current shipped Mac app where a capture of that state exists.

**Every meeting, note, time and transcript line in these concepts is invented.**
None of it comes from a real recording. Each frame says so in its corner.

## What stays the same in all three

- The Mac window frame (Concept A, selected 2026-09-02): a meeting list beside
  a document pane, one Record control in a unified toolbar, Trash last.
- The iPad structure (Concept C, selected 2026-10-14): the page is the screen.
  The time rail in its left margin is the scrubber. The transcript folds
  beside the ink. The list is a sidebar, hidden while recording. Consent is a
  full-screen step.
- No sentence explains the product in steady state. The storage fact is said
  once, in Settings and in the consent step. Status words are the brief's.
- Secondary actions (rename, retry, export, lock, Trash) live in the toolbar
  menu, not as buttons inside the document.
- Red is spent on recording only; amber on a condition that needs attention.

## How the three differ

| Question | 1 · Notebook | 2 · Rail | 3 · Record |
|---|---|---|---|
| The Mac After view | The notes are the document. The transcript is a labeled section under them | The notes carry the moment each line was typed. The nearest turn sits level with each note; the rest fold into a count | The transcript is the document. The notes are a pinned strip above it |
| The scrubber | A slim transport between notes and transcript | The rail down the page's left margin, as on the iPad | A full-width timeline with a tick per turn |
| The Mac During canvas | The page and the lines typed so far, with a collapsed live-transcript count at the foot | The page, with a rail mark appearing as each line is typed | The pinned notes, with the live transcript streaming under them |
| Settings | Preferences-style window, toolbar tabs, one section at a time | System Settings-style sidebar, one section at a time | One dense form, every group visible |
| Visual language | System: system grays, the system accent, a filled selection row. Nearest neighbour: Apple Notes | Paper: a warm sheet, graphite chrome, an ink locator on the selected row, blue ink for the moment link. The notes are the largest type | Instrument: cool neutrals, monospaced times, dense hairline rows, a filled navy selection |
| iPad After split | Page 60, transcript 40 | Page 60, transcript 40, on a paper panel | Page 50, transcript 50, monospaced times |
| What rhymes across devices | The language only; the Mac has no time link | The rail itself: one structural idea on both devices | The transcript-as-record stance, which gives the iPad page less room after the meeting |
| What it needs that is not built | Nothing | A moment per typed line on the Mac. The product stores one note text with no time | Nothing |

## States

1. Mac, after: transcript ready, eight typed notes, 22 turns, audio kept
2. Mac, during: 4:12 in, three lines typed
3. Mac, needs attention: the recording was interrupted at 6:41
4. Mac, Settings
5. Mac, first run: no meetings yet
6. iPad, during: 4:12 in, seven lines of handwriting
7. iPad, after: replay paused at 11:30, later strokes dimmed, one stroke tapped
8. iPad, before: the consent step
9. iPad, meeting list open beside a page

Frames are named `<concept>-<state>-<appearance>.png`. Mac frames are
2160 by 1800 (1080 by 900 at 2x). iPad frames are 2360 by 1640 (1180 by 820
points at 2x).

## Two reads of the frames

The author read the frames first. A blind reviewer then judged 21 of them
with no rationale
([record](../../../evidence/screen-reviews/family-concepts-2026-10-15-blind.md)).
Three of its findings were defects of the comps, fixed and recaptured after
the review; the record's last section lists each claim and its check.

| State | 1 · Notebook | 2 · Rail | 3 · Record |
|---|---|---|---|
| 1 Mac after | Title, then the notes as a plain document, then the transcript. All five questions answered. Nothing links a note to a moment | The notes are the largest type, each with its moment. The nearest turn sits beside each one. The only Mac frame that answers "where did this note come from" without a click. The fold hides the rest behind a count | The timeline and the transcript table lead. The notes are a 13px strip. Strongest for checking a time, weakest for rereading |
| 2 Mac during | The calmest page: three lines, a caret, status in the toolbar | The same calm, with the rail forming as you type | The live transcript streams under the notes. The most distracting; the brief asks for typing without looking |
| 3 Mac attention | A callout with two actions; complete but generic | The rail ends at 6:41 with "Recording stopped here". The blind review's pick for this state | The timeline carries the stop mark. The most complete status read |
| 4 Mac Settings | Toolbar tabs, one group per tab. The blind review's pick: the standard Mac layout | Sidebar sections, one at a time | Every group on one screen. The only one that meets the brief's density target |
| 5 Mac first run | A centered sentence and the Record control | The same, with the empty rail hinting at the page | The same, in the instrument type |
| 6 iPad during | White page, blue pen. Reads as Apple Notes | Warm page, blue-ink pen. Reads as a sheet of paper | White page, monospaced rail times. Reads as a lab notebook |
| 7 iPad after | One blue carries the chip, the tapped stroke and the turn. The link is seen without reading | Now the same link in blue ink on paper (fixed after the review) | Page 50, transcript 50. More turns, less ink |
| 8 iPad before | Card on gray | Card on paper | Squared card on gray. All three do the job |
| 9 iPad list | Filled-blue selected row | Quiet row with an ink locator | Filled-navy selected row |

Where the two reads agree: Record is third on both platforms. Rail is the
only concept that reads as one product on two devices. Notebook is the most
native Mac app state by state.

Where they differ: the blind reviewer ranks Notebook first on both platforms
and Rail second, because Notebook wins most single states and needs nothing
new. Its own red-team argument against Notebook is the author's reason for
Rail: on the Mac, no note line carries a time or a link, so checking one
detail takes a scroll and a hunt, while on the iPad every line does.

## Recommendation (the author's; the selection is the operator's)

The choice is a product call, not a taste call: **does the Mac carry the time
link?**

- If yes: **Concept 2, Rail.** It needs one capability the Mac does not have,
  the moment a typed line was first written during capture, stored with the
  operator note, with a merge rule for continuous typing. Grafts already
  folded in or named: Notebook's colored moment link (now blue ink on paper);
  Record's single-form Settings; Record's tick-per-turn on the scrubber.
- If no: **Concept 1, Notebook, with Rail's paper language grafted** (the
  warm page, the ink locator, the operator's words at 16px). Its structure
  needs nothing new, and the blind review ranks it first state by state.

Why the author leans to Rail: the reason the iPad direction was chosen is the
visible link between a note and the talk around it, and Rail is the only
concept that gives the Mac the same link. What it costs: the fold, and a
typed line's moment being fuzzier than a stroke's.

## What the frames cannot show

- **Handwriting is a font.** Bradley Hand stands in for finger strokes, as in
  the 2026-10-14 SwiftUI concepts. Real ink comes from the device build of
  the selected concept.
- **Fold counts assume a real talking pace.** "+ 20 turns" uses one turn
  every ten seconds, the pace the iPad test recording measured. The invented
  transcript itself has 22 turns over 24 minutes.
- **The Mac note moments are invented.** The product stores one note text
  with no time per line.
- **The iPad frames are HTML, not SwiftUI.** System materials, blur and the
  window resize handle are absent. The selected concept is accepted on its
  SwiftUI build, cold-reviewed on the device.
- **Nothing is live.** Scrolling, the playhead moving, a tap on a stroke, a
  fold opening, large text and increased contrast are checked on the selected
  concept.
- **Settings glyphs** are gray squares standing in for SF Symbols.

## Selection

Pending. The operator chooses; the record is
[decisions/family-direction-2026-10-15.md](../../../../decisions/family-direction-2026-10-15.md).
