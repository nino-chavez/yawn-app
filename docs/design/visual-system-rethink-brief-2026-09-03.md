# Yawn visual-system rethink — Concept A stays, the finish changes

Status: active comparison, second wave added 2026-09-03. This brief replaces the premise in
`refit-brief-2026-09-03.md` that the design system was sound and only its
application had drifted.

## The decision this comparison must enable

Choose a visual treatment that makes Yawn feel like a finished private Mac
utility in sparse, dense, and failure states. Concept A's structure stays. The
type system, neutral surfaces, toolbar composition, control hierarchy, and
document rhythm are open.

The person using Yawn knows how to use a Mac. They need to start recording,
read a finished note, add their own reminders, and inspect transcript evidence
without learning a new workspace.

## The evidence that reopened the system

The 2026-09-03 refit migrated CSS onto one spacing and type-token system. The
installed reading pane remained pixel-identical. The pane already matched
`DESIGN.md`'s 22/15/13 type budget, 62-character measure, and stated spacing.
Conformance did not produce quality.

The operator's screen names the defects more clearly than the source:

- A long meeting title, search, disabled Record control, and a full sentence
  compete in one toolbar row.
- The bright selected sidebar row is louder than the meeting document.
- Rename and Manage sit beside the title with more presence than reading or
  evidence actions.
- The empty operator-notes editor becomes a large invisible dead zone.
- The document is pinned to the top of a large, flat charcoal field.
- Body text, captions, separators, and disabled controls lose contrast in dark
  appearance.

## Character

The selected treatment should feel:

- **Private.** This is one person's local record, not a team workspace.
- **Assured.** Controls and hierarchy look deliberate, not merely present.
- **Readable.** The meeting note carries the window at ordinary Mac distance.
- **Quiet.** Restraint comes from proportion and material, not faint text.
- **Native.** The window behaves and composes like a Mac utility without
  counterfeiting AppKit controls in CSS.

It must not feel like:

- A web article placed inside a desktop window.
- An AI chat product, dashboard, or marketing page.
- A blank canvas whose emptiness is mistaken for calm.
- An administration surface led by Rename, Manage, or destructive actions.
- A generic monochrome prototype with the system accent added afterward.

## Structure lock

Every treatment renders the same semantic structure and copy:

- Unified toolbar with sidebar toggle, document title, title search, and one
  Record control.
- Day-grouped meeting list beside the document, with Trash last.
- Meeting document with title, metadata, note state, generated note, operator
  notes, and transcript disclosure.
- Transcript source in a conditional right inspector.
- Needs-attention state with the list still available.
- Existing keyboard, focus, ARIA, and reduced-motion behavior.

The treatments may change visual placement inside those regions. They may not
replace the sidebar with a popover, merge the document into a stream, add a
dashboard, or invent new product actions.

## The six treatments

The first wave established three distinct starting points. Its blind review
ranked Precision Utility first, but the operator did not select it and later
leaned toward Private Notebook without considering it resolved. The second wave
therefore tests which part of Private Notebook is carrying that preference.

### Native editorial

The note is the most carefully typeset object in the window. Mac chrome stays
compact and neutral. Strong title/body rhythm, disciplined measure, and subtle
surface layering do the work. The risk is becoming a reading app that feels too
precious during capture.

### Private notebook

The document feels personal and writable without becoming skeuomorphic. The
operator-notes area is immediately legible as their space. Neutral colors may
run warmer than the system default while record, attention, and evidence keep
their single semantic jobs. The risk is looking lifestyle-oriented or soft.

### Precision utility

The surface is compact, crisp, and operational. Information density is higher,
toolbar geometry is stable, and every control has an obvious rank. Depth comes
from pane relationships and separators rather than decoration. The risk is
recreating the cold, generic utility the current screen already approaches.

### Tonal Ledger

Private Notebook's contained reading object moves into a low-chroma violet
field with system sans typography. The treatment tests whether the useful
signal was privacy and containment rather than serif or paper styling. Its risk
is becoming a large card inside a desktop window.

### Quiet Folio

Serif remains inside the note, while the window chrome moves to neutral paper
and graphite. The operator-notes field becomes a ruled writing area rather than
a nested card. The treatment tests whether Yawn needs an editorial reading
voice without a themed notebook palette. Its risk is still feeling like a
document editor rather than a meeting utility.

### Marquee Document

The meeting becomes a full-bleed dominant object. Supporting chrome recedes,
and low-chroma violet establishes section and evidence hierarchy. The treatment
translates Minder Marquee's strongest principle without copying its phone
layout, components, or identity. Its risk is making long generated titles too
loud and the note less personal.

These are priors, not specifications. Each artist must make a coherent visual
argument and may deviate when the rendered state supports it. The structure
lock and product contract do not move.

## Convergence and finalist iteration

The operator retained Tonal Ledger and Marquee Document and rejected Native
Editorial, Private Notebook, Precision Utility, and Quiet Folio. Rejected
treatments remain in the harness for archaeology but are removed from the
active comparison.

The finalist pass makes one bounded correction to each surviving direction:

- **Tonal Ledger — Canvas** tests whether the center pane can provide privacy
  and focus without a tall floating document card.
- **Marquee Document — Quiet** tests whether the full-bleed hierarchy survives
  with a smaller title and sentence-case section headings.

The previous-round Tonal Ledger and Marquee Document remain visible directly
below the refinements so the comparison can show whether each change improves
its parent rather than merely looking different.

## Selection

The operator selected **Tonal Ledger — Canvas** on 2026-09-03. Marquee Document
— Quiet is rejected with the other treatments and retained only for design
archaeology. The selected treatment keeps Concept A, makes the center pane the
document surface, uses a low-chroma violet-neutral system, and reserves bounded
material for the operator's notes and actionable recovery.

## Representative states

Render every treatment at 1080 by 900 in dark and light appearance:

1. **Sparse real-state analogue.** The observed long title, short overview,
   empty operator notes, transcript disclosure, and disabled Record explanation.
   The long transcript-derived title is retained as a stress fixture, not as a
   decision to keep that product defect.
2. **Dense generated note.** Overview, decisions, follow-ups, open questions,
   and one claim's source open in the inspector.
3. **Needs attention.** A recovered interrupted meeting with one next action
   and one destructive secondary action.

The sparse frame is the primary comparison. A treatment that works only when a
rich note and inspector fill the window fails.

## Acceptance questions

The comparison must let a cold reader answer these from the frames:

- Does the meeting remain the loudest object?
- Can the toolbar hold a long title and an unavailable Record state without
  becoming a sentence-shaped banner?
- Is the operator-notes area visibly editable without creating a fixed void?
- Are management actions available but clearly secondary?
- Does the selected row remain findable without overpowering the document?
- Does the sparse state look composed at the full window size?
- Do light and dark appearances carry the same hierarchy and contrast?

## Generic recommendations explicitly declined

The required design-database search classified Yawn as an AI marketing surface
and recommended horizontal storytelling, pink CTA color, Lora/Raleway, and
streaming effects. None fit the product contract. They are not inputs to these
treatments.

## Deliverables

Each treatment owns one directory under
`apps/desktop/ui-harness/visual-treatments/` containing:

- `styles.css` — the complete visual layer for the shared frame.
- `RATIONALE.md` — the treatment's visual argument and the question it tests.

The orchestrator owns the shared semantic frame, fixtures, capture script,
comparison page, cross-review, and the final replacement of provisional rules
in `DESIGN.md` after operator selection.

## Provenance

- `docs/design-direction-decision.md` — Concept A selection.
- `docs/experience-brief-2026-09-02.md` — user moments and platform strategy.
- `docs/evidence/comparables-window-composition-2026-09-02.md` — observed
  installed-app composition.
- `docs/evidence/screen-reviews/captures/refit-check/MANIFEST.md` — proof that
  the refit left the reading pane unchanged and invalidated its own premise.
- Operator screenshot, 2026-09-03 12:27 MST — current sparse installed state.
- Minder `DIRECTION.md`, `docs/design/experience-brief.md`,
  `docs/design/recommendation-2026-09-01.md`, design tokens, physical build 13
  captures, and Marquee dark/light concept frames — inspected directly for the
  second wave. The current physical build was not treated as a visual model.
