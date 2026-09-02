# DESIGN.md — Yawn desktop visual system

Source of record for the visual system, written 2026-09-02 from the selected
concept (A, source list beside document; `docs/design-direction-decision.md`).
Tokens live in `apps/desktop/ui-harness/concepts/shared/tokens.css` until the
implementation wave ports them into `apps/desktop/ui/styles.css`; when that
port lands, `ui/styles.css` becomes the token source and this file points
there. Rendered reference: `apps/desktop/ui-harness/concepts/a/`.

## Composition

- One main window, default 1080x900, minimum 900x600. Unified toolbar (52pt)
  with, left to right: traffic lights, sidebar toggle, window title, search
  field, Record control.
- Sidebar 280pt: meetings grouped by day (Today, Yesterday, Previous 7 days,
  Previous 30 days, then month), each row title · caption (time · length ·
  status) · one excerpt line; a needs-attention row carries a dot. Trash is
  the last item. During capture, "Recording now" is the first row.
- Document pane: the note at the reading measure (62ch), title, caption,
  Overview, Decisions, Follow-ups, Open questions. The empty selection is one
  centered caption. During capture the pane is the note canvas with the pause
  fact as a caption and a collapsed live-transcript disclosure.
- Inspector 320pt, slides in from the right when a claim is opened: the cited
  turn highlighted, neighbours dimmed, "Open full transcript" at its foot.
- Below about 900pt wide the sidebar collapses behind its toggle; nothing
  else reflows.
- Settings is a standard Preferences-style window: closable, minimizable,
  not modal.

## Type

System font (`-apple-system`). Body 13, caption 11, note body 15 at 1.6,
transcript 14 at 1.55, title 17, large 22. Nothing above 22. No display
sizes, no uppercase eyebrows, no letter-spaced labels.

## Color

System semantic colors in both appearances; the user's accent for selection
and primary buttons. Three reserved colors and nothing else carries hue:

| Role | Where it may appear |
|---|---|
| Record (system red) | The Record control idle and live; the "Recording now" row |
| Attention (system yellow) | The dot on a needs-attention row; the needs-attention surface's one headline |
| Evidence (accent at 10-16% tint) | The open claim and its cited turn |

Content areas are never tinted. Banners are never used for facts; a fact is
a caption. A full-width surface is used only for a condition that blocks or
endangers the record, and it carries a next action.

## Copy

The on-device promise is stated once, in Settings, in the three-fact card.
Elsewhere at most one "On this Mac" caption per screen. Status words:
recording, preparing, finishing, transcript ready, needs attention. One
severity vocabulary: facts are captions; problems are needs-attention states.
No product pitch inside the product. An untitled meeting is "Meeting · date";
transcript text is never a title.

## Motion

120ms fast, 200ms standard, ease-out. Inspector slide, sheet rise, selection
change. Nothing loops, pulses, or performs.

## What this replaces

The hero headline, eyebrow labels, tinted note card, amber banners, custom
topbar with hidden title, page-per-route navigation with a "Back to
meetings" link, and the green Settings accent. Diagnosis:
`docs/design-rethink-2026-09-02.md`.
