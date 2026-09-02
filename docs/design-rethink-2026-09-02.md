# Decision memo — rethink the Yawn desktop layout from the brief

2026-09-02 · prepared for the operator · CLOSED the same day: rethink run, Concept A selected (`design-direction-decision.md`)

## Recommendation

Declare `design_intent: rethink` in `DIRECTION.md` and run the judged-screen
pattern's own rethink path (§ 2c): a blind cold review of the installed build,
an experience brief that finally writes the platform strategy, three divergent
whole-screen concepts compared on the same states, and a selection ADR that
names who chose.

Do not rewrite the product brief. Its mission is sound and specific. The
rendered app never obeyed it, and the review instruments could not see that.

## The mission, as the brief already states it

Yawn is a private meeting notepad with a recorder attached. Three moments
matter: before (one record action, a short consent check), during (a calm
note canvas, visible recording state, one way to stop), after (a readable
note, transcript available, a simple list of past meetings). The finished
note is the destination. The list exists to reopen notes; it is not the
home-screen subject. The surface is "short, quiet, Mac-native: generous
reading width, one main content column, light chrome, and color only for
recording or attention." It opens to the next useful action, not a dashboard.

Nothing in this memo changes that.

## What the installed app actually renders

Four frames of the rebuilt preview (bfa0a80) were captured and read back this
morning (library, a recovered-interrupted meeting, a transcript-ready meeting,
Settings). What they show:

- Home is an eyebrow label, a display-size two-line headline ("Capture the
  conversation. Keep your own judgment."), a tagline, a call-to-action card,
  then "Recent meetings." That is the composition of a marketing landing page.
- A meeting is another full-page route: eyebrow ("SAVED ON THIS MAC"), display
  title, tinted amber panels for the note card and the status banner, and a
  text link "Back to meetings." Every navigation replaces the whole window.
- Chrome is a hidden-title overlay bar with a custom topbar. There is no
  persistent list, no list/detail split, no toolbar, no inspector.
- Color is spent on idle surfaces: the note card is a tinted panel, the Ready
  dot is blue, Record is permanently coral (already filed as R5).

Every comparable installed on this Mac (Notes, Bear, Craft, Agenda, Obsidian,
Granola) composes its window as a persistent list beside a document, with the
document as the stable center and controls in a toolbar. Yawn composes its
window as a sequence of web pages.

## Where the drift came from

**1. It was born with the brief.** Commit `2d2b681` (2026-08-11) rebaselined
the product, wrote "Mac-native, light chrome" into the brief, and shipped the
hero headline in the same change. The words and the screen have disagreed
since the first commit of the reset. No later step compared them.

**2. The research asked at the wrong altitude.** The 40-app review
(2026-08-31, `1ba0b9a`) mined comparables for features and evidence mechanics:
row previews (D1), show-source (D4), three-depth disclosure (D5), retry diff
(D6), reading measure (D8). It never captured how those apps compose a window.
The brief's guardrail, "do not add folders, saved views, dashboards ... just to
match the category," was read as also excluding the category's layout. The
one layout fact it did take, Bear's 15pt / 1.5 / 48em, is a type measure; it
was applied to the reading surfaces as Treatment C and left Home and the
meeting header untouched.

**3. The review instruments cannot see it.**
- The cold review asks five job questions (what is happening, what is next,
  who, when and where, what can I do). A landing page answers all five.
- The conformance review reads `DIRECTION.md`. That file promoted the brief's
  words but never wrote the **platform strategy** the judged-screen pattern
  § 2b requires: "which platform conventions the surface adopts, which it
  declines, and the reason for each decline." The gate went quiet instead of
  failing.
- `design_intent: refit` was declared on 2026-09-01, which by definition says
  the direction stands. No one had reviewed the rendered direction for
  platform fit before declaring it.
- `signal-frontend-designer`'s Artist role optimizes for "distinctive, not
  slop" web aesthetics. It has no native-desktop lane and never names macOS
  conventions. Eyebrow, display headline, tinted cards, and a CTA hero are
  its idiom, and they are what is on screen. It also requires a design system
  from `DESIGN.md` or a brand kit; Yawn has neither, so the Artist free-picked.

So the skills did not cause the drift by themselves; the reset's first commit
did. The skills and reviews failed to catch it for four specific reasons, each
fixable.

## Adjustments to the workflow

| Where | Change | Why |
|---|---|---|
| `DIRECTION.md` | `design_intent: rethink`; add a Platform strategy section | The pattern already requires it; its absence is the hole the drift went through |
| `DIRECTION.md` | Add "platform fit" to the review ledger's required verdicts | Makes the gap visible on every future review |
| Judged-screen pattern § 3b (methodology, via amendment) | Conformance review fails, not goes quiet, when the brief lacks a platform strategy | A quiet gate is what let `refit` be declared on an unreviewed direction |
| Judged-screen pattern § 3a (via amendment) | Sixth cold-review question when the brief names a platform: "Does this read as an installed app of its category, or as a web page?" | The five job questions test legibility, not idiom |
| `signal-frontend-designer` | A native-desktop lane, or an explicit route-out for desktop apps to a HIG-first brief | The skill's idiom is web; applying it unmodified to a Mac utility produces exactly this |
| Yawn repo | Write `DESIGN.md` as an output of the rethink | The design system must come from a record, not from CSS comments |

The methodology changes go through the amendment mechanism, not a rewrite.

## What the rethink owes, in order (§ 2c)

1. **Blind cold review of the installed build, before any rationale.** Four
   valid installed frames exist from this morning's pass; the first-run,
   during, and paused frames are still owed.
2. **Device captures of the installed comparables** for window composition
   only: Notes, Bear, Craft, Agenda, Granola. These carry private content;
   the operator decides whether frames are taken with real content, empty
   accounts, or not at all. Marketing captures do not qualify.
3. **Experience brief** with the platform strategy written, and the
   object/action/state matrix carried over from `DIRECTION.md`.
4. **Three divergent whole-screen concepts**, served live, compared on the
   same four states (first run, home with meetings, during, after). Seeds
   only, not developed here:
   - A. Source list + document, the Notes/Bear shape: persistent meeting list
     left, note center, transcript as an inspector, Record in the toolbar.
   - B. Notepad first, the Granola/Wispr shape: the app opens on the note
     canvas; the list is a sheet or popover; recording is the menu-bar pill.
   - C. Dated stream, the Agenda shape: meetings as a timeline with the newest
     note expanded inline.
5. **Selection ADR** naming the human who chose. That ADR becomes the
   direction record.

## What the blind review said about the symptom (added after it ran)

The blind reviewer of the installed build (`docs/evidence/screen-reviews/
all-surfaces-bfa0a80-installed-cold.md`), asked directly whether each window
reads as an installed Mac app or as a web page, answered: the main window
reads native, on chrome grounds (traffic lights, plain top bar, no browser
chrome, plain list rows). In the same review it found the composition faults
this memo names: the "marketing headline" holds the dominant position on
every home screen and "never yields ground once the list is populated"; no
screen "uses window width for anything beyond a single centered column";
"no split view ... one meeting, full width, mostly empty"; Settings reads
"closer to a marketing splash than a system preferences pane"; the back
link is the one element that "reads more like a web page." Nine of ten
frames: revise.

So the word "landing page" in this memo overstates the chrome and is
withdrawn. The symptom, stated precisely: a native window whose content is
composed as a marketing hero over a list, with the width unused and status
carried by full-width tinted banners. The rethink stands on that, and on the
operator's own judgment, which is the one the pattern reserves the decision
for.

## Falsifiers

- If the operator reads the four installed frames and judges the composition
  Mac-native, this memo is wrong about the symptom and the rethink is not owed.
- If a written platform strategy for the current layout can be produced that
  declines list/detail composition for a stated reason, the correct intent is
  `refit`, not `rethink`.

## Provenance

Read: `docs/product-brief.md`, `DIRECTION.md`, `docs/roadmap.md` (design and
refit intake), the two e7e96f9 screen reviews, the prior session's category
and design review artifacts, `judged-screen-pattern.md` § 2b/2c/3a,
`signal-frontend-designer/SKILL.md`, `apps/desktop/ui/styles.css`,
`tauri.conf.json`. Verified by command: hero headline first commit
(`git log -S`), brief and research dates, the platform-strategy requirement's
exact wording. Observed: four installed frames of bfa0a80, read back from
disk. Not checked: any comparable's actual window on this Mac.
