# Experience brief — Yawn on Mac and iPad, rethink of the interface and visual style

2026-10-15 · draft for the operator · judged-screen pattern § 2b

The operator asked on 2026-10-15 to rethink the UI, the UX and the visual style
of both Yawn apps with the design skills that own Mac and iPad work. This brief
states what the two apps must do and feel like so three whole-screen concepts
can be drawn and compared across both. It does not change the product brief.

## Why a rethink, and what it does not reopen

Two product changes landed after the last design decisions and neither has
reached the Mac screen as design:

- **2026-09-30.** Automatic note generation was retired. The document pane was
  designed around a generated note with the operator's notes beneath it. With
  the note gone, the pane was patched: transcript retry and audio playback
  sections now sit above the operator's own notes, and a sentence explains
  what is no longer available.
- **2026-10-13 and 10-14.** An iPad in-person mode was decided and its screens
  were selected (Concept C, the time rail). On iPad every stroke stays tied to
  the moment it was written and the transcript sits beside the ink. The Mac
  has no equivalent link, and the two apps share no visual language yet.

The blind cold review of the shipped Mac app
([record](evidence/screen-reviews/desktop-rethink-baseline-2026-10-15-cold.md),
seven of seven frames revise) found: a web page in a native frame (bordered
buttons in the document, a notes card with a resize glyph, link-styled Settings
tabs); the operator's notes fourth or fifth in reading order under repair and
playback controls; 18 explanatory passages on screen, with "stays on this Mac"
said eight times; and the transcribing state read as an unfinished prototype.

What this rethink keeps fixed, because the operator decided it and nothing
since argues against it:

| Fixed | Source |
|---|---|
| The Mac window frame: a meeting list beside a document pane, one Record control in a unified toolbar, Trash last in the sidebar | [Selection ADR, 2026-09-02](design-direction-decision.md), "concept A wins completely" |
| The iPad structure: the page is the screen, the time rail in its left margin is the scrubber, the transcript folds beside the ink, the list is a sidebar hidden while recording, consent is a full-screen step | [Selection record, 2026-10-14](../decisions/ipad-concept-selection-2026-10-14.md) |
| System font, SF Symbols, light and dark, Dynamic Type and large text, reduced motion | Both prior briefs; DESIGN.md |
| Red for recording only; amber for a condition that needs attention; no other color carries meaning | Product brief, "color only for recording or attention" |
| Everything the product brief forbids: accounts, sync, bots, folders, templates, generated notes, sample data | Product brief |

What is open, and what the three concepts answer differently:

1. **What the Mac After view is made of.** The operator's notes and the
   retained transcript are the destination. How do they sit together: notes
   as the document with the transcript disclosed; notes and transcript linked
   by time, the iPad's model; or the transcript as the record with the notes
   as annotations?
2. **What the Mac During canvas is.** A plain page with the recording state
   in the toolbar; a page with marks appearing as lines are typed; or a page
   with the live transcript streaming beside it.
3. **Where secondary actions live.** Rename, retry transcript, play audio,
   export, lock, Trash. Today they are bordered buttons inside the document.
4. **How Settings is composed.** Today a page with link-styled tabs and cards.
5. **The visual language**, and how closely the Mac and iPad apps rhyme.

## The user's situation

One person, one meeting they are in, one private record. On the Mac they are
at a desk in a call with other windows on screen and type. On the iPad they
are at a table in a room, the app is the whole screen, and they write by
hand. The three moments are the same on both:

- **Before.** Record in a few seconds, with the consent check done.
- **During.** Write without looking at the app, know it is still recording,
  stop with one control.
- **After.** Reread what they wrote, and where a detail matters, reach the
  transcript or the audio for that moment. Later, find the meeting again.

## The five job questions, per moment

| Moment | What is happening now | What is next | Who | When and where | What can I do |
|---|---|---|---|---|---|
| Before | Not recording; speech ready or not | Confirm consent (and on Mac, headphones and one operator), choose a keep period, record | Mac: the operator and the call. iPad: the operator and the room | Now, this device | Record; open a past meeting |
| During | Recording, for how long, paused or not (Mac) | Stop | Mac: You and the call's audio. iPad: unlabeled voices | Elapsed time | Write; on iPad erase and undo; on Mac pause; stop |
| After | Finishing, transcript ready, or needs attention | Reread; reach the transcript or audio at a moment | Mac: You and Them by channel. iPad: unlabeled until proven | Meeting date and length | Edit the notes; read, search and copy the transcript; play audio; export; record another |

## Character

Five words a person could disagree with:

- **Plain.** Things are named, not explained. A control says what it does;
  a state says what is true. No sentence in steady state describes the
  product.
- **Settled.** The meeting's identity and the operator's writing surface stay
  where they are when the state changes. Absence, failure and progress are
  modifiers on a stable page, never headlines that replace it.
- **Candid.** Recording, paused, finishing, transcript ready, needs attention:
  each said once, in the brief's words, near its object.
- **Paper-first.** The writing surface is the page and the app is its margin.
  On the Mac the page is typed; on the iPad it is ink. Chrome recedes.
- **Kin.** The two apps are recognizably one product. They differ where the
  hand differs, keyboard against finger, and nowhere else.

## Anti-goals

- A web app in a Mac frame: bordered buttons in the document, cards inside
  panes, link-styled navigation, a textarea with a resize grip.
- A home feed or dashboard: counts, tips, upcoming meetings, cards to browse.
- An AI product: summaries, generated titles, suggested actions, chat.
- A recorder that could be recording without the screen saying so.
- Two products that share a name: a different palette, type, row shape or
  status vocabulary on each device.

## Density targets

- Mac, After, at the default 1080 by 900 window: the operator's notes and at
  least eight transcript turns visible together without a click.
- Mac, During: the page takes at least 80 percent of the document pane, with
  one line of status in the chrome.
- Mac, Settings: the first row of every group visible in one window height.
- iPad: the targets in the [iPad brief](experience-brief-ipad-2026-10-14.md)
  stand (the page at least 85 percent of the screen while recording; page and
  six transcript turns together after; eight meetings in the list).

## Hierarchy

1. The meeting is the stable identity: its title or date, length and status,
   small, at the top of the page, unchanged across states.
2. The operator's own writing is the first thing on the page and the largest
   reading object.
3. The transcript is the second reading object, reachable without leaving the
   page: beside the notes, under them, or linked to them by moment.
4. The recording control is in one place in the chrome and is the only
   element that spends the record color.
5. Everything else, rename, retry, audio tracks, export, lock, Trash, lives in
   the chrome, a menu, or a disclosure, and shows only when it applies.

## Content and microcopy

- One fact, once per screen. "On this Mac" and "stays on this iPad" are said
  in Settings and in the consent step, not on every card.
- Status words are the brief's: preparing, recording, paused, finishing,
  transcript ready, needs attention. One status per screen.
- An action is a verb or a noun, not a sentence: Rename, Retry transcript,
  Play microphone, Export, Move to Trash.
- Generation's retirement is not announced in the meeting view. A saved AI
  draft, where one exists, is labeled as a draft on the block itself and sits
  under the operator's notes.
- A fact is a caption; a problem is a needs-attention state with one next
  action. Never a banner for a fact.
- Mac transcript turns carry You and Them by channel; iPad turns carry no
  speaker until a table recording proves labels.

## Motion and feedback

State changes are short and legible: recording start and stop confirm in the
chrome; the iPad playhead dims and brightens strokes as it moves; a Mac note
linked to a moment highlights its transcript turn when the audio reaches it.
Nothing performs. Reduced motion removes the transitions and keeps the state.

## Platform strategy

The [2026-09-02 Mac brief](experience-brief-2026-09-02.md) and the
[2026-10-14 iPad brief](experience-brief-ipad-2026-10-14.md) stand; their
adopt and decline tables are not repeated. This rethink adds to the Mac table:

**Adopt**

| Convention | Why |
|---|---|
| Native control styling: toolbar items, menus and system push buttons; no bordered web buttons inside the document | The cold review's category read; every Mac comparable keeps actions in the chrome |
| The operator's notes as an edge-to-edge editor in the reading measure, not a card with a resize grip | The notes are the page, not a form field |
| Settings composed as a macOS settings window: sections in a sidebar or toolbar, grouped inset rows, one control per row, at most one line of description | System Settings on macOS 26, device capture 2026-10-15 |
| A secondary-action menu (Manage, or the toolbar ellipsis) for rename, retry, audio tracks, export, lock and Trash | Keeps the page for the record; matches Notes and Bear |
| Playback as one slim transport, not two bordered buttons and a status sentence | Voice Memos and Notes present a recording as one control |

**Decline**

| Convention | Seen in | Why declined |
|---|---|---|
| Link-styled tabs or underlined section navigation | Current Settings | Web convention; no Mac comparable |
| A caption on every card restating provenance or storage | Current meeting view | One fact once |
| A What's New or feature sheet on launch | Voice Memos | No pitch inside the product |
| Full-saturation accent on idle surfaces | Current pills in Settings | Color only for recording or attention |

## Prior art, one question per reference

| Reference | Question asked | What it showed | Adaptation |
|---|---|---|---|
| System Settings, macOS 26 (device capture 2026-10-15, not committed) | How does the platform compose a settings window now? | A sidebar of sections, grouped inset rows, a short heading per group, one control per row, a one-line description only under a toggle | Settings becomes rows and groups; the four explanatory paragraphs per card go |
| Bear (device capture 2026-10-15, not committed; the welcome notes, nothing private) | How does a Mac note app render the note and its actions? | The note edge to edge with a large title; actions as toolbar icons and an info popover; nothing bordered in the document | Rename, info and export move to the chrome |
| Voice Memos, macOS (device capture 2026-10-15; the app had no recordings here, so only its empty state and chrome were seen) | How does Apple place Record and search in a recorder? | Record as one red circle at the foot of the list; search covers titles and transcripts; a feature sheet on first launch | Transcript search belongs in the one search field; the feature sheet is the anti-goal |
| Apple Notes on iPad (Apple Support, cited in the [build plan](ipad-build-plan-2026-10-15.md)) | What is the shipped pattern for writing while a live transcript is made? | Recording and transcribing together, notes taken meanwhile, transcript available after | The Mac During canvas may show the transcript as a count, not as a stream, without losing anything the platform offers |
| Notability (comparables record, 2026-10-13) | Tap a word, hear the moment: can a typed Mac note carry the same link? | On iPad the link is a stroke's time. On the Mac the nearest unit is the line, timed when it is first typed during capture | One concept tests this; it is a new capability the operator would have to decide |
| Yawn's own Concept C frames (2026-10-14) | What does the time link look like when the transcript folds beside the notes? | The nearest turn level with each note; the rest folded into a count | The Mac equivalent, if chosen, uses the same fold |

Mac comparables from the 2026-09-02 record (Notes, Bear, Craft, Agenda,
Granola, Obsidian) were not recaptured; their composition findings stand.

## Object, action and state

The Mac matrix in [DIRECTION.md](../DIRECTION.md) and the iPad matrix in the
iPad brief stand. One object may be added by a concept and is not built:

| Object | Owner | Valid actions | States | Reverse action |
|---|---|---|---|---|
| Note moment (Mac): the time a typed line was first written during capture | The recording | Jump the audio to it; see the transcript turn nearest it | Timed (typed while recording); untimed (typed after Stop); recording released, so no audio to reach | Editing a line keeps its moment; a line typed after Stop has none and says so by position |

Today the Mac stores the operator's note as one text with no time at all
(`apps/desktop/src-tauri/src/operator_note.rs`, `StoredNote`: a schema and the
text). A note moment per line is a product decision and a storage change; a
concept that depends on it says so on the frame.

## Representative states for the concept comparison

Each concept renders these on both platforms, light and dark. Mac frames are
1080 by 900 at 2x; iPad frames are 1180 by 820 points (iPad Air 11-inch,
landscape) at 2x. All meeting content is invented and labeled so on the frame.

Mac:

1. **After, transcript ready.** The operator's typed notes from the meeting,
   the full transcript, audio retained for 7 days. The state where the
   concepts diverge most.
2. **During.** Four minutes in, notes being typed, recording live in the
   toolbar.
3. **Needs attention.** The recording was interrupted; the audio up to that
   point is kept; one next action.
4. **Settings.**
5. **First run.** No meetings; what the window opens to.

iPad (Concept C's structure in each concept's language):

6. **During.** Four minutes in, large finger handwriting, the rail marking
   each line.
7. **After.** Replay paused mid-meeting, later strokes dimmed, one stroke
   tapped, the transcript folded beside the ink.
8. **Before.** The consent step.
9. **Meeting list.** The sidebar open beside a page, four meetings.

Pruned from the pattern's canonical set, with the reason:

- Changed by source: nothing outside the device changes a meeting.
- Upcoming: there is no calendar.
- Largest text and increased contrast: checked on the selected concept.
- Dense: the long transcript in state 1 and the long page in state 6 are the
  dense cases.
- Completed with undo: Trash and restore are unchanged by this rethink.
- iPad portrait and iPad needs-attention: unchanged from the 2026-10-14
  concepts; checked on the selected concept.

The current shipped Mac app is the control: its captures sit in the
comparison beside the three concepts for the states where they exist.

## Surfaces

- Mac: Home, first run
- Mac: Meeting, after
- Mac: During capture
- Mac: Needs attention
- Mac: Settings
- iPad: Before
- iPad: During
- iPad: After
- iPad: Meeting list

## Provenance

The operator's request, 2026-10-15. Product brief (reset 2026-08-10, amended
2026-09-30 and 2026-10-13). Blind cold review of the shipped Mac app,
2026-10-15. Selection records of 2026-09-02 (Mac) and 2026-10-14 (iPad).
Device captures of System Settings, Bear and Voice Memos on this Mac,
2026-10-15, kept in the session scratchpad because two of them show the
operator's account and network names. The Mac operator-note storage, read
2026-10-15. The adopt and decline rows and the open questions are the author's
proposal; the operator's choice among the concepts is what makes them
direction.
