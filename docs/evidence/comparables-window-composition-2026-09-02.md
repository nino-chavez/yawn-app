# Comparables — window composition on this Mac, 2026-09-02

Device captures of six installed apps, taken by the agent with the operator's
authorization, dark appearance, each app's default window. The frames are not
committed: they show the operator's private notes and account state. They live
in the session scratchpad; this record carries only what the frames show about
how each app composes its window. Nothing here is a feature recommendation;
the product brief's anti-goals still hold.

| App | Window composition | Chrome | Where the primary action sits | What the document area is |
|---|---|---|---|---|
| Apple Notes | Three panes: folder sidebar, note list, note | Native unified toolbar with traffic lights, sidebar toggle, note count under the title, formatting and compose icons, search field | Compose icon in the toolbar | The note, edge to edge, date stamp centered at top |
| Bear | Three panes: tag sidebar, note list with excerpt and thumbnail, note | Custom-drawn but toolbar-shaped: sidebar, compose and search icons, formatting cluster right | Compose icon in the list header | The note, generous margin, large title |
| Craft | Sidebar plus content: space name, New Document, All Docs / Tasks / Calendar, folders, tags; content area is a document grid with view switcher | Custom sidebar, content header with view controls, sort, search | "New Document" as the first sidebar row, plus a "+" in the content header | Document cards; a document opens in the same pane |
| Agenda | Sidebar plus content: Overviews (On the Agenda, Today, To Do, Trash), projects; content is a dated stream | Custom, dark, with a "+" and search in the content header | "+" in the content header | A stream of dated notes under a project |
| Granola | Single column: "Coming up" calendar card, an upgrade banner, a dated list of meetings; "Ask anything" bar pinned to the bottom | Minimal: traffic lights, a sidebar toggle glyph, "New note" top right | "New note" top right | Meetings listed by date; a meeting opens as a note |
| Obsidian | Ribbon, file sidebar, tabbed editor, status bar | Custom tab bar in the title area, back/forward, view toggles | New note in the sidebar header | The note in a tab |

What the set has in common, and Yawn does not:

- **A persistent place where the list lives.** Five of six keep the list on
  screen beside the document. Granola is the exception and still keeps the
  list as the window's subject, with the meeting opening from it. None
  replace the whole window with a page and offer a text link back.
- **The primary action is a control in the chrome, not a headline.** Compose,
  "+", or "New note" sits in a toolbar or list header. No window opens with
  a display-size sentence explaining the product.
- **The window's width is spent on panes, not on a reading column with empty
  margins.** At 1000 to 1200 pt every app fills the width with list plus
  document. Yawn at 1080 pt shows one column and a wide right gutter.
- **Status and metadata are small and near the object.** Note counts, dates,
  and word counts sit in toolbars and list rows at caption size. Yawn puts
  status in eyebrow labels and full-width tinted banners.
- **Dark is a rendering, not a default.** All six use layered greys with one
  accent; none use tinted panels for content areas.

What Yawn shares with Granola specifically: a single-column, list-as-home
composition and a quiet toolbar. Granola gets there with a top-right "New
note" and a date-grouped list, no hero. That is the nearest existing shape to
the brief's "open to the next useful action, one main content column."

Frames: `scratchpad/cap/comp/{Notes,Bear,Craft,Agenda,Granola,Obsidian}.png`
in session aba45535 (not committed; re-take from the installed apps if
needed, they are all on this Mac).
