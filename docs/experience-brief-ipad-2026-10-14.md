# Experience brief — Yawn for iPad, in-person mode

2026-10-14 · draft for the operator · judged-screen pattern § 2b

This brief turns the product brief's
[iPad in-person mode](product-brief.md#ipad-in-person-mode--decided-2026-10-13)
into what each screen must do, so three screen concepts can be drawn and
compared. It does not change the product brief. Its evidence is the operator's
own use of the research spike (`spike/ipad-ink/`) on an M1 iPad Air, not other
apps' screens. The Mobbin record
(`docs/evidence/comparables-ipad-ink-audio-2026-10-13.md`) is a minor input.

## The user's situation

One person at a table with other people, the iPad flat or propped in front of
them, writing with a finger. Unlike the Mac, the app is the whole screen and
the only thing the operator looks at. Others at the table can see it.

- **Before.** People are sitting down. The operator says out loud that they
  are recording, gets agreement, and starts. The screen is a step on the way,
  not a place to stay.
- **During.** The conversation is on. The operator writes large, scrolls on,
  and keeps writing. They glance at the screen to confirm it is still
  recording, and stop with one control.
- **After.** The operator rereads what they wrote. Where a note is unclear,
  they tap it and hear what was being said, with the transcript beside it.
  They can still add to the page or erase. Later, they find a past meeting by
  when it was, what the page looks like, or words in its transcript.

## The five job questions, per moment

| Moment | What is happening now | What is next | Who | When and where | What can I do |
|---|---|---|---|---|---|
| Before | Not recording; speech model ready or not | Confirm consent, choose keep period, record | The operator and the people in the room | Now, this iPad | Record; open a past meeting |
| During | Recording, for how long | Stop | Unlabeled voices in the room | Elapsed time | Write; scroll; erase; undo; stop |
| After | Transcribing, transcript ready, or needs attention | Reread; tap a note to hear it | Unlabeled until speaker labels are proven | Meeting date and length | Replay; tap a stroke to jump; edit the page; read, search, and copy the transcript; export to Files; start a new meeting |

## What the spike already settled

These came from the operator using the spike, and each one changed the build:

- The page must grow. A fixed box filled after one large note (PR #116).
- A finger must write and two fingers scroll. Pencil-only drawing left a
  finger unable to write (PR #117; the operator writes with a finger).
- Tapping a stroke to jump the audio works, and dimming strokes written after
  the playhead reads as replay (spike README, "Known limits").

## What the spike does not do yet, and production needs

Checked in spike source on 2026-10-14, not on the device:

- **Keep recording when the screen would sleep.** The spike neither keeps the
  screen awake nor declares background audio, and it does not handle audio
  interruptions such as a phone call. If the iPad auto-locks mid-meeting, the
  recording probably stops. A meeting must never end silently.
- **Erase and undo.** Neither exists; the spike matches stroke records to the
  drawing by position, so either one would break the timing (roadmap step 2).
- **A list of past meetings.** The spike holds one meeting at a time.
- **A transcript before Stop.** The spike transcribes only after Stop.
- **A resting hand.** With finger drawing on, a palm resting on the glass may
  leave marks. Test on the device before choosing a fix.

## Hierarchy

1. The page is the loudest thing on screen. Never the app.
2. Recording state and Stop sit in one place, always visible, and are the only
   things that use the recording color.
3. Nothing covers the strokes the audio belongs to. Replay controls and the
   transcript sit beside or above the page, never on top of it.
4. Consent and the keep period are one short step before recording, not a form
   on the page.
5. Status is small and near its object. A full-width message is only for a
   condition that endangers the recording.

## Content strategy

- Status words are the product brief's: preparing, recording, finishing
  (which covers transcribing after Stop), transcript ready, needs attention.
  One status per screen.
- The consent line stays the spike's: "Everyone who can be heard agreed to this
  recording." Keep periods read 1 day, 7 days, 30 days.
- Say "Stays on this iPad" once, in the before step. Not on every screen.
- The transcript says "Speaker" nowhere until labels are proven. Turns are
  shown as text with times.
- No pitch, quotes, promotions, or sample meetings inside the app.
- A meeting has no typed title. Its row shows the date, time, length, and a
  picture of the top of its page, because the notes are handwriting.

## Motion and feedback

Short and functional. Recording start and stop confirm in the status area. The
playhead moves through strokes as they dim and brighten. Nothing performs.

## Platform strategy

The surface is an iPadOS document app used flat on a table, in landscape or
portrait, with a finger.

**Adopt**

| Convention | Why |
|---|---|
| A sidebar with the meeting list beside the page, hidden while recording | The iPadOS standard for list and document; the product brief keeps the list as a way to reopen meetings, not the home screen's subject |
| A slim bar above the page with the recording state, elapsed time, and Stop | One obvious way to stop (product brief); keeps the page clear |
| One finger writes, two fingers scroll, and the Pencil-only setting is honored | Product brief, iPad section; proven in PR #117 |
| Keep the screen awake while recording, record in the background, and recover from interruptions with a plain message | A meeting must never end silently; the spike does none of these |
| Pen, eraser, and undo only, with a thicker default pen | Finger-first and large handwriting (product brief); a full tool palette is not the job |
| System font, system colors, light and dark, Dynamic Type for chrome | Quiet and native, like the Mac app |
| Landscape and portrait, and resizing in Split View and Stage Manager | The iPad lies flat in either orientation at a table |
| Export to Files as the only transfer off the iPad; copying transcript text stays available | Product brief: no sync, no upload, but the transcript can be copied |

**Decline**

| Convention | Seen in | Why declined |
|---|---|---|
| Notebooks, folders, templates, paper styles | Goodnotes, Notability | Product brief anti-goal: no folders or templates |
| A home screen with cards, quotes, or promotions | Notability (operator's screenshot, 2026-10-13) | No pitch inside the product |
| An audio sheet over the page | Goodnotes | It covers the strokes the audio belongs to |
| An AI summary tab | Goodnotes | Product brief: no note generation |
| Speaker names or numbers | Otter | Not until a table recording proves them (product brief) |
| The full PencilKit tool palette | Apple Notes, Goodnotes | More than a finger-first meeting page needs; colors and rulers add choices without helping the job |

**Undecided, for the concepts to test**

- Where the transcript lives after Stop: a panel beside the page, a strip
  below it, or collapsed by default but always openable in the meeting view.
  The full transcript stays available in the meeting view (product brief).
- Where consent happens: a sheet before recording, or inline in the bar above
  the page.
- Where replay controls sit: the bar above the page, a scrubber along the page
  edge, or in the sidebar.
- What the app opens to: the meeting list, the last meeting's page, or a blank
  page ready to record.

## Representative states for the concept comparison (§ 2c)

Each of the three concepts renders these four, in landscape, light and dark;
state 2 also in portrait:

1. **Before:** three past meetings in the list, nothing recording, the consent
   step open.
2. **During:** four minutes in, the page scrolled past its first screen, large
   finger handwriting.
3. **After:** replay paused mid-meeting, later strokes dimmed, one stroke just
   tapped, the transcript turn at that moment visible.
4. **Needs attention:** the recording was interrupted (the screen locked or a
   call came in), the audio up to that point kept, and one clear next action.

## Provenance

Product brief, iPad section (2026-10-13). Operator feedback while using the
spike on 2026-10-12 and 2026-10-13: the fixed page and the finger that could not
write. Spike source read on 2026-10-14 for screen-awake, background audio,
interruption, tool, and undo handling (none present). Operator's Notability
screenshot from 2026-10-13. Mobbin record from 2026-10-13. The adopt and decline
tables are the author's proposal; the operator's choice among the concepts is
what makes them direction.
