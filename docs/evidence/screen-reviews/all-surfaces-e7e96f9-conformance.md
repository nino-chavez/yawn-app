---
surface: all (01-09 roster)
build: e7e96f9
commit: 0155e2d (captures), cdeb599 (direction)
device: browser-render (stubbed bridge, synthetic fixtures) — PROVISIONAL, superseded by installed-app captures
reviewer: conformance-reviewer agent (read DIRECTION.md and the 2026-09-01 brief amendment first)
implementer: W10 packet agent and prior waves
kind: conformance
cold: false
verdict: amendment honored (no counterfeiting anywhere); drifts — color budget (02, 07), note-ordering at 960px (04), prefers-contrast absent (09)
---
---
surface: all
build: e7e96f9-browser
commit: 0155e2d
device: browser-render
reviewer: conformance-agent
implementer: w10-first-run agent
kind: conformance
cold: false
verdicts:
  01-home-first-run-sheet: conforms
  02-home-first-run-empty-state: not-evidenced
  03-home-with-meetings: drifts
  04-during-capture: conforms
  05-after-note-default-view: drifts
  06-after-note-scrolled: conforms
  07-withheld-turn: conforms
  08-needs-attention: conforms
  09-settings-active-model: drifts
  10-accessibility-large-text: conforms
  11-accessibility-increased-contrast: drifts
---

# Conformance review — W10 first-run build (e7e96f9)

Read DIRECTION.md and the product brief's Interface rules + 2026-09-01
amendment first, then judged the nine captured surfaces against them. Captures
are browser renders with a stubbed bridge and synthetic fixtures; that lowers
confidence about exact pixel/window behavior, not about composition, string
content, or structural ordering, which are all directly legible in the DOM as
rendered.

## Surface 1 — Home, first run, three-moments sheet showing (`01`)

**Verdict: conforms.**

The sheet renders "A meeting has three moments," with Before/During/After
cards whose copy paraphrases the brief's own three-moments section (consent +
headphones + retention choice; a calm canvas for notes; a generated note that
links back to the transcript, staying on the Mac). Nothing on the sheet
states a meeting, transcript, claim, or count — it explains the product in
its own words, which is exactly what the 2026-09-01 amendment permits. The
blurred backdrop behind the modal shows "No meetings yet" — the sheet sits
over a genuinely empty library, not over seeded rows. This matches the ledger's
"Content reads" entry, which records the operator already approved these
strings verbatim, including the x-close.

## Surface 2 — Home, first run, sheet dismissed, teaching empty state (not captured)

**Verdict: not evidenced.** DIRECTION.md's surface roster lists this as its
own state (item 2), distinct from the sheet. No file in the capture set
renders it head-on — only the blurred backdrop under the surface-1 modal
hints at its copy ("Record meetings," "No meetings yet," "Press Record to
start," "Transcript on this Mac," "All appears right here"), too indistinct to
judge wording or layout. The ledger's content-reads row says this state's
strings were already reviewed and approved, so this is a capture-coverage gap
for this pass, not a signal of drift — but it should be captured cleanly next
time so the guided-invitation language can actually be checked against "teach
without counterfeiting."

## Surface 3 — Home with meetings, rows with note previews (`02`, `09`)

**Verdict: drifts** — color budget, not composition.

Row composition conforms: each preview line reads like an excerpt from an
admitted generated note ("The team agreed to ship the redesigned onboarding
flow next sprint."), and the locked "1:1 with a teammate" row shows no preview
at all rather than fabricating one — exactly the reverse-action rule for
Past-meetings list ("Row preview comes only from an admitted note; absent
means absent"). The synthetic fixture meetings themselves are the review
harness standing in for real captures, per the brief for this review, so they
are not judged as counterfeiting.

The drift is DIRECTION.md's Character line, repeated in the brief's Interface
rules: **"Color only for recording or attention, one evidence accent."** In
this idle "Ready" state, two colored elements sit outside that budget: the
status dot renders in a blue/violet accent even though nothing is recording or
needs attention, and the "Record" button is filled solid coral as a permanent
primary-action color rather than a state-triggered one. The same blue/violet
also serves as the "Show source" evidence-accent color elsewhere in the app
(surface 5/6), so reusing it for a plain "Ready" indicator dilutes the one
color that is supposed to mean "there is evidence to check." Comparing across
captures: the dot correctly turns red for "Recording" (surface 4) and amber
for "Needs attention" (surface 8) — so the budget is honored for those two
states, but the idle state and the CTA button carry color the rule does not
license.

## Surface 4 — During capture (`03`)

**Verdict: conforms.**

Status language is concrete throughout: "Recording locally," "Both audio
sources are being captured on this Mac," the "Recording" pill, elapsed time —
all matching the brief's required vocabulary ("recording, preparing,
finishing, transcript ready, needs attention"). Pause/Stop controls are
visible without any settings, diagnostics, or planned-feature clutter nearby,
matching the interface rule to keep the recording control clear of that noise.
Color usage here is the correct case: the coral pill, activity card, and Stop
button all key off the actual recording state, so this surface alone does not
drift on the color-budget rule flagged above.

The canvas splits "What is this meeting for? (optional)" (guidance for the
generated note) from "Your notes" (the operator's own reminders). This split
is additive relative to the brief's single "unconstrained place to write
their own reminders" — not contradicted by any anti-goal, but not named in
DIRECTION.md's object/action/state matrix either. Judged as **not covered by
the direction** rather than a drift, since the field is explicitly optional
and is not styled as a form to complete.

## Surface 5 — After, default view at 960px (`04`)

**Verdict: drifts** — ordering, not omission.

Per the capture producer's note, the notes/context aside precedes the
generated note in document order at the reviewed width, so this is what
actually renders first: "Listen to saved audio," "Meeting context," "Your
notes." The generated note (Overview/Decisions/Follow-ups/Open questions)
only appears after scrolling (surface 6, `04b`).

This is a direct drift from two explicit statements: DIRECTION.md's Character
line, **"The generated note is the primary reading surface,"** and the
brief's Interface rule, **"Make the generated meeting note the first readable
result after capture. Lead with the overview and outcomes."** Both say the
note comes first; the rendered document order says the aside does. This is a
structural fact about the markup at the default width, not an artifact of the
browser stand-in — reordering DOM sections is exactly the kind of thing this
capture medium *can* show reliably.

## Surface 6 — After, scrolled to the note (`04b`)

**Verdict: conforms**, once reached.

Overview leads, followed by Decisions / Follow-ups / Open questions, each
with its own "Show source" link — this matches "lead with the overview and
outcomes" and "every generated decision, follow-up, and open question needs a
clear path back to the retained source text." "Regenerate note" is present as
the explicit path the matrix requires (regeneration never happens silently).
This surface is well-formed; the drift recorded above is about what a reader
sees *first*, not about this content's correctness.

## Surface 7 — A withheld transcript turn (`05`)

**Verdict: conforms.**

The withheld turn renders as italicized text, "This turn was withheld by the
voice check," with a "Restore this turn" action beneath it — never as missing
text, an ellipsis, or an invented guess. This is exactly the matrix's
non-negotiable rule for Transcript turn state ("withheld — rendered as
withheld, never as missing or invented") and CLAUDE.md's identical
instruction. "3 of 6 turns are cited by the note" is a real, derivable count
from the fixture, not a fabricated number, so it does not trip the anti-goal
against fake counts.

## Surface 8 — Needs-attention (`06`)

**Verdict: conforms.**

"Yawn cannot record yet." / "Yawn could not verify its local recording engine
after the last restart." is concrete, names the actual failure, and offers a
real recovery action ("Check again") rather than presenting a degraded state
as complete or hiding it behind vague language. The status dot is amber here,
correctly inside the recording-or-attention color budget.

## Surface 9 — Settings with active model (`07`)

**Verdict: drifts** — scope creep in the color budget, same rule as surface 3.

Content stays inside the brief's limits for Settings ("audio access and local
speech-model storage. Show which model is in use."): "What leaves this Mac,"
"Where transcription runs," "What is kept, and for how long," then "Speech
model" with "Compact model" tagged "In use." That is the correct information
and the active model is clearly shown, per the matrix and the interface rule.

Two things sit outside the direction as written. First, the same color-budget
rule from surface 3 recurs: the "In use" tag is rendered in a persistent green
accent that is neither a recording state nor an attention state — a third
status color alongside the coral CTA and the blue "Ready" dot, none of which
DIRECTION.md's "color only for recording or attention, one evidence accent"
licenses. Second, the whole surface carries a distinct green-tinted
background unlike the neutral near-black used everywhere else; this isn't
named as a violation of any specific rule (no rule speaks to Settings'
background hue), so it's judged **not covered by the direction** rather than
a drift, but it's worth the operator's eye given how tightly the rest of the
character rules are specified elsewhere.

Cannot verify from a browser capture whether the window itself is "small" and
"auxiliary" as required — that is a real window-chrome property this capture
medium cannot show. Flagged as reduced confidence, not a verdict.

## Surface 10 — Accessibility, large text (`08`)

**Verdict: conforms.**

Type scales up and the layout reflows without breaking the single-column,
generous-reading-width composition — headline wraps to three lines, body
copy and row previews stay legible, nothing overlaps or truncates. This is a
real, mechanically-checkable difference from surface 3 (larger point sizes
throughout), so the app does respond to this axis of the accessibility
surface.

## Surface 11 — Accessibility, increased contrast (`09`)

**Verdict: drifts.**

Per the capture producer's note, this file is byte-identical to surface 3
because neither stylesheet carries a `prefers-contrast` rule anywhere. Unlike
the large-text case, there is no differentiation at all: the app does not
respond to the increased-contrast signal in any way.

DIRECTION.md's surface roster names "increased contrast" as its own
representative state to review (item 9), on par with large text — the
roster's inclusion of it presumes some distinguishable response exists to
judge. There isn't one. This also sits awkwardly against the Character line
"quiet, Mac-native": a Mac-native surface is expected to notice platform
accessibility settings the way it already does for text size, and this one
silently doesn't. No rule in the brief's Interface rules explicitly mandates
contrast handling as a line item, so the citation here is the surface
roster's own listing plus the Mac-native half of the Character line, not a
harder textual rule — flagged as a drift on that basis, with the caveat that
it is the softest-cited drift in this review.

## Overall — was the amendment honored?

Yes, on its actual terms. The 2026-09-01 amendment is about counterfeiting:
no seeded meeting, transcript, claim, or count the operator did not produce.
Nothing in these nine captures fabricates provenance — the first-run sheet
teaches the three moments in the app's own words over a genuinely empty
library, the withheld turn renders as withheld rather than invented text, note
previews disappear rather than being faked for a locked meeting with no
admitted note, and every generated claim carries a real "Show source" link
back to the retained transcript. Counterfeiting, specifically, was not found.

What the review did find are three drifts from other named rules in the same
documents, none of them counterfeiting: the generated note is not actually
what a reader sees first at the default width, contradicting "the generated
note is the primary reading surface"; a color budget stated twice
("recording or attention, one evidence accent") is exceeded by a permanent
CTA color and a reused evidence-accent hue on an idle status dot and an
"in use" tag; and the increased-contrast accessibility state the surface
roster names has no implementation to review. None of these threaten the
anti-counterfeiting guarantee itself, but the note-ordering drift cuts against
the same brief passage the amendment sits inside ("Make the generated meeting
note the first readable result after capture"), so it is the one worth
treating as more than cosmetic.
