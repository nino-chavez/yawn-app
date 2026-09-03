# Refit verification — partial, 2026-09-03 12:10

**Two frames, and the pass stopped when the screen locked.** Not a review set;
a build-verification check. The lock guard refused the third frame rather than
capturing, which is the intended behaviour.

Bundle: `Yawn Preview.app` rebuilt at 12:09 from `45b5146` via
`npm run preview-build`. 199 Mach-Os signed with the Developer ID,
`codesign --verify --deep --strict` exit 0, `preview-verify` clean. The Rust
crate genuinely recompiled (16.54s in the build log).

## What the rebuild proves

**The Record explanation is live, and it corrects an earlier guess.**
`00-refit-check` shows the toolbar rendering "Record" disabled followed by
"Yawn is finishing your last meeting. Recording will be available again
shortly." So the gate that actually fires on this machine is the **capture
state**, not microphone permission — which is what this session had speculated.
The reason chains back to the zero-turn meeting: its note failed, the app sits
in `summary-failed`, and that state disables Record.

**Manage now outweighs Rename.** A changed band at y 84-97pt over the button
row, which is the `font-weight: 600` the refit scoped to the manage control.

## The open question, now settled — and the answer is uncomfortable

The note frame was taken (`01-refit-note`). It shows the **same three bands**
changed and the document pane again pixel-identical: 0.54% of the frame differs
in total, all of it toolbar, button row and bottom edge.

The cause is not a build problem. The pane's rules were migrated — they read
`var(--space-*)` and `var(--t-*)` now — but **the migration was value-preserving
there**. `.doc-caption` went from `margin: 4px 0 18px` to
`margin: var(--space-4) 0 var(--space-18)`: the same numbers, spelled
differently. Its `font-size: var(--t-body)` was already correct and untouched.
`.doc-fact`, `.meeting-note-section` and their neighbours are the same story.
The pane's spacing was already on the scale; the 31 ad-hoc values were
concentrated in sheets, sidebar, transcript panel and Settings.

**So the refit did real internal work and changed the reading surface not at
all** — and the reading surface is what the operator was looking at when he said
it does not feel like a high-grade desktop app.

This also corrects a diagnosis made earlier in the day. The measured rhythm in
this pane — gaps of 1, 4, 10, 15, 16, 20, 20, 30, 33, 56, 56pt — was attributed
to the ad-hoc spacing values. It cannot have been: the pane's own declarations
were already 4, 12, 18 and 24. Those measured gaps are emergent, composed of
margins **plus** line-heights **plus** element heights, and they do not map one
to one onto declarations. The number was real; the cause assigned to it was not.

The consequence is worth stating plainly rather than filing as a nuance. The
document pane conforms to DESIGN.md's own budget — 22, 15, and one line of 13 —
and it still reads the way the operator described. That points at the
specification rather than at drift from it, and changing the specification is an
operator decision, not one to make from a stylesheet.

## Also visible, not yet judged

The Record explanation renders as a full sentence inline in the toolbar, and the
search field is narrower than it was in the baseline. Whether a toolbar is the
right place for a sentence that long is a design question this check does not
answer.
