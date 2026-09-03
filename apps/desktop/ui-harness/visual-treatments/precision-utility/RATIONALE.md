# Precision utility

This treatment makes the meeting note feel like the window's primary working
object. It is not an attempt to recreate a native control library in CSS. It
uses the quiet geometry, rank, and density that make a Mac utility dependable.

## Type

System text stays throughout. The title has enough size and a tight enough
measure to read as a document title, while toolbar, list, metadata, and source
details step down in deliberate increments. The source inspector uses a small
tabular time treatment only where elapsed time benefits from alignment.

## Neutral palette and depth

The palette is near-neutral rather than black-on-black. The chrome, sidebar,
reading surface, and raised editor are four close materials, separated by a
small number of firm rules. That makes the document easy to locate without
turning it into a card or spending color on ordinary content. Dark and light
use the same contrast roles, not inverted grays copied mechanically from one
another.

## Controls and selection

Toolbar columns reserve room for the unavailable-record explanation. The
explanation truncates as a status fact instead of claiming the toolbar row.
Record keeps the recording hue, but its disabled state is visibly unavailable.
Search is present but compact. Rename and Manage are transparent, low-contrast
secondary controls beside the title.

The selected meeting gets a quiet blue-gray material and a thin inset locator.
It remains easy to recover in a long list but cannot outshout the note title.
Attention retains amber; evidence retains blue; destructive cleanup remains a
text-weight action.

## Sparse-state composition

The empty operator-notes area is a bounded, labeled editor with a natural
minimum height. It reads as editable the moment the document opens, and it
does not require an invented prompt panel or decorative filler to keep the
sparse state composed. The document itself uses a limited reading measure and
one header rule, so it feels anchored inside the larger pane.

## Accessibility

Body copy and status text use distinct contrast steps rather than opacity
alone. Keyboard focus has a high-visibility outline. Semantic hue is always
backed by labels, control rank, or borders. The two themes retain the same
hierarchy, and reduced-motion preferences suppress transitions and animation.

## Known risks

The compact toolbar is intentionally demanding: very narrow windows will
eventually need the real application's responsive overflow behavior, rather
than allowing the disabled reason to expand. The inspector's 268-pixel width
is suited to this 1080-pixel comparison but should be tested with large text
and a resizable production split. The treatment must also be judged on the
actual native window because browser-rendered system fonts are only a close
approximation of AppKit text rendering.
