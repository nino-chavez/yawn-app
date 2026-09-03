# Yawn — design direction

design_intent: refit
design_direction: docs/design-direction-decision.md
<!-- 2026-09-02: the rethink concluded. The operator selected Concept A
     (source list beside document) from three rendered concepts; the
     selection ADR above is the direction record and DESIGN.md carries the
     visual system. Everything below the Thesis was the prior direction's
     matrix and ledger and still applies; the Character section is
     rewritten to the selected concept. History of the rethink:
     docs/design-rethink-2026-09-02.md (diagnosis), docs/experience-brief-
     2026-09-02.md (brief with platform strategy), docs/evidence/
     comparables-window-composition-2026-09-02.md, and the concept harness
     at apps/desktop/ui-harness/concepts (A selected; B and C kept). -->

This file promotes what [docs/product-brief.md](docs/product-brief.md) already
says into the shape the judged-screen pattern reads, and adds the two things
the brief does not carry: the object/action/state matrix and the review
ledger. The brief remains the product contract; where they disagree, the
brief wins and this file is stale.

## Thesis

A private meeting notepad with a recorder attached. The finished note is the
destination; the list exists to reopen finished notes. It is not a workspace,
dashboard, task manager, CRM, team wiki, or calendar.

## Character

Short, quiet, Mac-native, in the shape of Apple Notes and Bear: a unified
toolbar carrying sidebar toggle, title, search, and the one Record control;
a persistent meetings list on the left grouped by day, never the subject; the
note in a document pane at the reading measure; the transcript disclosed as
an inspector at the point of doubt. System font, system semantic colors, the
user's accent. Color only for recording (Record and the live state) or
attention (one dot, one needs-attention surface with a next action), plus
one evidence tint. No hero, no pitch line, no eyebrow labels, no tinted
content panels. The on-device promise is said once, in Settings. Status is
small and near its object. Settings behaves like Preferences. Motion exists
to make state changes legible, never to perform. Rendered reference:
`apps/desktop/ui-harness/concepts/a/`.

## Anti-goals

Folders, saved views, action dashboards, templates, calendar sync, meeting
bots, sharing, collaborative workspaces, chat over meetings, automatic task
creation, accounts, engagement mechanics. A first run teaches without
counterfeiting: no seeded meeting, transcript, claim, or count the operator
did not produce (brief amendment, 2026-09-01).

## Objects, actions, states

| Object | Owner | Valid actions | States | Reverse action |
|---|---|---|---|---|
| Meeting | Operator | record, name, open, lock/unlock, export, move to Trash, restore | idle -> arming -> recording <-> paused -> stopping -> captured -> transcribing -> transcript-ready -> summarizing -> ready; recovered-interrupted; locked; trashed | Trash <-> restore (30-day window); lock <-> unlock; pause <-> resume. Purge past the window is irreversible and says so |
| Recording (retained audio) | Operator, bounded by the retention choice | play (verified handle), release | retained (1/7/30 days) -> deleted-under-retention | None -- retention deletion is the privacy promise and is never reversible or deferred (runs even in Trash, even locked) |
| Transcript turn | The retained transcript (immutable) | read, search within meeting, restore-if-withheld (source-bound), see citing claims | attributed; speaker-corrected (projection, source untouched); **withheld -- rendered as withheld, never as missing or invented** | Speaker correction is a separate local operation, never a rewrite; withheld restore is source-bound |
| Generated note | Generated; reviewable AI output, never a final account | generate, regenerate (from current transcript), read, follow claim -> source | draft with overview/decisions/follow-ups/open-questions; transcript-highlights fallback when no summary is possible; stale after transcript promotion | Regeneration replaces a note only through the explicit path; keep-or-promote never silently regenerates |
| Claim + source | The evidence chain (claim and its transcript span are one unit) | show source (exact span, highlighted), hover-preview, open split view | evidenced (locators verify against the retained transcript) -- a claim without transcript evidence does not exist | Reverse: a turn shows its citing claims; absence of citation is visible, not hidden |
| Past-meetings list | Operator's library | open, title-search, transcript-search (probe flag only), open Trash | populated; genuinely empty (teaches); filtered-empty ("no matching"); loading -> honest stall after 10 s | Row preview comes only from an admitted note; absent means absent |
| Speech model (Settings) | Operator | choose, download, switch (between meetings only), remove (never the active one) | required (first run, calm glyph); downloading; verifying; active; error | Switching is reversible between meetings; the active model is never removable |

## Surfaces (representative states for review)

1. Home, first run -- zero meetings, three-moments sheet showing
2. Home, first run -- sheet dismissed, teaching empty state with guided invitation
3. Home with meetings -- rows with note previews
4. During capture -- canvas, recording state, pause control
5. After -- the note with overview and outcomes, source excerpt one click away
6. A withheld transcript turn
7. Needs-attention (a real failure state)
8. Settings with the active model
9. Platform accessibility: large text, increased contrast (operator-run; the
   agent does not change system settings)

## Ledger

| id | verdict | device | cites the thesis by | rules |
|---|---|---|---|---|
| cold-e7e96f9 | 01/03/04/06/07 accept; 02/05/08/09 revise (PROVISIONAL, browser-render) | browser render, stubbed bridge (captures at 0155e2d) | blind -- read only captures and the five job questions | judged-screen pattern, section 3a |
| conformance-w10-e7e96f9 | amendment honored; drifts: color budget (02, 07), note-ordering at 960px (04), prefers-contrast absent (09) (PROVISIONAL) | browser render, stubbed bridge (captures at 0155e2d) | the brief amendment of 2026-09-01 and this file | section 3b |
| cold-bfa0a80-installed | 01-07, 09, 10 revise; 08 accept. Category read: main window native by chrome; composition faults named (headline persists over populated list, width unused, no split view, Settings reads as splash) | this Mac, packaged preview, real storage, dark and light (captures at bfa0a80) | blind -- ten frames, five job questions, category read | section 3a plus category read; rethink phase 1 step 1 |
| cold-2765401-installed | 01-07 revise. Category read: window mechanics native (sidebar plus detail, native search, one system font); interior treatment reads web-in-a-window (flat hairline cards in Settings, first-run modal card, "Open" as trailing link text). Three to fix first: empty-state headline outranks the meeting title; failure copy contradicts its one control and the title bar stays stale; rows indistinguishable without a real title | this Mac, packaged preview of the concept-A rebuild, real storage, dark and light | blind, seven frames, five job questions, category read | section 3a plus category read; rethink phase 3 |
| cold-41c026b-installed | 01-08 revise. Category read moved: the main window now reads as an installed native Mac app (chrome, grouped rows, toolbar, bezel buttons); Settings still reads as bordered web cards. Three to fix first: the first-run sheet sits over an unmentioned system-audio gate; the interrupted state drops the meeting from the title bar and floats its message; the notes box carries a fixed dead zone under its placeholder | this Mac, packaged preview of the ported document pane, real storage, dark and light (captures at 41c026b) | blind, eight frames, five job questions, category read | section 3a plus category read; rethink phase 3, second pass |
| cold-88da6b6-installed | 01, 03, 04, 05, 07, 08 accept; 02, 06 revise. Category read: main window and Settings both read as an installed native Mac app. One to fix: the needs-attention block's only action, Move to Trash, wears the accent fill. Group-label weight and Settings icons declined as by design. Reviewer's frame 02 text misdescribes the frame; disposition in the record | this Mac, packaged preview of the R15-R19 refit, real storage, dark and light (captures at 88da6b6) | blind, eight frames, five job questions, category read | section 3a plus category read; rethink phase 3, third pass |
| cold-630d08a-installed | 01-05 accept. Category read: native Mac app, no web or prototype tells. First generated note on a packaged build reviewed. Findings: summary-failed shows no next step (confirms R23); "Transcript Only" reads as a constraint (R24); rename help line redundant (R25); Note badge versus Overview declined as two axes | this Mac, packaged preview with the generated note, real storage, dark and light (captures at 630d08a) | blind, five frames, five job questions, category read | section 3a plus category read; rethink phase 3, fourth pass |

## Content reads

| Date | Surface | Strings | Verdict |
|---|---|---|---|
| 2026-09-01 | W10 first-run sheet, teaching empty state, guided hint | All W10 user-facing strings, verbatim in the packet report | Approved as-is by the operator; x-close retained |

## Job questions (the cold review's only context)

What is happening now? What is next? Who is involved? When and where? What
can I do?
