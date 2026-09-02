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

Where each size goes, because the 2026-09-02 refit set most of the document
pane at 13 and the operator found it too small to read:

- 22: the meeting title, once.
- 15 at 1.6: every sentence in the document pane, including empty states,
  help under a control, the operator's notes and their placeholder, and
  section headings (15 at 600).
- 13: sidebar rows, toolbar, controls, sheets, Settings, and the one
  metadata caption under the title (date · length · status).
- 11: group labels and the inspector label only.

A document pane, then, has three sizes on screen: 22, 15, and one line of
13. If a fourth appears, something is misfiled.

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

## Spacing

4, 6, 8, 12, 16, 18, 24, 48. Row padding 6/8; toolbar padding 12; sidebar
inset 16; inspector padding 18; document 24 top, 48 sides. Toolbar 52,
sidebar 280, inspector 320, controls 22 (26 on a needs-attention surface).
Radii 6 for controls, 10 for cards and sheets.

## Components and states

Rendered specimen, both appearances, increased contrast, and large text:
`apps/desktop/ui-harness/concepts/shared/specimen.html`
(`?theme=dark|light&contrast=more`). The specimen is the reference; a
component not on it is not in the system.

| Component | States | Rule |
|---|---|---|
| Button | default, primary, pressed, disabled, focused | One primary per surface. Disabled is opacity, never a color change. Every button is the specimen's `.btn`: 22 high (26 on a needs-attention surface), 10 side padding, radius 6, one border; buttons beside each other share a baseline and a gap of 8. No card, pill, link, or bordered row stands in for a button. A destructive action (Move to Trash, Remove download) is never the primary, even when it is the only button on the surface |
| Record control | idle, live (elapsed + Pause + Stop), paused | The only red. Lives in the toolbar and the menu-bar pill only |
| Icon button | default, hover | Sidebar toggle. No other icon buttons in the toolbar |
| Search field | empty, typing, filtered-empty | Title search only; "No matching meetings" is one caption |
| Toolbar | nothing selected ("Yawn"), meeting selected (title), recording ("New Recording") | Nothing else enters the toolbar |
| Sidebar row | default, hover, selected, needs-attention (dot), recording-now, large text | Title one line, caption, excerpt. Untitled is "Meeting · date"; transcript text is never a title |
| Group label | — | The only uppercase in the system |
| Trash row | — | Last item, hairline above |
| Document | empty selection, note, canvas (during) | Empty selection is one centered caption |
| Claim | closed (dashed hairline), open (evidence tint, accent underline) | Opening a claim opens the inspector; Esc closes both |
| Inspector | closed, open | 320, panel background, never scrolls the note |
| Transcript turn | default, highlighted, dim, withheld | Withheld renders as withheld text with Restore, never as missing |
| Disclosure | collapsed, expanded | "Live transcript (n turns)" under the canvas, "Full transcript" under the note; one line at body size with a trailing chevron, inside the reading measure, no card, no border, no description line |
| Needs attention | — | Headline, detail, one primary and one secondary action. No banner, no icon |
| Sheet | Start sheet only | Record disabled until three attestations and a retention choice |
| Popover | speech-model picker, row Manage menu | Never the meetings list |
| Toast | one line, optional action | Bottom center, 4 s, never for a fact a row already shows |
| Menu-bar pill | idle, live, paused | Click: Stop, Pause, Open Yawn |

## Focus and keyboard

Focus ring 2px accent, 1px offset, on every control and row. Tab order:
toolbar left to right, sidebar rows top to bottom, document, inspector.
⌘R Record · ⌘. Stop · ⌘⇧P Pause · ⌘⇧S Sidebar · ⌘F Search · ⌘, Settings ·
↑↓ selection · ⏎ open · ⌫ Move to Trash · ⌘⌥T full transcript · Esc closes
the inspector or sheet. Every shortcut is also a menu item.

## Accessibility

Large text: sizes follow the system setting; the measure is in characters,
so it holds. Increased contrast: separators and secondary labels darken, no
new colors (`prefers-contrast: more`, token override in tokens.css). Reduced
motion: all transitions 0 ms. Reduce transparency: the sheet dim is opaque
panel. VoiceOver: rows are buttons named "title, status, date"; the live
Record control announces elapsed time on demand; an open claim is a
disclosure whose expanded content is the cited turn. Captures of the large
text and increased contrast states are operator-run (DIRECTION.md roster
item 11) and belong in the cold review set of the rebuilt app.

## Icons

SF Symbols only, via the system: sidebar (`sidebar.left`), trash (`trash`),
record (`record.circle`), pause (`pause.fill`), stop (`stop.fill`), search
(`magnifyingglass`), settings gear in the toolbar is not used; Settings is
⌘, and the app menu. No custom glyphs, no emoji, no illustration.

## Family invariants (shared with Minder)

Yawn and Minder share no palette, type, radii, or components: one is a Mac
document window, the other a one-handed phone screen. They share the rules
below, stated once here and once in Minder's DIRECTION.md and DESIGN.md, so
both apps are built the same way even though they look different.

| Invariant | Minder | Yawn |
|---|---|---|
| The five job questions in under five seconds are the thesis and the review protocol | Today screen thesis | Every surface; the blind review's protocol |
| State is carried in words and structure, never color alone | Accessibility contract | Status words plus the dot; no color-only meaning |
| No explanatory chrome in steady state | Copy about the app appears only when something is broken and actionable | No pitch line; on-device promise said once, in Settings |
| Attention is not completion; a fact is not a problem | Attention, completion, freshness are distinct | A fact is a caption; a problem is a needs-attention state with an action |
| Four semantic tones with the same names, different values | violet, rose, gold, neutral | accent, record, attention, neutral |
| Source authority is shown, never restated | Source facts panel vs local details | Claim and its cited turn; the transcript is the record |
| Customer terminology, no internal jargon | Minder, activity, handoff, source calendar | meeting, note, transcript, recording; never queue, claim gate, router |
| Tokens are a schema, not a stylesheet | packages/design-tokens/tokens.json | The same schema, Yawn's values (implementation wave) |
