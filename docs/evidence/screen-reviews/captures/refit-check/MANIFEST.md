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

## The open question this pass did not settle

Diffed against the build 21 baseline, only three narrow bands moved: the
toolbar (y 20-33), the button row (y 84-97), and the bottom edge (y 878-882).
**The whole document pane is pixel-identical** — 0.44% of the frame differs in
total.

That is unexpected and is not yet explained. The spacing migration is real in
source (10→12, 13→12, 38→32, 17→18, 11→12 among others) and CSS demonstrably
ships, since the Manage weight change rendered. So either the pane's own rules
were already on the scale and the migration was value-preserving there, or part
of the refit is not reaching this surface.

**Do not record the refit as verified until this is settled.** The decisive
frame is the note document, where `.read h2` was 13px under 15px prose and is
now 15 at 600 — a change that cannot be invisible. That frame is the first thing
to take when the screen is unlocked.

## Also visible, not yet judged

The Record explanation renders as a full sentence inline in the toolbar, and the
search field is narrower than it was in the baseline. Whether a toolbar is the
right place for a sentence that long is a design question this check does not
answer.
