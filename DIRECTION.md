# Yawn — design direction

design_intent: refit
<!-- Set from the 2026-09-01 cold review of e7e96f9 (browser-render captures,
     provisional): revise on 4 of 9 surfaces, including composition on a core
     surface (two differently-styled Record affordances on Home). The
     direction itself stands -- the primary journey surfaces (first-run,
     capture, after, settings) passed cold, so this is refit, not rethink:
     presentation changes inside the approved direction. Refit's owed
     experience brief is this file's object/action/state matrix plus the
     surface roster. Installed-app captures supersede these verdicts and may
     move this back to preserve. -->

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

Short, quiet, Mac-native. One main content column, generous reading width,
light chrome. Color only for recording or attention, one evidence accent.
Concrete status language. The generated note is the primary reading surface;
transcript, provenance, and repair disclose at the point of doubt. Settings
stay a small auxiliary window. Motion exists to make state changes legible,
never to perform.

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

## Content reads

| Date | Surface | Strings | Verdict |
|---|---|---|---|
| 2026-09-01 | W10 first-run sheet, teaching empty state, guided hint | All W10 user-facing strings, verbatim in the packet report | Approved as-is by the operator; x-close retained |

## Job questions (the cold review's only context)

What is happening now? What is next? Who is involved? When and where? What
can I do?
