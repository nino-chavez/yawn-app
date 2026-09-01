# local-meeting-notes — working guide

Read [`docs/product-brief.md`](./docs/product-brief.md) before changing the
product surface. It is intentionally short: the product is being rebuilt from
first principles, so do not restore a retired screen, design system, prototype,
or navigation model from Git history without a new operator decision.

The source code is the authority for technical behavior. In particular:

- The app captures only after explicit participant-consent, headphone, and
  single-operator attestations. The accepted audio-retention choices are 1, 7,
  and 30 days.
- Data must stay on the Mac. Do not imply an account, cloud sync, bot, calendar
  integration, automatic sharing, or task creation that is not actually built.
- A transcript and a generated meeting note are different things. Render a
  withheld transcript turn as withheld, never as missing text or a guess.
- `apps/desktop/ui/` is the bundled frontend. Use the named Tauri commands and
  their returned fields; do not reach into storage from browser code.
- Treat existing local changes outside the task as user work. Do not reset or
  discard them.

For a frontend change, run `npm run test:ui` from `apps/desktop`. For a Rust
contract change, also run the relevant `cargo test -p local-meeting-notes-desktop`
lane. A successful automated check proves code behavior, not a real meeting or
human-quality review.

A changed surface is not done until a cold screen review of it exists under
`docs/evidence/screen-reviews/` (`kind: cold`, by a reviewer who did not build
it and has not read its rationale); passing `test:ui` does not satisfy this.
Every closeout names what was removed, combined, demoted, or hidden, or says why
nothing was. UI tests assert state and role, not label copy, until the copy is
in `DIRECTION.md`'s content-reads table. Rule and review protocol:
`tools/blueprint/template/docs/methodology/judged-screen-pattern.md`.
