# Yawn on Mac and iPad: the family design direction

2026-10-15 · DECIDED 2026-10-15 · judged-screen pattern § 2c

Status: **decided.** Concept 2, Rail, chosen by Nino Chavez, the operator, on
2026-10-15. The selection and its conditions are in the Selection section;
the comparison that led to it stands as written.

## The question

The operator asked on 2026-10-15 to rethink the UI, UX and visual style of
both Yawn apps. Two product changes had reached neither screen as design. Note
generation was retired on 2026-09-30, which emptied the Mac document pane's
center and left repair and playback controls above the operator's notes. The
iPad in-person mode was decided on 2026-10-13 with a time rail that ties every
stroke to its moment, which the Mac has no equivalent of. The blind baseline
review of the shipped Mac app returned seven of seven frames revise
([record](../docs/evidence/screen-reviews/desktop-rethink-baseline-2026-10-15-cold.md)):
a web page in a native frame, the operator's notes fourth in reading order,
18 explanatory passages on screen.

Fixed through this rethink, because the operator decided them and nothing
since argues against them: the Mac window frame (Concept A, 2026-09-02) and
the iPad structure (Concept C, 2026-10-14). Open: what the Mac After view is
made of, the Mac During canvas, where secondary actions live, Settings, and
the visual language on both devices.

## The three concepts

Frames and the state-by-state comparison:
[docs/design/concepts/family-2026-10-15](../docs/design/concepts/family-2026-10-15/README.md)
(`compare.html` lays all 54 out beside the current app).

| | 1 · Notebook | 2 · Rail | 3 · Record |
|---|---|---|---|
| The Mac After view | Notes as the document, transcript beneath, one slim transport | Notes with the moment each line was typed on a rail; the nearest turn level with each; the rest folded | The transcript as the document; the notes a pinned strip; a tick-per-turn timeline |
| Language | System (Apple Notes) | Paper (a warm sheet, graphite chrome, blue ink for the moment link) | Instrument (cool neutrals, monospaced times, dense rows) |
| What it needs that is not built | Nothing | A moment per typed line on the Mac | Nothing |
| Risk to test | The Mac and iPad share a look but not an idea | A typed line's moment is fuzzier than a stroke's; continuous typing floods the rail | The notes read as a sidebar of the transcript; the live stream invites looking at the app |

## Comparison across the states

Two reads: the author's, and a blind reviewer's with no rationale
([record](../docs/evidence/screen-reviews/family-concepts-2026-10-15-blind.md)).
Three findings of the blind review were defects of the comps, fixed and
recaptured after it; the record's last section lists each claim, its check,
and what changed.

| State | 1 · Notebook | 2 · Rail | 3 · Record |
|---|---|---|---|
| Mac after | Notes then transcript, unlinked; all five questions answered. Blind pick | The only Mac frame that answers "where did this note come from" without a click; the notes are the largest type; the fold hides the rest | The transcript dominates; the notes are a 13px strip; strongest for checking a time, weakest for rereading |
| Mac during | Calm page. Blind pick | Calm page, the rail forming as you type | The live transcript streams; the most distracting |
| Mac attention | A generic callout | The rail ends at the stop. Blind pick | The timeline carries the stop; the most complete status read |
| Mac Settings | One section per tab. Blind pick: the standard Mac layout | One section per sidebar item | Every group visible; the only one that meets the density target |
| iPad during and after | Reads as Apple Notes; one blue carries the moment link. Blind pick | Reads as a sheet of paper; the moment link now in blue ink | Reads as a lab notebook; the page gives up width after the meeting |
| Kin | Palette only | The rail on both devices. Blind pick for the family read | A stance the iPad's own rule contradicts after the meeting |

Blind ranking, both platforms: 1 Notebook, 2 Rail, 3 Record. Its red-team
argument against Notebook: on the Mac no note line carries a time or a link,
so checking one detail takes a scroll and a hunt, while on the iPad every
line does. "That gap makes Notebook a single-device winner and a two-device
mismatch."

## Recommendation

The choice is a product call: **does the Mac carry the time link?**

- If yes: **Concept 2, Rail**, on the condition that the operator decides for
  the one capability it needs, a moment per typed line on the Mac, stored
  with the operator note, with a merge rule for continuous typing.
- If no: **Concept 1, Notebook, with Rail's paper language grafted**, since
  its structure needs nothing new and the blind review ranks it first state
  by state.

Grafts, folded in by hand: from Notebook into Rail, the colored moment link
(applied as blue ink on paper after the blind review); from Record into
either, the single-form Settings and the tick-per-turn on the scrubber.

What Rail declines, and why: Notebook's unlinked notes and transcript, because
the reason the iPad direction was chosen is the visible link; Record's
transcript-first document, because the brief names the operator's own writing
as the first thing on the page, and a streaming transcript during capture
invites looking at the app.

## Selection

Concept: **2, Rail.** Chosen by Nino Chavez, the operator, on 2026-10-15, in
these words: "Select Concept 2 Rail. The Mac gets a moment per typed line;
amend the brief and storage before the Mac wave."

What this decides:

- The Mac carries the time link. Each line the operator types during capture
  keeps the moment it was first typed; the transcript turn nearest each note
  sits level with it after the meeting, and the rest fold into a count. The
  rail down the page's left margin is the scrubber on both devices.
- The paper language is the family's visual system: a warm sheet, graphite
  chrome, an ink locator on the selected row, blue ink for the moment link,
  the operator's words as the largest type, red for recording only.
- The grafts named above are part of the selection: Notebook's colored
  moment link (already in the Rail frames), Record's single-form Settings,
  and Record's tick-per-turn on the scrubber.
- Order of work, as the operator set it: the product-brief amendment and the
  storage change for the note moment come before the Mac implementation
  wave.

## Consequences, once selected

- `DIRECTION.md` `design_intent` returns to `refit` pointing at this record.
  `DESIGN.md` is rewritten from the selected concept's tokens and rules for
  the Mac, with a matching section for the iPad app (`apps/ipad/`).
- If Rail: a product-brief amendment for the note moment, and a storage change
  in `apps/desktop/src-tauri/src/operator_note.rs` before the Mac
  implementation wave.
- The Mac implementation wave rebuilds the document pane, the During canvas,
  Settings and the control styling to the selected concept. The iPad build
  plan's slices continue on the selected language.
- The rejected concepts stay in `apps/desktop/ui-harness/family-concepts/`
  with their frames.

## What would change this

- The operator declines a per-line moment on the Mac (falls back to Notebook
  with the paper graft).
- A real typed-note session shows the rail unreadable under continuous
  typing, with no merge rule that keeps the moments honest.
- A device build of the selected language fails its cold review on the iPad
  for a reason no graft repairs.

## Provenance

The operator's request, 2026-10-15. Experience brief, 2026-10-15. Blind
baseline review of the shipped Mac app, 2026-10-15. Concept frames, 54, from
the harness on 2026-10-15, 16 of them recaptured after the blind review's
findings. Blind comparison review, 2026-10-15. Prior selections of 2026-09-02
and 2026-10-14.
