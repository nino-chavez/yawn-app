# Yawn for iPad — three concepts, 2026-10-14

Three whole-screen concepts for the iPad in-person mode, drawn from
[the experience brief](../../../experience-brief-ipad-2026-10-14.md) and rendered
on its six representative states. They run live in the **Yawn Concepts** app
(`spike/ipad-concepts/`), where a finger writes on every page; the frames here
are the record.

**Every meeting, note, time and transcript line in these concepts is invented.**
None of it comes from a real recording. The app labels it the same way.

## How the three differ

Each row answers the brief's open questions differently. Where two concepts
share an answer, the row says so.

| Question | A · Notebook | B · Pad | C · Time rail |
|---|---|---|---|
| What the app opens to | The meeting list beside the last meeting's page | A blank page, ready to record | A full-screen list of meetings by day |
| Where the meeting list lives | A sidebar beside the page, hidden while recording | A grid of page pictures behind a Meetings button | It is the home screen; a meeting opens over it |
| Where consent happens | A sheet before recording | Inline: the Record button opens into a small card at the top | A full-screen step of its own |
| Where the transcript lives after Stop | A column on the right, a list you scroll | A strip docked under the page, which shrinks to make room | Beside the page, each turn level with the ink written at that moment |
| Where replay controls sit | The toolbar | Along the top of the transcript strip | The rail on the page's left edge is the scrubber |
| How a stroke added after Stop looks | Blue ink | A small dashed mark in the margin | Below the rail's end, under "After the meeting" |
| Where pen, eraser and undo live | The toolbar, trailing | The top bar, beside the Record button | A narrow column on the right while recording; the top bar after |

## Rules every concept keeps

- The full transcript can always be opened in the meeting view.
- Nothing covers the strokes.
- Recording state and Stop are always visible while recording.
- No speaker labels.
- Consent and the keep period come before recording.
- Status words: recording, finishing, transcript ready, needs attention.

## States

1. Before: three past meetings, the consent step open
2. During: four minutes in, the page scrolled past its first screen
3. After: replay paused at 11:30, later strokes dimmed, one stroke tapped
4. Needs attention: the recording stopped when the iPad locked
5. First run: no meetings yet
6. Meeting list: today's meeting and three earlier ones

## Frames

Frames are named `<concept>-<state>-<appearance>.jpg`.

- **Landscape** frames come from the operator's iPad (iPad Air, 5th
  generation, iPadOS 27), captured over the cable with `devicectl`.
- **Portrait** frames (`-portrait`) come from the iPad Air 11-inch (M4)
  simulator, which could not be rotated here.
- **Real-pace** frames (`C-3-*-real-pace.jpg`) show Concept C after the meeting
  with about 140 transcript turns instead of 16. Run the app with `-dense 1` to
  see them.

## What the frames cannot show

- **The transcript is far too sparse.** The invented transcript has 144 words
  over 23 minutes, about 6 a minute. The iPad test recording ran at about 153
  words a minute. At a real pace, Concept C's time-aligned column falls
  minutes behind the ink beside it (see the real-pace frames). A's list and
  B's ribbon just scroll.
- **The notes run top to bottom in time order.** A real page may not: going
  back up to add to an earlier note puts a later moment higher on the page.
- **During fits one landscape screen.** Four minutes of notes at this
  handwriting size fit, so the landscape frames of state 2 are not scrolled.

## Selection

Pending the operator's choice. The selection record will name who chose, what
was taken from the other concepts, and what was left out.
