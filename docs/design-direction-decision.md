# Decision — Yawn desktop design direction (rethink selection ADR)

Status: DECIDED 2026-09-02.
Selected by: Nino Chavez (operator), after reviewing the 24 rendered frames
on the comparison page. Words used: "concept A wins completely."

This record replaces the prior direction in `DIRECTION.md` once a concept
is selected. Until then `DIRECTION.md` holds the object/action/state matrix
and the review ledger, and the experience brief
(`docs/experience-brief-2026-09-02.md`) holds the platform strategy.

## The question

Which whole-screen composition should Yawn's main window have, given the
brief's three moments, the platform strategy, and the blind review of the
installed build (nine of ten frames: revise)?

## The three concepts

Renders: `apps/desktop/ui-harness/concepts/compare.html` (24 frames: three
concepts, four states, both appearances), served with
`preview apps/desktop/ui-harness/concepts 8791`.

| | A — Source list beside document | B — Notepad first | C — Dated stream |
|---|---|---|---|
| Shape | Sidebar list, note in the document pane, transcript as an inspector | The window is the note; the list is a popover from the toolbar; transcript as a synced split below | One column of day-grouped items; the selected one expands inline; source inline under the claim |
| Nearest installed comparable | Apple Notes, Bear | Granola, Wispr Flow | Agenda |
| Where the brief's "list exists to reopen notes" lands | Always visible, never the subject | Hidden until asked, then one click | The stream is both the list and the reading surface |
| Where During lives | Document pane becomes the canvas; sidebar shows "Recording now" | The same note surface; recording state in a bottom status bar | A "Now" item pinned at the top of the stream |
| Adopt-table departure | None | List is a popover, not a pane | List and document are one column |
| Risk to test | Sidebar invites folders and views the brief forbids | The library is out of sight; finding a past meeting is two steps | Expanded items push the list off screen; long notes make the stream a page |

## Comparison on the four states

(Filled after the operator reviews the renders. One line per state per
concept: what the eye lands on, whether the five job questions are
answered, what competes.)

| State | A | B | C |
|---|---|---|---|
| 1 Home with meetings | Eye lands on the list; all five questions answered from the sidebar rows plus the toolbar Record. The document pane is an empty two-thirds of the window with one caption. Nothing competes. | Eye lands on the open popover, then the note behind it. The list is only there because the popover is open; close it and the home is a meeting with no note. Record competes with nothing, but "what is next" is two clicks for any other meeting. | Eye lands on the first row. Answers the five questions in one column with no chrome beyond the toolbar. The right third of the window is empty; the column reads as a page. |
| 2 During capture | Live Record in the toolbar, "Recording now" as the first sidebar row, canvas in the document pane, pause fact as a caption. State is legible from the chrome alone. | The canvas is the whole window; recording state is a bottom status bar and the live toolbar. Calmest of the three; the pause fact is small and at the bottom, easy to miss. | A "Now" item expanded at the top of the stream with the earlier meetings still below it. Legible, but the past meetings share the screen with the live canvas, which the brief's During moment does not ask for. |
| 3 After, claim source open | Note in the reading measure, source in a right inspector with neighbours dimmed. The note stays primary; the inspector does not scroll the note. | Note full width to the measure, source as a pinned bottom strip. The strip takes a third of the height and pushes the note up; strongest evidence disclosure of the three for reading the surrounding turns. | Source quoted inline under the claim. The lightest disclosure and the most literal "one click away," but it reflows the note, and the meetings below the expanded item are visible while reading. |
| 4 Needs attention | Attention row selected in the sidebar; the pane shows the headline, detail, and two actions. The library stays reachable without leaving the state. | Headline, detail, two actions, nothing else. Cleanest single-purpose frame; the library is behind the Meetings button. | The attention item expanded in place with its neighbours collapsed around it. The library is visible, but the failure sits between two ordinary rows and reads as one of them. |

Read across the four states, the divergence is real: A spends width on
persistence, B on calm, C on continuity. A answers the five questions from
the chrome in every state; B is the strongest During and After surface and
the weakest Home; C never hides the list and never gives the note a room of
its own.

## Selection

Concept: **A — Source list beside document.** No hybrid.

Reason, as the comparison shows it: A is the only concept that answers the
five job questions from the chrome in every one of the four states. The list
is persistent and never the subject; the note has a room of its own; the
recording control is one place in the toolbar; a failure state keeps the
library reachable without leaving it. It also carries no departure from the
brief's adopt table, so the platform strategy stands as written.

What this declines, and why:
- B's popover list. Calmer During and After surfaces, but the library is
  out of sight on Home, and finding a past meeting is two steps. The brief
  says the list exists to reopen notes; hiding it makes reopening a search.
- C's single stream. Continuity, but the note never gets a room of its own,
  a long note turns the stream into a page, and a failure sits between
  ordinary rows and reads as one of them.
- B and C stay in the repo as rendered alternatives, with their frames, so
  the next person who proposes a popover or a stream can see what was
  compared and why it lost.

## Consequences

- `DIRECTION.md` `design_intent` returns to `refit` pointing at this record.
- `DESIGN.md` is written from the selected concept's tokens and rules.
- The methodology amendments in `docs/design-rethink-2026-09-02.md` are
  filed (platform-strategy gate; category-read question).
- The first implementation wave rebuilds Home and the meeting view to the
  selected composition; the D-LOCK, D-TOAST, and R-series findings carry
  over unchanged.

## Provenance

Blind cold review bfa0a80-installed; comparables composition record;
experience brief 2026-09-02; concept renders (synthetic note and capture
fixtures, real library rows).
