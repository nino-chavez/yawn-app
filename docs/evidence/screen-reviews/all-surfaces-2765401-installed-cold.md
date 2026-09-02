---
surface: all (2765401-installed set 01-07)
build: 2765401
commit: 2765401 (concept-A rebuild: 07a5528 UI, f5688c7 shell, a71786f tokens, 166eb96 and 2765401 attention-pane fixes)
device: this Mac, packaged Yawn Preview.app, real storage with four real meetings; dark and light appearance
reviewer: blind reviewer agent (fresh session; seven frames and the protocol only)
implementer: impl-ui-rebuild, impl-shell, impl-tokens worktree agents; captures and fixes by the rethink session
kind: cold
cold: true
release_marker: rethink-phase3-rebuild-2026-09-02
states: launch (library with most recent selected), recovered-interrupted meeting (reader failure), settings; the same in light; first run
verdict: 01 revise, 02 revise, 03 revise, 04 revise, 05 revise, 06 revise, 07 revise
not_reviewed: during, paused, meeting with generated note and inspector (harness-only), withheld turn, large text, increased contrast
protocol: judged-screen pattern section 3a, five job questions, plus the category read
---
# Cold Screen Review — installed build

## 01-dark-launch

Appearance: Dark three-pane window. Left sidebar lists meetings grouped under "YESTERDAY" (three rows, one selected in blue) and "PREVIOUS 30 DAYS" (one row). Right pane shows a selected meeting's detail: title, "Rename"/"Manage" buttons, status line, a paragraph of meta-narration about the recording, a large "No meeting note yet." headline, a blue "Generate note" button, an empty "Meeting context" section, an empty "Your notes" text area, and a bordered "Full transcript" card with an "Open" link. Top bar has a centered window title, a search field, and a red "Record" button.

- What is happening now? Not answered as a live status — this is a past, already-recorded meeting with no note generated yet.
- What happens next? Implied: generate a note, or do nothing further. No scheduled/upcoming meeting is shown.
- Who is involved? Not answered — no participant names or count anywhere on screen.
- When and where is it? When: "Sep 1, 2026" (date only on this pane; time is in the sidebar row, 8:55 AM). Where: not answered.
- What can I do right now? Generate note, rename, manage, add personal notes, open the full transcript, or start a new recording via the top-right Record button.

Eye lands first on "No meeting note yet." — it is the largest, boldest text on the screen. It competes with the red "Record" button at the top right, which is the only saturated red on screen and pulls attention away from the content column.

## 02-dark-recovered

Appearance: Same dark sidebar, different row selected (8:48 AM, "note only"). The entire right pane is empty except for a text block roughly mid-height: "This meeting is unavailable." followed by an explanation and a blue "Back to meetings" button. The top title bar still reads "Meeting · Sep 1, 2026."

- What is happening now? A meeting failed to load; content could not be read.
- What happens next? Text says "Reopen Meetings to try again," but the only visible control is "Back to meetings," which does not match that instruction.
- Who is involved? Not answered.
- When and where is it? Not answered on this pane (time is visible only in the sidebar row behind it).
- What can I do right now? Click "Back to meetings" — the one button on screen — though the body copy points to a different action ("reopen").

Eye lands first on "This meeting is unavailable." — it sits alone in a large empty pane. It competes with the title bar above it, which still names a specific meeting as though it loaded successfully, contradicting the error message directly beneath it.

## 03-dark-settings

Appearance: Separate dark window, "Yawn Settings" title, disabled (gray) zoom button. "Settings" heading and subhead. First card: "What happens with your meeting" with three sub-rows (what leaves this Mac, where transcription runs, what is kept and for how long). Second card: "Speech model" with a "Couldn't check speech model" row carrying an amber "Unavailable" pill, plus a duplicate amber-colored error line repeating the same sentence. A "Note model" card begins at the bottom, cut off.

- What is happening now? The app cannot verify which speech model is installed; it is retrying.
- What happens next? Not answered — no visible retry countdown, retry button, or resolution path beyond the word "Retrying…".
- Who is involved? Not applicable — settings screen.
- When and where is it? Not applicable.
- What can I do right now? Nothing offered for the model error beyond reading it; no "retry now," "choose a different model," or "learn more" control is visible for that specific failure.

Eye lands first on the amber "Unavailable" pill — the only saturated color on the screen. It competes with the amber error text directly below it, which restates the same fact a second time.

## 04-light-launch

Appearance: Identical layout and content to 01, rendered light: white/near-white cards on a light gray page, dark text, same red Record button and blue Generate note button.

- What is happening now? Same as 01 — not answered as live status; a past meeting with no note yet.
- What happens next? Same as 01 — generate a note is the offered path.
- Who is involved? Not answered.
- When and where is it? Same as 01 — date only, no location.
- What can I do right now? Same list as 01.

Eye lands first on "No meeting note yet." — same hierarchy problem as 01, unchanged by the appearance switch. It competes with the red Record button, which reads slightly softer against the light toolbar than it did in dark.

## 05-light-recovered

Appearance: Identical to 02, rendered light. Same empty pane, same "This meeting is unavailable." text block, same "Back to meetings" button, same stale title bar.

- What is happening now? Same failure as 02.
- What happens next? Same instruction/control mismatch as 02.
- Who is involved? Not answered.
- When and where is it? Not answered on this pane.
- What can I do right now? Same single button as 02.

Eye lands first on the heading text, same as 02. It competes with the same stale, contradictory title bar.

## 06-light-settings

Appearance: Identical to 03, rendered light. White cards read more distinctly against the light gray page than the dark cards did against near-black.

- What is happening now? Same model-check failure as 03.
- What happens next? Same — no resolution path shown.
- Who is involved? Not applicable.
- When and where is it? Not applicable.
- What can I do right now? Same as 03 — nothing offered for this specific error.

Eye lands first on the amber "Unavailable" pill, same as 03. It competes with the duplicate amber sentence below it — the repetition is if anything easier to spot here because the card boundary is more visible in light mode.

## 07-dark-first-run

Appearance: Dark main window, blurred and dimmed behind a centered modal dialog titled "Before, during, after." with an X close control and a light "Got it" button. The blurred background is legible enough to make out a heading and a "Try it now with a 30-second demo on yourself" line underneath it.

- What is happening now? First-run orientation: the app is explaining its three-phase model (before/during/after a meeting).
- What happens next? Dismiss the dialog ("Got it") and, per the blurred background text, optionally try a 30-second demo recording.
- Who is involved? Not answered.
- When and where is it? Not answered — this is orientation copy, not a specific meeting.
- What can I do right now? Dismiss via "Got it" or the X; nothing else is reachable while the modal is up.

Eye lands first on the bold heading "Before, during, after." It competes with the partially legible blurred text behind the dialog, which is distracting because it's readable enough to invite a second read rather than receding into pure backdrop.

---

## 1. Information hierarchy

Screen 1/4's largest, boldest text is "No meeting note yet." — bigger and heavier than the meeting's own title above it. An absence state is outranking the meeting's identity, date, and content. The meta-narration paragraph ("Recording was paused 1 time…", "The audio was already deleted…") sits in plain gray body copy with no visual priority marker, so a fact that changes what the user can do (this meeting can never be retranscribed) carries the same weight as routine status text. In Settings (3/6), the true structural hierarchy — page title, card title, row title — is legible, but a single fact (model check failed) is asserted three times at three different visual weights within one card, which flattens rather than reinforces hierarchy.

## 2. What competes

On 01/04, the blue "Generate note" button and the red "Record" button are both saturated, both near the top of the window, and both plausible "primary action" candidates with no stated relationship between them. On 02/05, the title bar ("Meeting · Sep 1, 2026") and the content pane ("This meeting is unavailable.") directly contradict each other in the same screen. On 03/06, the amber pill and the amber sentence beneath it compete for the same attention by repeating the same message rather than by presenting distinct information.

## 3. Subtractive pass

- Collapse the duplicated speech-model error (03/06) into one message; the row's own "Retrying…" text and the standalone amber sentence say the same thing twice.
- Reconcile or remove the "Reopen Meetings to try again" instruction on the error screen (02/05) since the visible button does something else; keep whichever instruction matches the actual control.
- Drop or reset the title bar on the error screen so it doesn't keep naming a meeting that failed to load.
- The "Stored on this Mac" label on the notes section and the notes footer disclaimer ("Saved separately from the transcript…not generated claims.") repeat privacy/storage promises already made in Settings' "What leaves this Mac" card — this is stated in three separate places across the set (meeting detail body text, notes footer, Settings).

## 4. Interaction semantics

"Rename" and "Manage" (01/04) are styled as equal-weight buttons, but one is a narrow text edit and the other implies a broader menu of actions — same visual weight for different-risk operations. "Open" next to "Full transcript" is plain trailing text with no button styling, chevron, or underline — easy to miss as a control at all, unlike every other actionable element on the same screen. The error screen's only button ("Back to meetings") does not match the instruction text above it ("Reopen Meetings to try again"), so a user reading top-to-bottom is told to do one thing and shown a control that does another. The Settings window's disabled (gray) zoom button correctly signals a fixed-size panel — that one reads correctly.

## 5. Copy

Privacy/local-storage language recurs across three separate surfaces with near-identical wording ("Nothing leaves your computer" / "Stored on this Mac" / "Nothing leaves this Mac... no network calls with meeting data") — it reads as disclaimer text embedded directly in working UI rather than confined to one place. "No admitted note is available." uses "admitted," an unusually legalistic word for a note that simply hasn't been generated yet. The Settings card intro line "Three facts about this Mac, not a policy promise" is itself explaining the app's own rhetorical stance to the user, which is copy about copy. "Nothing already saved here was replaced" (02/05) is a double-negative construction that takes a second read to parse. The speech-model failure is stated three times in adjacent lines within one card (section label "Couldn't check speech model," body "Yawn could not check the saved speech model. Retrying…," and a third standalone repeat of the same sentence).

## 6. Typography, color, spacing, shape, depth

One system sans throughout, consistent bold/regular contrast between headings and body. Corner radii on buttons and cards are moderate and consistent across both appearances. There is no elevation system — no shadows anywhere; structure is carried entirely by 1px hairline borders. In dark mode (03), that border is very low-contrast against the near-black page, so the "card" barely reads as a distinct surface from its background; the same border in light mode (06) is crisp because the card is white against light gray. Color is used semantically (blue = primary action / selection, red = record, amber = warning) but inconsistently applied: of the five interactive verbs on screen 1 (Rename, Manage, Generate note, Open, Record), only two carry color, so a viewer can't rely on color alone to find every actionable element.

## 7. Window and chrome

The main window is a standard sidebar-plus-detail, two-pane layout with a centered title, a search field, and one colored action button in the top bar — a familiar document-browser skeleton. Settings opens as its own separate native window with its own traffic lights and a correctly disabled zoom control, rather than as a sheet or panel attached to the main window. The toolbar is sparse — text controls and one button, no icon toolbar. On width: the detail pane's content sits in a fixed-width column with a wide unused margin to the right of the window (visible across 01/04's full capture) with nothing filling it — no secondary panel, summary, or wider layout adapts to the available space.

## 8. Empty and failure states

"No meeting note yet." (01/04) is a reasonably complete empty state: it explains why, and offers one clear recovery action. "This meeting is unavailable." (02/05) is the weaker of the two — no icon, positioned in a way that reads as leftover single-column text rather than a designed state (it sits off-center in a large empty pane rather than centered to it), and its instruction text conflicts with the one button provided.

## 9. Sheets and secondary windows

Two secondary surfaces appear: the Settings window (a full native window) and the first-run coach-mark (07), which uses a centered card over a dimmed, blurred backdrop with an X and a "Got it" button — a modal-dialog pattern rather than a native sheet dropping from the title bar or a popover anchored to a control.

## 10. Dark versus light

Content and layout are identical between the two appearances (01/04, 02/05, 03/06) — no copy or component changes. Dark is the weaker of the two specifically in card/border legibility: Settings' card boundary (03) is nearly invisible against the near-black page, while the same boundary in light (06) reads clearly white-on-gray. Neither appearance reads as a plain fallback of the other — both got matching background/foreground treatment — but the dark-mode card-contrast gap is a real, measurable weakness, not a rendering artifact.

## 11. Category read

Reads native: real traffic-light window chrome, a correctly disabled zoom button on a fixed-size settings window, a sidebar+detail pane structure, a native-style search field, one system font throughout, hairline dividers between list rows. Reads as a web page in a window: the first-run dialog (07) is a centered card over a dimmed/blurred backdrop with an X and a light pill button — a web-onboarding-modal pattern rather than a native sheet or popover. The flat, shadowless, hairline-bordered "cards" throughout Settings read like bordered divs on a web dashboard rather than macOS's grouped-list or inset-table materials — there's no vibrancy, translucency, or grouped-table styling anywhere in the set. The "Open" link next to "Full transcript" is styled as plain trailing text, a web link convention, where a native control would more likely use a chevron or a button. Overall: the window mechanics read native; the interior surface treatment reads web-in-a-window.

## 12. Element classification

- Traffic-light window controls — correct
- Sidebar toggle icon — correct
- Centered window title — usable but visually weak (goes stale/contradictory on the error screen)
- Search meetings field — correct
- Record button (red) — correct
- Sidebar meeting list rows — usable but visually weak (three rows nearly indistinguishable except timestamp and one status word)
- Selected row highlight (blue) — correct
- Sidebar section headers ("YESTERDAY," "PREVIOUS 30 DAYS") — correct
- Meeting title heading in detail pane — usable but visually weak (outranked by the empty-state headline below it)
- Rename button — correct
- Manage button — usable but visually weak (same visual weight as Rename despite presumably broader scope)
- Meta-narration paragraph (paused/audio deleted) — usable but visually weak (buried in plain body text despite consequential content)
- "No meeting note yet." headline — appealing but functionally wrong (largest, boldest text on the screen for a null state)
- Generate note button — correct
- Meeting context (empty) section — correct
- Your notes text area and placeholder — correct
- "Stored on this Mac" label — unnecessary (repeats a promise made elsewhere)
- Notes footer disclaimer — unnecessary at this location (duplicate of Settings copy)
- Full transcript card — correct
- "Open" link — usable but visually weak (no button/chevron styling)
- "This meeting is unavailable." heading — defect in placement, correct as a message
- Error body copy ("Reopen Meetings to try again") — defect (doesn't match the visible control)
- Back to meetings button — correct
- Settings heading and subhead — correct
- "What happens with your meeting" card — correct
- Speech model "Unavailable" pill — correct
- Duplicate amber error sentence — defect (restates the row above it)
- First-run dialog heading and body — correct
- Got it button — correct
- X dismiss control — correct
- Blurred background text behind the first-run dialog — defect (legible enough to distract rather than recede)

## Verdicts

- 01-dark-launch: revise — hierarchy inversion (empty-state headline outranks meeting title), redundant privacy copy.
- 02-dark-recovered: revise — error layout is undesigned, instruction text contradicts the one button, stale title bar.
- 03-dark-settings: revise — same error message repeated three times, near-invisible card border in dark mode.
- 04-light-launch: revise — same hierarchy inversion as 01.
- 05-light-recovered: revise — same instruction/control mismatch as 02.
- 06-light-settings: revise — same duplicated message as 03 (card contrast itself is fine in light).
- 07-dark-first-run: revise — background bleed-through behind the modal is distracting; message and controls are otherwise clear.

## Three findings to fix first

1. **The empty-state headline outranks the meeting itself.** "No meeting note yet." (01/02/04/05... specifically 01 and 04) is styled larger and bolder than the meeting's own title, so opening a past meeting the user should be able to scan in seconds instead leads with an absence. Demote it to a secondary weight and let the meeting's title/date remain the largest element on the pane.
2. **The failure screen's instructions don't match its own button.** On 02/05, the body copy says "Reopen Meetings to try again," but the only control on screen says "Back to meetings" and navigates rather than reopening anything — and the title bar keeps naming the meeting that just failed to load, contradicting the message directly below it. Align the copy to the actual control and reset the title bar on failure.
3. **Sidebar rows are indistinguishable at a glance.** Three consecutive entries all read "Meeting · Sep 1, 2026" and differ only by a timestamp and one status word ("transcript available" vs. "note only"), so telling meetings apart requires reading fine print on every row — one entry lower in the same list already carries a real, distinguishing title, showing this is achievable. Give meetings a real title by default, or add a stronger visual differentiator than timestamp alone.

## What a stronger version would feel like

A stronger version would let the meeting itself — its title, date, and status — stay the single largest thing on the screen no matter what state that meeting is in, so absence, failure, and success all read as *modifiers* on a stable identity rather than headlines that replace it; the sidebar would let a user tell meetings apart without reading timestamps one by one; every error would say exactly what its one visible button does and nothing else; and the app's local-only, no-account privacy story would live once, prominently, in Settings, instead of being re-asserted in three different corners of the working UI as if it needed re-justifying every time the user opened a meeting.

## Not reviewed

Recording in progress, a meeting with a generated note actually rendered, large text / increased contrast accessibility modes, and any state showing real participant or location data were not captured in this set and are not evaluated here.
