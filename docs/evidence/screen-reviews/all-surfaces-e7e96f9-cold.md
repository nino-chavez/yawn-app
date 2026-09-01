---
surface: all (01-09 roster)
build: e7e96f9
commit: 0155e2d (captures), cdeb599 (direction)
device: browser-render (stubbed bridge, synthetic fixtures) — PROVISIONAL, superseded by installed-app captures
reviewer: cold-reviewer agent (fresh session, captures + five job questions only)
implementer: W10 packet agent and prior waves
kind: cold
cold: true
states: first-run sheet, home-with-meetings, during-capture, after-note (default + scrolled), withheld-turn, needs-attention, settings-active-model, large-text, increased-contrast
verdict: 01 accept, 02 revise, 03 accept, 04 accept, 05 revise, 06 accept, 07 accept, 08 revise, 09 revise
---
# Cold Screen Review — Yawn

Reviewed at face value from ten static captures. No source, no roadmap, no assumptions about intent — only what each frame shows, judged against: what is happening now, what is next, who is involved, when and where, what can I do.

---

## Surface 1 — Home, First Run (01)

A modal sits over a blurred, empty home page. Modal text: eyebrow "BEFORE YOU RECORD," headline "A meeting has three moments," subhead "This appears once. It shows what happens before, during, and after you press Record." Three boxed rows follow — "BEFORE" (confirm participant consent, headphones, choose retention: 1, 7, or 30 days), "DURING" (keep your own notes in a calm canvas while Yawn records in the background), "AFTER" (generate a readable note with decisions and follow-ups, each pointing back to the transcript). Close X, top right; "Got it" button, bottom right. Behind the blur: "Capture th[e conversation]. Keep you[r own judgment]," and a partially legible "No meetings yet" empty state.

**Five questions.** (1) What's happening now — answered: a one-time explainer. (2) What's next — answered structurally by the three-phase list, though the actual next step ("press Record") is implied, not stated. (3) Who's involved — unanswered; "participant consent" is generic. (4) When/where — "on this Mac" answers where; no time. (5) What can I do — "Got it" or the X; nothing else in the modal is actionable.

**Eye path.** First: "A meeting has three moments" (largest, highest-contrast text). Second: the three BEFORE/DURING/AFTER rows, read top to bottom because of their boxed alignment. Third: "Got it," the only filled, colored control.

**What competes.** Little. The X and "Got it" both mean "dismiss," but they're differently weighted enough that this isn't real competition.

**Remove/combine/demote/disclose.** The three cards earn their place — they chunk three distinct moments and would lose clarity as one paragraph. The blurred background text is legible enough to invite the eye to try finishing "Capture th…" and "Keep you…," which is wasted effort for the reader; either blur it fully or don't blur it at all.

**Classification.**
- Headline, subhead — correct.
- Three timeline cards — correct.
- "Got it" — correct, sole clear CTA.
- X close — usable-but-weak (small, redundant, but a fine escape hatch).
- Partially legible blurred background — unnecessary (invites reading effort with no payoff).

**Verdict: accept.** A single-purpose onboarding card that tells a first-time user exactly what happens in three phases, with one obvious way out.

---

## Surface 2 — Home, With Meetings (02)

Nav: "Yawn," status pill "● Ready," "Meetings," "Record" (light-filled button), gear icon. Hero: eyebrow "NEW MEETING," headline "Capture the conversation. Keep your own judgment," body "A local recording, your private notes, and a transcript you can check when a detail matters." Right rail: salmon "Record" button + "⌘ R" chip, caption "Everything stays on this Mac. No account, bot, or automatic sharing." Below a divider: "Recent meetings" heading, search box "Find a meeting by title." Three rows: "Weekly sync — design review" / "The team agreed to ship the redesigned onboarding flow next sprint." / "Sep 1, 2026 · transcript available"; "Meeting · Aug 31, 2026" / "Budget approval is still pending finance sign-off." / "Aug 31, 2026 · transcript available"; "1:1 with a teammate" / "AUG 30, 2026 · LOCKED" (no summary line).

**Five questions.** (1) Now — the "● Ready" pill answers at a system level; the page itself isn't "doing" anything. (2) Next — answered clearly: Record, or open a meeting. (3) Who — unanswered anywhere; no names, only "a teammate." (4) When/where — dates given, no times, no meeting location/platform. (5) What can I do — fully answered: record, search, open a meeting, open Meetings, open Settings.

**Eye path.** First: the bold hero headline (largest, highest-contrast text on the page). Second: the salmon "Record" button — smaller than the headline but the only saturated color, and color beats size. Third: "Weekly sync — design review," the first list row.

**What competes.** Two "Record" buttons — one white-filled in the nav, one salmon-filled in the hero — do the same thing but look like two different priorities. A first-time viewer can't tell from styling alone which is "the" button.

**Remove/combine/demote/disclose.** One of the two Record buttons should be demoted; a returning user with meetings already listed doesn't need both a persistent nav button and a full pitch-plus-button hero every visit. The "⌘ R" shortcut chip is worth keeping — discloses the shortcut without cluttering the button. The search box, with three items, is harmless but premature at this volume.

**Classification.**
- Nav "Record" button — appealing-but-wrong (duplicates hero CTA, ambiguous priority).
- Hero headline/subhead — usable-but-weak (strong first-run copy, weaker value once real meetings sit right below it).
- Hero "Record" + ⌘R — correct.
- Privacy caption — correct.
- "Recent meetings" heading, search box — correct.
- Rows 1–2 — correct (each answers what/when).
- Row 3 ("1:1 with a teammate") — usable-but-weak: no summary sentence, no "transcript available," and it's unclear whether "LOCKED" replaces the summary or the summary simply failed to generate.

**Verdict: revise.** Two differently-styled Record buttons and one meeting row that breaks the pattern the other two rows establish leave a returning user guessing which control is primary and why one meeting looks different.

---

## Surface 3 — During Capture (03)

Nav: "Yawn," "● Recording" pill, gear icon. Eyebrow "RECORDING LOCALLY," headline "Stay in the conversation," subhead "Add a short note whenever something matters." Right: "Pause" (text) and "Stop recording" (salmon button). A "Recording" badge sits alone below the headline. Card: "ACTIVITY" / "Recording locally" / "Both audio sources are being captured on this Mac." / big "7:02" / "elapsed in this step." Field: "What is this meeting for? (optional)," right-aligned "Saved on this Mac," content "Decide whether to ship the onboarding redesign this sprint.," helper "Your context, used to guide the generated note. Not a transcript." Below: "Your notes" heading, content visible at the frame edge: "Ask about the Q4 budget line before we close."

**Five questions.** (1) Now — over-answered: five separate elements ("● Recording," "RECORDING LOCALLY," the "Recording" badge, "Recording locally" card title, and the 7:02 timer) all say the same thing. (2) Next — weakly answered: Stop/Pause exist, but nothing hints what happens right after stopping. (3) Who — unanswered. (4) When/where — 7:02 elapsed answers duration; no wall-clock time or platform. (5) What can I do — clearly answered: pause, stop, set meeting purpose, take notes.

**Eye path.** First: the salmon "7:02" timer and "Stop recording" button (color reads as live/urgent). Second: the headline "Stay in the conversation." Third: the "ACTIVITY" card body.

**What competes.** Four near-identical "recording is happening" signals leave nowhere singular for the eye to land — not competition so much as redundancy.

**Remove/combine/demote/disclose.** The loose "Recording" badge directly under the headline is pure repetition of the nav pill and the eyebrow above it — remove it. The "ACTIVITY" card should stay; it adds real information (both audio sources, elapsed time) the other three don't.

**Classification.**
- Nav pill, eyebrow, headline, subhead — correct.
- Pause, Stop recording — correct.
- Loose "Recording" badge — unnecessary (redundant).
- "ACTIVITY" card — correct.
- Meeting-purpose field + helper text disambiguating it from "Your notes" — correct; the helper line is doing real, needed work since the two free-text fields would otherwise look interchangeable.
- "Saved on this Mac" repeated on this field and again under "Your notes" — usable-but-weak (borderline repetition, see surface 4 for the fuller pattern).

**Verdict: accept**, with a trim. Working controls and clearly differentiated text fields are correct; the screen just states "recording" four times before reaching anything else.

---

## Surface 4 — After Note (04, 04b)

**04 (top of page):** "Back to meetings" link. Eyebrow "SAVED ON THIS MAC," title "Weekly sync — design review," "Rename" (text) / "Manage" (bordered button). Meta: "Sep 1, 2026 · Note" / "Recording audio is kept for 7 days." Card: "Listen to saved audio" / "Microphone and system audio are separate recordings." / "Play microphone" / "Play system audio" / "No recording is playing." "Meeting context": "Decide whether to ship the onboarding redesign this sprint." / "What the operator said this meeting was for, used to guide the generated note. Not a transcript." "Your notes" ("Saved on this Mac"): "Ask about the Q4 budget line before we close."

**04b (scrolled to the note):** Headline "What happened and what comes next" / "Generated from the transcript. Use the source links to check anything that matters." "Overview" — "The team reviewed the new onboarding flow and agreed on next steps." + "Show source." "Decisions" — "Ship the redesigned onboarding flow next sprint." + "Show source." "Follow-ups" — "Confirm budget sign-off with finance by Friday." + "Show source." "Open questions" — "Whether the redesign needs a second round of user testing." + "Show source." "Regenerate note" button. An illegible, cut-off line sits at the very bottom edge.

**Five questions.** (1) Now — answered: reviewing a saved, past meeting. (2) Next — weak on 04 (no clear action beyond playback); much better on 04b, where Decisions/Follow-ups read as next actions, though nothing lets you check one off. (3) Who — unanswered anywhere on this surface. (4) When/where — date given; "this Mac" restated twice. (5) What can I do — rename, manage, play mic/system audio separately, edit context, edit notes, read the note, click "Show source," regenerate.

**Eye path (04).** First: the bold title. Second: "Play microphone" / "Play system audio," the only bordered, clearly clickable shapes on the page. Third: the "Decide whether to ship…" sentence.

**Eye path (04b).** First: "What happened and what comes next," styled at the same weight as the meeting title on 04 — reads as a second page rather than a scroll position. Second: the four section labels down the left edge. Third: the blue "Show source" links, the only colored text, repeating four times.

**What competes.** "Rename" and "Manage" sit at similar visual weight near the title with nothing to signal that "Manage" is likely the more consequential (delete/export-leaning) action.

**Remove/combine/demote/disclose.** "Saved on this Mac" appears three times across this one surface — consolidate to one clear statement per page rather than reasserting it at every field. The illegible line truncated at the bottom of 04b is a defect: either it should be fully visible or it shouldn't be there as captured.

**Classification.**
- "Back to meetings," title, meta, retention line — correct.
- "Rename" — correct but under-differentiated from "Manage" (usable-but-weak).
- "Manage" — usable-but-weak (no label or icon hints what it actually does).
- "Listen to saved audio" card — correct, the clearest block on the surface.
- "Meeting context" + disambiguating caption — correct.
- "Your notes" — correct.
- Generated-note headline — usable-but-weak (visually competes with the meeting title as a second "big idea").
- Overview/Decisions/Follow-ups/Open questions structure — correct; the strongest content in the whole capture set.
- "Show source" links — correct, directly answers a real trust question.
- "Regenerate note" — correct.
- Truncated illegible footer text — defect.

**Verdict: accept**, pending two small fixes. The generated note with working "Show source" links is the best design in the set; the surface just repeats "saved on this Mac" past the point of registering and ends on an unreadable line.

---

## Surface 5 — Withheld Turn (05)

Card: "Run a local retry, then compare it with the retained transcript before deciding." / "Retry transcript" (link). "Full transcript" / "The retained record for checking a decision, owner, or follow-up." / "Open." Sub-panel "Source transcript" / "Search or read the complete retained conversation." / "3 of 6 turns are cited by the note." Link row: "Vocabulary," "Copy transcript," "Open transcript file," "Export meeting." Search box "Find in transcript" / "Find a phrase, decision, or follow-up." Transcript: a truncated blue bubble ("…flow next…"); "0:52 / Speaker 2" — "I'll chase finance on the budget line before Friday." — bubble "Follow-up: Confirm budget sign-off with finance by…"; "1:00 / Speaker 1" — "Do we need another round of testing before this ships?" — bubble "Open question: Whether the redesign needs a second…"; "1:13" — italic "This turn was withheld by the voice check." — "Restore this turn."

**Five questions.** (1) Now — partially answered: reviewing a transcript, one turn withheld by something called "the voice check," which is never defined on this screen. (2) Next — weak: "Restore this turn" is offered with no explanation of what restoring changes. (3) Who — better than other surfaces: "Speaker 1"/"Speaker 2" are present, though generic. (4) When/where — timestamps given (0:52, 1:00, 1:13); no wall-clock time. (5) What can I do — retry, open full transcript, search, view vocabulary, copy, open file, export, restore a withheld turn.

**Eye path.** First: "Retry transcript" (only colored text at the top). Second: the two round blue annotation bubbles — the only filled, colored shapes on the page. Third: "This turn was withheld by the voice check," distinctive by italics and the loaded word "withheld."

**What competes.** Five blue-link-styled actions ("Retry transcript" plus the four-item row) compete with no visual grouping to separate transcript utilities from a repair action. The round annotation bubbles are visually heavier than "Restore this turn," so the least consequential UI (routine citations) outweighs the most consequential one (recovering withheld content).

**Remove/combine/demote/disclose.** Fold "Vocabulary / Copy transcript / Open transcript file / Export meeting" into a single overflow control — four peer-level links in a flat row reads as an unsorted toolbar. "3 of 6 turns are cited by the note" is valuable and under-promoted; move it higher. The truncated bubble text ("…flow next…") is illegible as captured — don't truncate mid-clause.

**Classification.**
- "Retry transcript," "Full transcript"/"Open," "Source transcript" panel — correct.
- "3 of 6 turns are cited by the note" — correct but under-placed.
- Four-link utility row — usable-but-weak (functional, poorly grouped).
- Timestamps, speaker labels — correct.
- Round annotation bubbles — appealing-but-wrong (loudest element on the page despite being secondary, and illegibly truncated).
- "This turn was withheld by the voice check" — correct as a disclosure, but "the voice check" is undefined jargon on this screen.
- "Restore this turn" — correct as a control, underweighted for its stakes.

**Verdict: revise.** Disclosing a withheld turn honestly is the right instinct, but nothing on screen explains what "the voice check" is or what restoring does, and the least important elements are styled more boldly than the one action that actually matters here.

---

## Surface 6 — Needs Attention (06)

Nav: "Yawn," "● Needs attention" pill, gear icon. Centered card on an otherwise empty page: amber warning triangle. "NEEDS ATTENTION" / "Yawn cannot record yet." / "Yawn could not verify its local recording engine after the last restart." / "Check again" button.

**Five questions.** (1) Now — clearly answered, with a specific, checkable reason. (2) Next — answered narrowly: "Check again," with no second path if that fails. (3) Who — not applicable. (4) When/where — "after the last restart" gives rough causal timing, not a timestamp. (5) What can I do — only "Check again"; no logs, no manual permission path, no alternative.

**Eye path.** First: "Yawn cannot record yet." (largest, boldest, centered — nothing else competes). Second: the warning icon just above it. Third: "Check again," the only interactive element.

**What competes.** Nothing — this is the cleanest, least ambiguous surface in the set.

**Remove/combine/demote/disclose.** Nothing to remove; something is missing instead — no "what if this doesn't work" path (open logs, open System Settings, retry differently).

**Classification.**
- Nav pill, icon, eyebrow — correct.
- Headline, body sentence — correct (specific, not generic).
- "Check again" — usable-but-weak (a dead end if it fails again).

**Verdict: accept**, with one gap flagged. The clearest, best-written state in the set — specific, honest, one button — weakened only by having no second move if "Check again" doesn't fix it.

---

## Surface 7 — Settings, Active Model (07)

Eyebrow "YAWN," headline "Settings," subhead "Recording access and on-device storage live here." Card: "What happens with your meeting" / "Three facts about this Mac, not a policy promise." Row: "What leaves this Mac" / "Nothing. Recording, transcription, and notes run and stay here. Yawn has no account and makes no network calls with meeting data." Row: "Where transcription runs" / "On this Mac, using the local speech model shown below." Row: "What is kept, and for how long" / "Transcript and notes: until you delete them (deleted meetings stay in Trash for 30 days). Recording audio: 1, 7, or 30 days — you choose at the start of each meeting." Second card: "Speech model" / "Choose the model Yawn uses for transcripts. Meeting audio stays on this Mac." Row: "Compact model" with badge "In use" / "Smaller download, less disk space." (Content continues past the visible frame.) Note: this capture's palette (a mint/green cast to the near-black background and accent color) reads differently from the salmon/blue accents on every other surface in this set.

**Five questions.** (1) Now — answered: reviewing privacy facts and the active transcription model. (2) Next — not really applicable to a settings page. (3) Who — not applicable. (4) When/where — the strongest "where" answer in the whole set: explicit, itemized statements about what leaves the Mac, where transcription runs, and exact retention windows. (5) What can I do — only implied (presumably choosing a different model further down); nothing interactive is confirmed in the visible frame besides the "In use" badge state.

**Eye path.** First: "Settings" headline. Second: "What happens with your meeting" card title. Third: the three bolded row labels, each answered in the sentence beneath.

**What competes.** Little. One soft tension: "Three facts about this Mac, not a policy promise" pre-emptively names the reader's potential distrust, which is memorable but risks drawing attention to the doubt it's trying to defuse rather than simply reassuring.

**Remove/combine/demote/disclose.** Nothing to cut — this is dense, well-organized information appropriate to a settings/privacy screen, unlike the repeated "Saved on this Mac" badges scattered elsewhere.

**Classification.**
- Headline, subhead, three fact rows — correct (the strongest privacy communication in the set).
- "Three facts about this Mac, not a policy promise" — usable-but-weak (clever, but risks reading as defensive).
- "Speech model" card, "In use" badge — correct.

**Verdict: accept.** The most trustworthy-reading screen in the app: it itemizes exactly what leaves the Mac, where transcription runs, and how long everything is kept, in plain declarative sentences instead of a vague policy link.

---

## Surface 8 — Accessibility States of Surface 2 (08 large text, 09 increased contrast)

**08 (large text).** Same content and layout as surface 2, at visibly larger type — the hero headline now wraps across more lines, and the third meeting row ("1:1 with a teammate") is pushed to or past the bottom edge of the visible frame.

**09 (increased contrast).** Same content and layout as surface 2. Judged directly against 02 at face value, no legible difference is visible — border weight, text contrast, and the salmon button color all read the same as the default state.

**Five questions.** Same answers as surface 2 for both states, with one exception: in 08, "what can I do" is degraded, because the third meeting is no longer confirmed visible without scrolling — the opposite of what an accessibility setting should do.

**Eye path.** Same order as surface 2 in both cases (headline, then salmon Record button, then first list row); 08 simply consumes more vertical space getting there.

**What competes.** Same duplicate-Record-button issue as surface 2, made proportionally more costly in 08, where enlarged nav and hero elements eat more of the limited vertical space.

**Remove/combine/demote/disclose.** For 08 specifically: a more compact row treatment at large text sizes would keep more of "Recent meetings" visible without scrolling — a targeted fix, not a general layout change. For 09: there is no structural finding beyond the absence of a visible difference, which is itself the finding.

**Classification.**
- All elements inherit their surface-2 classification.
- 08 adds one new defect: the third meeting row is pushed past the visible fold at this text size.
- 09 adds one new defect: the "increased contrast" toggle produces no legible visual change against the same frame at default settings.

**Verdict (08): revise.** Large text doesn't break anything, but it pushes the third recent meeting past the fold without adjusting row density.

**Verdict (09): revise.** Judged strictly against the same frame at default settings, increased contrast is visually indistinguishable from the baseline — a failure for a toggle whose whole job is to look different.

---

## Three most consequential findings, ranked

1. **The increased-contrast accessibility state (09) shows no visible change from the default state (02).** At face value, comparing the two frames directly, borders, text weight, and color all read the same. An accessibility toggle that doesn't visibly do what it claims fails exactly the reader it exists to serve, and does so silently — nothing on screen tells that reader the setting isn't working.

2. **"The voice check" is used as load-bearing, undefined jargon on the withheld-turn screen (05).** The app is making an unusual and important claim — that it screened and withheld part of a transcript — and gives the reader no definition, no reason, and no stated consequence for "Restore this turn." This sits at the center of the app's own trust proposition (a checkable transcript with sourced claims), so unclear language here undercuts the strongest idea in the whole set (the sourced Overview/Decisions/Follow-ups note on 04b).

3. **Two differently-styled "Record" buttons appear together on the home screen (02, 08, 09), and the third meeting row breaks the pattern the other two establish.** A new user faces two visually distinct controls that do the same thing with no stated reason to prefer one, and a returning user sees one meeting row ("1:1 with a teammate · LOCKED") that drops the summary sentence every other row has, with no way to tell whether that's a status or a missing feature.
