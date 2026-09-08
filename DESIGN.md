# DESIGN.md — Yawn desktop visual system

Status: **Tonal Ledger — Canvas selected 2026-09-03**. Lavender Haze was
selected for dark appearance on 2026-09-08.
The static reference remains the geometry authority; the selected palette
supersedes its dark colors. The signed Preview bundle is verified; installed
cold-review acceptance and visual-fidelity acceptance remain separate.

Concept A remains the decided window structure: source list beside document,
one Record control, and transcript evidence in an inspector. The operator
selected **Tonal Ledger — Canvas** for its visual system: a low-chroma
violet-neutral Mac window whose center pane is the document surface. There is
no floating document card. The operator's notes are the one quiet bounded
writing object.

The completed comparison is governed by
`docs/design/visual-system-rethink-brief-2026-09-03.md` and rendered from
`apps/desktop/ui-harness/visual-treatments/`. The selected static reference is
`tonal-ledger-refined/`. Its CSS and shared frame are the exact geometry
authority for this visual system. The product implementation lives in
`packages/design-tokens/tokens.json` and `apps/desktop/ui/styles.css`; it must
match the reference rather than reinterpret it. Static frames remain comparison
evidence, not installed acceptance.

The operator rejected Native Editorial, Private Notebook, Precision Utility,
Quiet Folio, and Marquee Document — Quiet. Their files remain for archaeology;
they are not alternate product themes. Evidence and limits:
`docs/evidence/screen-reviews/visual-treatment-comparison-2026-09-03.md`.

### Product rules retained through the selection

- Concept A's persistent meeting list, document pane, and conditional source
  inspector.
- The note-before-transcript reading order and the separation between operator
  notes and generated claims.
- The semantic jobs of record, attention, and evidence color.
- Copy truth, keyboard paths, focus order, ARIA, and reduced-motion behavior.

### Visual decisions made by Tonal Ledger — Canvas

- System sans throughout; the meeting title, not a display typeface, carries
  hierarchy.
- Violet-neutral chrome and canvas in both appearances. Selection is a quiet
  surface plus an inset locator, never a bright filled row.
- The pane supplies document containment. Only personal notes, evidence, and
  actionable recovery receive bounded or tinted material.
- Rename and Manage remain secondary. Generated-note prose remains readable at
  16px; the operator's editable note uses 14px.

## Composition

- One main window, default 1080x900, minimum 900x600. Unified toolbar (58pt)
  with, left to right: traffic lights, sidebar toggle, window title, search
  field, Record control.
- Sidebar 282pt: meetings grouped by day (Today, Yesterday, Previous 7 days,
  Previous 30 days, then month), each row title · caption (time · length ·
  status) · one excerpt line; a needs-attention row carries a dot. Trash is
  the last item. During capture, "Recording now" is the first row.
- Document pane: the canvas contains a centered 700px reading column (64ch for
  text). It has 36px top canvas inset, 26–68px responsive side inset, and 54px
  bottom inset; the reading column has 25px/14px/42px internal padding. The
  title, caption, and generated-note sections share that left edge. The empty
  selection is one centered caption. The transcript-only capture has the
  released-audio fact, “No meeting note yet.”, Generate note and its local-only
  explanation, personal notes, then a collapsed Full transcript disclosure.
- Inspector 264pt, slides in from the right when a claim is opened: the cited
  turn highlighted, neighbours dimmed, "Open full transcript" at its foot.
- The selected static reference is fixed to a 900px minimum width. Its capture
  breakpoint tightens chrome below 960px; a product sidebar-collapse behavior
  needs its own reviewed reference before it becomes a visual-system rule.
- Settings is a standard Preferences-style window: closable, minimizable,
  not modal.

## Type

System font (`-apple-system`) throughout. Base UI is 13px/1.4. The meeting
title is 28–36px at 1.08, weight 650, with a maximum width of 21ch.
Generated-note prose is 16px/1.56. The editable operator note is 14px/1.55.
Toolbar title and search are 12px; row titles are 13px/18px; metadata,
excerpts, save facts, and group labels are 11px; section headings are
13px/18px; inspector labels are 10px and inspector turn text is 12px/18px.
The needs-attention headline is 22px/28px. No separate editorial or display
family is introduced.

## Color

Tonal Ledger uses direct light/dark values rather than a branded gradient.
The authored `packages/design-tokens/tokens.json` is the color value authority;
`apps/desktop/ui/tokens.css` is generated from it. Lavender Haze lifts dark
surfaces to violet charcoal, softens white text, and uses dusty lavender
accents. Layout, typography, and light appearance retain their existing values.
The comparison is preserved in `apps/desktop/ui-harness/palette-studies/`.
Core roles are:

| Role | Light | Dark |
|---|---:|---:|
| Window chrome | `#e4e0e7` | `#26242e` |
| Sidebar | `#dfdbe3` | `#22212a` |
| Document canvas | `#faf8fb` | `#2c2a35` |
| Primary label | `#211f25` | `#e8e2ed` |
| Accent | `#6855a9` | `#b8a7ce` |
| Selection locator | `#8c7ca8` | `#a294b1` |

The live Record badge uses dark ink on the softened rose fill in dark mode
(`onRecord`), preserving readable text. Record, attention, and evidence keep
distinct semantic jobs:

| Role | Where it may appear |
|---|---|
| Record (muted red) | The Record control idle and live; the "Recording now" row |
| Attention (amber) | The dot on a needs-attention row; the bordered recovery surface |
| Evidence (accent at 10-16% tint) | The open claim and its cited turn |

The document canvas is the reading surface; it does not sit inside another
card. Banners are never used for facts; a fact is a caption. A bounded amber
surface is used only for a condition that blocks or endangers the record, and
it carries a next action.

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

4, 6, 8, 12, 16, 18, 24, 32, 48. Toolbar is 58px with 16px side padding;
sidebar scroll inset is 13px top, 7px sides, 16px bottom; group separation is
18px. Rows use 7px/9px/8px padding. The inspector is 264px with 25px/17px/18px
padding. Standard buttons are 28px minimum height with 4px/10px padding;
secondary buttons are 26px minimum height with 3px/9px padding; paired actions
have a 6px gap. Search is 29px high with 9px side padding. Controls have a 6px
radius and notes a 10px radius. Personal notes use 15px/16px/13px internal
padding; their editor has a 76px minimum, 6px vertical padding, and a 9px left
gutter.

32 was added in the 2026-09-03 visual refit, migrating `apps/desktop/ui/
styles.css`'s 31 ad hoc margin/padding/gap values onto this scale. Three
legacy sheet values (28, 32, 38px, eight declarations) clustered there;
snapping them straight to 24 or 48 instead would have cost 14-37% off each.
No other step changed.

## Components and states

The prior rendered specimen, both appearances, increased contrast, and large text:
`apps/desktop/ui-harness/concepts/shared/specimen.html`
(`?theme=dark|light&contrast=more`) is historical evidence, not the current
visual reference. The selected treatment frames and the rules below now govern.

| Component | States | Rule |
|---|---|---|
| Button | default, primary, pressed, disabled, focused | One primary per surface. Disabled uses the muted record treatment, not generic opacity. Every standard button is the shared `.button`: 28px minimum height, 4px/10px padding, 12px/17px type, 6px radius, and one border. Secondary buttons are 26px minimum height with 3px/9px padding; paired actions share a baseline and a 6px gap. No card, pill, link, or bordered row stands in for a button. A destructive action (Move to Trash, Remove download) is never the primary, even when it is the only button on the surface |
| Record control | idle, live (elapsed + Pause + Stop), paused | The only red. Lives in the toolbar and the menu-bar pill only |
| Icon button | default, hover | Sidebar toggle. No other icon buttons in the toolbar |
| Search field | empty, typing, filtered-empty | Title search only; 29px high, 9px side padding, 12px type. "No matching meetings" is one caption |
| Toolbar | nothing selected ("Yawn"), meeting selected (title), recording ("New Recording") | Nothing else enters the toolbar |
| Sidebar row | default, hover, selected, needs-attention (dot), recording-now, large text | Title one line, caption, excerpt. Selected uses the quiet selection surface, one-pixel border, and inset violet locator; text does not invert. Untitled is "Meeting · date"; transcript text is never a title |
| Group label | — | 10px/15px, 0.065em tracking; the only uppercase in the system |
| Trash row | — | Last item, hairline above |
| Document | empty selection, note, transcript-only, canvas (during) | The center pane is the document canvas, with no enclosing document card. Transcript-only contains the released-audio fact, a body-rank “No meeting note yet.”, Generate note, its help text, personal notes, then Full transcript. Empty selection is one centered caption |
| Claim | closed (dashed hairline), open (evidence tint, accent underline) | Opening a claim opens the inspector; Esc closes both |
| Inspector | closed, open | 264px, panel background, never scrolls the note |
| Transcript turn | default, highlighted, dim, withheld | Withheld renders as withheld text with Restore, never as missing |
| Disclosure | collapsed, expanded | "Live transcript (n turns)" under the canvas, "Full transcript" under the note; 12px, a trailing chevron, inside the reading measure, one top hairline and no card or description line |
| Needs attention | — | Bounded amber surface with a left locator, headline, detail, one primary and one secondary action. No banner |
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
