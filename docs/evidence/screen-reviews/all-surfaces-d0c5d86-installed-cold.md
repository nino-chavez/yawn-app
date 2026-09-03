---
kind: cold
build: d0c5d86
surface: all-surfaces (installed, dark)
date: 2026-09-03
reviewer: cold agent, did not build these surfaces and did not read their rationale
---

## Verdict

The reading surface is calm and the copy is mostly honest, which makes the
failures more conspicuous rather than less. Two of them break the product's own
contract. A note generation that runs for five minutes and produces nothing
returns the screen to a state pixel-identical to the one before the click, so a
failed run is stated as no run at all. A generated note renders an Overview and
nothing else — no decisions, no follow-ups, no open questions, and no words
saying whether those are empty or absent — so a thin summary reads as a complete
record. Below that: a destructive confirmation whose two buttons are pixel-
identical apart from their labels, a transcript header whose layout has broken
badly enough to hide the citation count, and a meeting library where three
entries share one title and a fourth is titled with a disfluent transcript
fragment that then propagates into the delete dialog as the only identification
of what you are about to destroy.

**Provenance note.** I read `product-brief.md` and the frames. I also opened
`MANIFEST.md`, which turned out to contain the capture author's own findings
rather than only a frame index. Ordering matters here: the 01/09 pixel diff, the
04/06 dialog crops, and the full-resolution read of 10a were all done **before**
that read. Every claim below is traceable to a frame or to a pixel measurement I
ran myself. Nothing from the manifest's on-disk claims (log entries, model files,
process state) appears here — that is outside what these frames can show.

---

## Findings, most severe first

### 1. A five-minute generation that failed leaves no trace that it happened
`08-dark-generating.png`, `09-dark-after-generate.png`, `01-dark-open.png`

`08` enters the working state correctly: "Preparing your meeting note.", the
button disabled and relabelled "Generating note…", and "The note model is
reading this transcript on your Mac. This can take several minutes — you can
keep using Yawn." Five minutes later, `09` is visually identical to `01`, the
frame taken before the attempt. I diffed them: of 3,888,000 pixels, 18 differ by
more than 30/765, all on a single row at the window's top edge — window-shadow
antialiasing, not content.

So the person who clicked and walked away comes back to the same heading, the
same body sentence, the same enabled button, and no timestamp or attempt count.
`01` was already reporting a *prior* failure — "Note not created", "Yawn could
not create a note" — so after the attempt the screen cannot distinguish "that is
the old failure you already knew about", "it is still running", and "the run you
just started also failed". The brief requires that "an interrupted or failed run is
stated plainly."

**Remedy.** The post-attempt state must differ from the pre-attempt state in
words, not only in internal state. Render the attempt as a fact: an attempt line
carrying the time and outcome ("Tried at 9:30 AM — the note model stopped
without producing a note."), and change the button label to "Try again" once an
attempt exists, so the control itself distinguishes a first run from a retry.
If the run has an elapsed clock while in flight, keep it; a five-minute silent
wait with no progress signal is the condition that produces the walk-away.

### 2. The generated note shows Overview only, and does not say whether the rest is empty or absent
`02-dark-note.png`

The note contains one section, "Overview", holding two sentences: "The
discussion centers on introducing something to BigCommerce." and "The speaker
wants to reframe the topic." There is no Decisions, Follow-ups, or Open
questions section, and no line saying whether the meeting had none or the model
produced none.

The brief's central requirement is that the generated note "separates decisions,
follow-ups, and open questions", and that "a tidy summary must never look like a
complete record." Two vague sentences under a single heading, with nothing else
on screen, is exactly the shape that reads as complete. A person deciding
whether to reopen the transcript has no signal that anything is missing.

**Remedy.** Render all four sections always, with an explicit empty state per
section that names which of the two cases it is — "No decisions were identified
in this transcript." is a different sentence from "This note did not produce a
decisions section." The absent sections are the load-bearing information here;
suppressing them is what makes the note look finished.

### 3. The delete confirmation's two buttons are pixel-identical apart from the label
`04-dark-trash-confirm.png`, `06-dark-lock-sheet.png`

Measured. In `04`, "Cancel" and "Delete meeting" both have fill rgb(58,58,58)
and the same 1px rgb(78,78,78) rim. Nothing but the text distinguishes the
irreversible choice from the escape. Neither carries a default-button
indication. In `06`, the reversible action gets the emphatic treatment: "Lock
meeting" is rgb(59,130,247), no rim, unmistakably the primary.

The warm tint that ought to carry the danger signal is spent on the wrong
elements. The framed title box border is rgb(111,77,69) — the same warm
red-brown — in **both** dialogs, including the reversible one, so it means
nothing. The dialog's own outer border does differ (rgb(83,48,41) on delete,
rgb(177,178,176) on lock), but the destructive dialog's frame is the *darker,
lower-contrast* of the two against the rgb(32,33,30) backdrop. The
irreversible sheet is the quieter of the pair on every axis measured.

**Remedy.** See Q1 — Cancel becomes the accent-filled default, "Delete meeting"
takes a red label rather than a red fill, and the warm tint is stripped from
`06` so it means danger where it appears. Full reasoning there.

### 4. The transcript header has collapsed; the citation count is hidden behind a button
`10a-dark-transcript-header-detail.png`, `10-dark-transcript.png`

The "Source transcript" label wraps to two lines, and two description blocks
have collapsed to roughly one word per line in columns about 90px wide, while a
row of three buttons — "Copy transcript", "Open transcript file", "Export
meeting" — sits on top of their first lines. Full answer under Q3.

**Remedy.** Put the button row on its own line beneath the label, and let the
description run at the card's full width as a single sentence. The header is
three short strings and three buttons; it does not need columns.

### 5. A transcript fragment is used as the meeting title, including as the subject of the delete confirmation
`02`, `03`, `04`, `06`, `10-dark-transcript.png`

The fourth meeting is titled "um, rule-based like system where it's like these
products and these fields," — verbatim the first transcript turn, disfluency,
trailing comma and all. `10` confirms the source: transcript turn `0:00` is that
exact string. It propagates to the window title bar, the sidebar entry, the
document heading, and both confirmation sheets.

This matters most in `04`. The thing you are being asked to permanently destroy
is identified only by that fragment, presented inside a rounded bordered box
that is styled like a text input — so it also reads, momentarily, as a field you
might be expected to type into. An unreadable subject plus two identical buttons
is the worst pairing on this surface.

**Remedy.** Derive a title from the transcript rather than copying its first
turn — strip leading disfluency, cap at a phrase, trim to a clause boundary — and
fall back to "Meeting · <date>" when nothing usable comes out. In the
confirmation sheets, render the name as text with a clear non-input treatment,
and show the date and duration next to it so an ambiguous title is still
identifiable.

### 6. "Delete transcript" carries no ellipsis, implying it fires with no confirmation
`03-dark-manage-menu.png`

The Manage popover offers "Lock meeting…", "Delete transcript", "Move to
Trash…". On macOS the ellipsis says further input follows; its absence says the
action commits immediately. So the one item that destroys the record fires
without a sheet, while the two that don't destroy anything both ask first.

That reading is severe in this app specifically. `02`'s own copy says "The audio
was already deleted… this meeting cannot be retranscribed", and the brief calls
the full transcript "the record for checking a decision, owner, or follow-up
that matters." Deleting it is unrecoverable and unrepeatable.

**Remedy.** Confirm it, and label it "Delete transcript…". State in the sheet
that the note's source links will stop resolving and that the meeting cannot be
retranscribed once the audio is gone.

### 7. Three sidebar entries share one title, and the fourth shows no date
`01-dark-open.png`, `02-dark-note.png`

Under "Previous 7 days": "Meeting · Sep 1, 2026", "Meeting · Sep 1, 2026",
"Meeting · Sep 1, 2026". The only distinguishing text is the second line —
"8:55 AM · transcript available", "8:48 AM · interrupted", "8:44 AM ·
interrupted". Meanwhile the "Previous 30 days" entry's second line reads "1:59
PM" with **no date at all**, so the one entry whose day you cannot guess from
the group heading is the one that omits it. The title line duplicates what the
subtitle already says on three entries and drops it on the fourth.

An amber dot marks the two interrupted entries and is never explained.

**Remedy.** One date-and-time source per row: title carries the meeting name (or
"Meeting" alone), subtitle carries date + time + state, on every row without
exception. Give the dot a text state next to it or remove it — "interrupted"
already appears in the subtitle, so the dot is currently redundant *and*
unlabelled.

### 8. The retention caveat is the first thing you read, above the note
`02-dark-note.png`, `01-dark-open.png`

In `02` the first body paragraph is "The audio was already deleted. The
transcript and note remain available, but this meeting cannot be retranscribed."
The Overview comes after it. In `01` there are three such paragraphs stacked
above the action. The brief says to "make the generated meeting note the first
readable result" and "lead with the overview and outcomes."

**Remedy.** Demote retention and pause facts to a quieter row beneath the note,
or into the meeting's detail line next to "Meeting note". They are true and
worth keeping; they are not what the person opened the meeting to read.

### 9. The only source affordance is a dotted underline with no label
`02-dark-note.png`

Both Overview sentences carry a dotted underline. Nothing on screen says what it
is or that it is clickable. In dark grey text a dotted underline reads first as
a spell-check squiggle and second as an abbreviation tooltip. The brief calls a
clear path back to the source text non-negotiable, and this is the whole of it.

**Remedy.** Give the affordance a name on screen — a "Show source" control per
claim, or a visible marker with the turn's timestamp — and state once, near the
Overview heading, that every claim links to the retained transcript.

### 10. "Record" is the least prominent control on the screen, and it is red
`01`–`10`, toolbar

Measured in `02-dark-note.png`. "Record" is a filled button — fill rgb(43,43,43),
rim rgb(52,52,52) — against a rgb(30,30,30) toolbar. "Rename" and "Manage",
the two secondary controls in the content area, are rgb(58,58,58) with a
rgb(78,78,78) rim. So the recorder's primary action has a darker fill and a
dimmer rim than the two buttons that rename and manage an existing meeting; it
nearly disappears into the toolbar. Its label is rgb(122,55,48), a muted brick
red, on that rgb(43,43,43) fill — low contrast for the one word that names the
product's main verb. Red is also the colour this app should be reserving for
recording-in-progress and attention; on an idle screen a red "Record" reads
closer to stop than to start.

**Remedy.** Make it a filled primary control so it is the most prominent thing
in the chrome, and reserve red for the in-progress state so the button visibly
changes meaning when capture begins.

### 11. Settings is clipped mid-heading with no visible scroll affordance
`11-dark-settings.png`

The window ends immediately after the words "Note model" — a section heading cut
off with its content below the fold and no scrollbar or edge shading visible.
The brief requires that Settings show which model is in use; the speech model
does, and the note model may or may not, but the frame cuts before it.

Smaller, in the same frame: "Full model 1.61 GB" states the size in the row
title and again in the description ("using about 1.61 GB"). Say it once.

**Remedy.** Size the window to its content or make the scroll boundary visible.

### Surfaces that are fine

`05-dark-after-cancel.png` — md5-identical to `02-dark-note.png`. Cancel returns
cleanly and the meeting is intact. Correct.

`11-dark-settings.png` above the clip — the three privacy facts state what
leaves the Mac, where transcription runs, and what is kept, in concrete terms
and without a promise. No change needed.

`10-dark-transcript.png` below the header — timestamped turns with a speaker
column, a find field, and readable line lengths. Fine apart from finding 4.

---

## Q1 — the delete/lock button pairing

**Yes, and the current pairing is inverted on every axis I could measure.**

What is actually there, measured from the frames:

| | 04 delete | 06 lock |
|---|---|---|
| Confirm button fill | rgb(58,58,58) | rgb(59,130,247) |
| Confirm button rim | rgb(78,78,78), 1px | none |
| Cancel button | rgb(58,58,58), rgb(78,78,78) rim | rgb(58,58,58), rgb(78,78,78) rim |
| Title-box border | rgb(111,77,69) | rgb(111,77,69) |
| Dialog outer border | rgb(83,48,41) | rgb(177,178,176) |

Read that as a person, not as a spec. On the reversible sheet, one button is
obviously the one you came for. On the irreversible sheet, the two buttons are
the same object twice; you read the labels to tell them apart, and "Cancel" and
"Delete meeting" are both two-word phrases in the same weight at the same size.
Meanwhile the one visual cue that could mean danger — the warm red-brown
rgb(111,77,69) frame around the meeting name — appears on both sheets, so it
signals nothing. And the delete sheet's own outer border is the *darker* of the
two against the rgb(32,33,30) backdrop; the reversible dialog announces itself
more loudly than the destructive one.

**The axis, and it is not more weight.** Do not answer this by making "Delete
meeting" a red-filled primary. A red-filled primary invites the click, and on
macOS the destructive choice conventionally does *not* hold the emphasis. What
is missing is not weight, it is **differentiation plus a safe default**. Three
changes, in order of importance:

1. **Make Cancel the emphasized, default-focused button in the delete sheet.**
   Give Cancel the rgb(59,130,247) treatment that "Lock meeting" gets in `06`,
   so Return and the obvious click both land on the safe choice. This is the
   change that matters; the other two are supporting.
2. **Give "Delete meeting" a destructive label, not a destructive fill.** Keep
   the rgb(58,58,58) fill, set the label to the system red. It stays visually
   secondary while ceasing to be a Cancel look-alike. Deliberately choosing it
   should require reading it.
3. **Reserve the warm tint so it means something.** Strip the rgb(111,77,69)
   title-box border from `06` entirely — that sheet is reversible and should be
   neutral throughout — and raise the delete sheet's outer border to a saturated
   red with real contrast against rgb(32,33,30), so the frame itself announces
   which kind of sheet this is before the reader parses a word.

One thing I could not settle from a still: which button, if either, Return
currently commits. Neither carries a visible focus ring. That is itself a defect
worth fixing alongside the above — a confirmation sheet where the keyboard
default is invisible is a sheet you cannot safely dismiss without looking.

Also fold in finding 6. Whatever is done here, it is undermined while "Delete
transcript" in `03` sits one menu-item away with no ellipsis, apparently
committing with no sheet at all.

## Q2 — what 09 tells you, and what it should have

**What it tells you: nothing happened.** `09` is pixel-identical to `01` bar 18
antialiasing pixels on one row. Every word on it was already on screen before I
clicked:

- "Your meeting note needs another try."
- "Yawn could not create a note. Your transcript is unchanged and you can try again."
- the button, enabled, reading "Generate note"
- the detail line, "Runs the downloaded note model on this Mac. It usually takes several minutes, longer for long meetings. Nothing leaves your computer."
- the subtitle, "Sep 1, 2026 · Note not created"

That is the problem in one sentence: **a stale failure report and a fresh one
are the same string.** `01` was already saying a note could not be created, from
some earlier run. After my attempt the screen says exactly that again, so there
is no state expressing "the run you just started also failed", and no way to
tell one failure from two. Coming back after five minutes, I cannot tell whether
my click registered, whether it is still running, or whether it ran and failed —
and the button label still says "Generate note", which is the language of a
first attempt. The natural next move is to click it again and get the same five
minutes of nothing, with the same evidence.

Worse, the pre-existing copy is now actively misleading. "Yawn could not create
a note" was written about some earlier failure; after my attempt it reads as a
report on *my* attempt, which it is not, and it offers "you can try again" —
which is what I just did.

**What it should have told me,** in words on that screen:

- That an attempt was made, and when. "Tried at 9:30 AM."
- That it ended, and how. "The note model stopped without producing a note."
  Say plainly if the reason is unknown — "Yawn does not know why" is honest and
  better than silence.
- That the transcript is untouched. This is the one thing the current copy gets
  right; keep it.
- A button whose label reflects history: "Try again", not "Generate note".
- If more than one attempt has failed, say how many. Two silent five-minute
  failures should not look like one.

The brief's line is "An interrupted or failed run is stated plainly." This is a
failed run stated as nothing.

## Q3 — the transcript header break

**What is wrong.** The header row of the Source transcript card has collapsed.
Three elements are fighting for one line:

- The label "Source transcript" wraps to two lines in a column too narrow for
  it, and its second line sits below the row baseline.
- Two description blocks have collapsed to columns roughly 90px wide, wrapping
  at one to two words per line: "read the / complete / retained / conversation."
  and "9 / turns / are / cited / by / the / note."
- The three buttons — "Copy transcript", "Open transcript file", "Export
  meeting" — are painted **over the first line of both text blocks**, not
  beside them. "Copy transcript" fully covers the start of both sentences. The
  only surviving evidence of the covered text is a clipped "of" filling the narrow
  gap between "Copy transcript" and "Open transcript file".

**Would a reader recover the sentence? Half of it, and not the half that
matters.**

The first block is recoverable by inference: "…read the complete retained
conversation." is a complete-enough thought, and a word or two of preamble
hidden behind the button changes nothing a reader needs.

The second block is **not recoverable, and it is the one carrying the
information.** What is visible is "[hidden] of / 9 / turns / are / cited / by /
the / note." The sentence is a ratio — some number of 9 turns are cited — and
the numerator is behind "Copy transcript". A reader can be certain a count
exists and certain they cannot read it. Guessing is not recovery: "9 of 9" and
"2 of 9" are opposite statements about how well the note is grounded, and this
is precisely the number the brief makes non-negotiable, since every generated
claim needs a clear path back to the retained source text. The single surface
whose job is to report provenance coverage is hiding the coverage figure behind
a button.

Note the reading is also hostile before you get to the occlusion. One word per
line in a 90px column is slower to parse than a fragment, and the two columns
sit close enough to be read across rather than down, so a first pass yields
"read the 9 / complete turns / retained are / conversation. cited".

**Remedy.** Label on its own line, buttons on their own row beneath it,
description as one full-width sentence: "Read the complete retained
conversation. N of 9 turns are cited by the note." Nothing here needs a
multi-column layout, and no button should overlap text under any width.

I saw this at the app's default window size only, so I cannot say whether it is
width-dependent — see below.

---

## What I could not judge from these frames

Silence on any of the following is absence of evidence, not approval.

- **Light appearance.** Entirely absent. `07-appearance-override-ignored.png` is
  the tooling's own evidence that the override did not take, so nothing here
  speaks to the light palette, and several of my colour findings above
  (rgb(58,58,58) fills, the rgb(111,77,69) tint, the dotted underline's
  legibility) would need re-measuring there.
- **The entire capture flow.** The consent check, the headphone and
  single-operator attestations, the 1/7/30-day retention choice, the during-
  meeting note canvas, the visible recording state, and the stop control. Half
  the product's contract lives in these three moments and none of them is in
  this set.
- **First run and empty library.** The brief has a specific and unusual rule
  here — teach without seeding a fake meeting. No frame shows it.
- **Search.** `Search meetings` is never exercised; no results, no-match, or
  in-transcript match state.
- **Whether "Delete transcript" actually confirms.** The Manage popover was
  never actioned. Finding 6 rests entirely on the missing ellipsis — a
  typographic convention, not observed behaviour.
- **Rename**, and whether it can repair the transcript-fragment title in
  finding 5.
- **Trash and restore.** The delete sheet promises restore for 30 days; the
  Trash surface and the restore path are not shown, so I cannot confirm the
  promise is kept.
- **The locked state.** Both sheets were cancelled, so I never see a locked
  meeting, the Touch ID prompt, or how a locked row renders in the sidebar.
- **Settings below the clip** — the Note model section, and whether it is
  reachable by scrolling at all.
- **Keyboard behaviour.** Which button Return commits in either sheet, tab
  order, and whether Escape cancels. Inferred nothing from a still; the absence
  of a visible focus ring is what I report, not the behaviour.
- **Other window widths.** Everything here is the default size. Finding 4 in
  particular may be better or far worse at other widths, and I cannot tell which
  from one.
- **Accessibility.** No accessibility tree, so nothing on VoiceOver labels,
  roles, the unlabelled amber dot's announced state, or whether the dotted
  underline is exposed as an actionable element at all.
- **Contrast beyond the values I sampled.** I measured button fills, rims, and
  two borders. Body-text contrast, the muted greys in the transcript header, and
  the placeholder text were not measured.
- **Whether "Me" is the correct speaker label** for every turn in `10`, or
  whether other speakers exist and are being collapsed.
- **A successful note generation.** Every note-generating frame in this set is a
  failure or a pre-failure state, so I have not seen the surface the app is
  actually for: a complete note with decisions, follow-ups, and open questions
  populated. Finding 2 reports what this note renders; it cannot report whether
  the sections exist when the model succeeds.
