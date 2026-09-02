---
surface: all (bfa0a80-installed set 01-10)
build: bfa0a80
commit: bfa0a80 (captures and direction)
device: this Mac, packaged Yawn Preview.app, real storage with four real meetings; dark and light appearance
reviewer: blind reviewer agent (fresh session; ten frames and the protocol only; the tool denied any other read)
implementer: waves through D-LOCK fix; captures by the rethink session
kind: cold
cold: true
release_marker: rethink-phase1-2026-09-02
states: first-run sheet, first-run empty, launch view (terminal transcript-ready), library, meeting recovered-interrupted, meeting with pause receipt, settings; launch view, library, meeting in light
verdict: 01 revise, 02 revise, 03 revise, 04 revise, 05 revise, 06 revise, 07 revise, 08 accept, 09 revise, 10 revise
not_reviewed: during, paused, withheld turn, needs-attention (no data or operator-run), large text, increased contrast, settings in light (captured as 11 after the review started)
protocol: judged-screen pattern section 3a, five job questions, plus a category-read section added for the rethink (does the window read as an installed Mac app of its kind or as a web page)
---
# Cold Screen Review — installed build

## Not reviewed
Accessibility states (large text, increased contrast) were not captured in this set. No findings below speak to them.

## Per-image walkthrough

### 01 — dark-first-run-sheet
A centered modal sheet, dimmed backdrop, over a blurred launch screen. Eyebrow "BEFORE YOU RECORD," headline "A meeting has three moments." Three bordered cards labeled BEFORE / DURING / AFTER, each with one paragraph. "Got it" button bottom right, an X close in the top right of the sheet.

- What is happening now: a one-time onboarding sheet explaining three phases of a meeting is open.
- What happens next: dismiss via "Got it" or the X, returning to the screen behind it.
- Who is involved: not answered.
- When and where: not answered.
- What can I do right now: read the three cards, click "Got it."
- Eye lands first on: the bold headline "A meeting has three moments."
- Competes with it: the blurred background text ("Capture th... Keep your...") is legible enough through the dim overlay to pull the eye behind the modal.

### 02 — dark-first-run-empty
Main window, no modal. Left: eyebrow "NEW MEETING," large headline "Capture the conversation. Keep your own judgment.," a one-line subtitle. Right: a filled "Allow system audio" button and a caption below it. Below a divider: "Recent meetings" heading, "No meetings yet" empty state with instructional text, a greyed hint line.

- What is happening now: no meeting recorded yet; setup shows microphone access ready.
- What happens next: not directly answered on screen.
- Who is involved: not answered.
- When and where: not answered.
- What can I do right now: click "Allow system audio." The empty-state text says "Press Record to start a private meeting," but no Record control is visible anywhere on this screen.
- Eye lands first on: the large bold headline.
- Competes with it: the filled "Allow system audio" button, the only high-contrast filled shape on the page, pulls focus toward a one-time setup action rather than the empty state below it.

### 03 — dark-launch-view
Despite the filename, this screen shows a post-recording state: eyebrow "TRANSCRIPT READY," headline "Your meeting is ready to read." Top right: "Back to Meetings" text link and a filled "Record another meeting" button. Below: a "Transcript Ready" pill badge, an empty "What is this meeting for? (optional)" textbox, and the top of a "Your notes" box with placeholder text.

- What is happening now: a transcript has just been produced for an unnamed meeting.
- What happens next: go back to Meetings, record another meeting, or add context/notes.
- Who is involved: not answered.
- When and where: not answered on this panel.
- What can I do right now: type meeting purpose (optional), write notes, or navigate away via the two top-right controls.
- Eye lands first on: "Your meeting is ready to read."
- Competes with it: the solid white "Record another meeting" button, the brightest object on the screen.

### 04 — dark-library
Same launch template as 02, but "Recent meetings" is now populated with four rows: three dated Sep 1, 2026 (one "transcript available," two "note only") and one dated Aug 10, 2026 whose title is a raw sentence fragment, "um, rule-based like system where it's like these products and these fields," marked "transcript available." A search box "Find a meeting by title" sits above the list.

- What is happening now: four past meetings are listed; the new-meeting panel still occupies the top of the screen.
- What happens next: not clearly signaled.
- Who is involved: not answered — rows show only generic "Meeting" or a transcript fragment, no names.
- When and where: dates are shown; location not answered.
- What can I do right now: click a row to open it, or search by title.
- Eye lands first on: the marketing headline, still dominant at the top.
- Competes with it: the sentence-fragment title in the list — it is the longest, boldest line in the list and reads as an anomaly.

### 05 — dark-meeting-a
Meeting detail page. "Back to meetings" link, eyebrow "SAVED ON THIS MAC," large title "Meeting · Sep 1, 2026," "Rename" / "Manage" buttons top right. Subtitle "Sep 1, 2026 · Transcript Only" / "Meeting audio is retained on this Mac." An amber banner: "Yawn could not verify whether this recording was paused." Left: an amber "MEETING NOTE" card, "No meeting note yet." / "No transcript was created for this retained meeting." Right: "Listen to saved audio" card (Play microphone / Play system audio, "No recording is playing"), "Meeting context" ("No pre-meeting context was written for this meeting."), and a "Your notes" card with placeholder text.

- What is happening now: viewing a saved meeting with no note and an unresolved integrity warning.
- What happens next: not clearly signaled — no visible action addresses the warning, and no "Generate note" button appears (transcript wasn't created for this one).
- Who is involved: not answered.
- When and where: Sep 1, 2026; location not answered.
- What can I do right now: rename or "Manage" the meeting, play saved audio, or write notes.
- Eye lands first on: the amber warning banner — its color makes it the loudest element on the page.
- Competes with it: the large "Meeting · Sep 1, 2026" title directly above it, equally bold but out-ranked in color salience.

### 06 — dark-meeting-b
Same layout as 05, different content. Banner (same amber styling): "Recording was paused 1 time, for 0:03 in total. Nothing was captured during that gap." "MEETING NOTE" card: "No meeting note yet." / "No admitted note is available. Retained transcript text remains available." Below it, a black "Generate note" button with a caption ("Runs the downloaded note model on this Mac... Nothing leaves your computer."), then a "Transcript retry" card ("Run a local retry, then compare it with the retained transcript before deciding," with a "Retry transcript" link) and a "Full transcript" card with an "Open" link. Right column unchanged from 05.

- What is happening now: viewing a saved meeting with a transcript retained but no note generated yet.
- What happens next: generate a note, retry the transcript, or open the full transcript.
- Who is involved: not answered.
- When and where: Sep 1, 2026; location not answered.
- What can I do right now: click "Generate note," "Retry transcript," or "Open."
- Eye lands first on: the amber banner again, by color.
- Competes with it: "No meeting note yet." headline and the black "Generate note" button below it both compete for the primary-action role.

### 07 — dark-settings
Separate window, title bar reads "Yawn Settings"; only the red (close) traffic light appears active, the other two read disabled/grey. Background is a dark green-tinted panel, distinct from the near-black of the main app. Green eyebrow "YAWN," very large headline "Settings," subtitle "Recording access and on-device storage live here." A card: "What happens with your meeting" / "Three facts about this Mac, not a policy promise," with three subsections — "What leaves this Mac" (nothing), "Where transcription runs" (on this Mac), "What is kept, and for how long" (retention terms). A second card: "Speech model," with a "Checking speech model" row and a "Checking" pill.

- What is happening now: the Settings window is open, explaining data handling; a local speech-model check is in progress.
- What happens next: the model check presumably resolves ("Checking" implies pending).
- Who is involved: not answered.
- When and where: not answered.
- What can I do right now: read the three data-handling facts; nothing else is actionable while the model check is pending.
- Eye lands first on: "Settings" — the largest type in the entire review set.
- Competes with it: the green "YAWN" eyebrow above it, an accent color that appears nowhere else in the set, briefly reads as a separate brand mark.

### 08 — light-launch-view
Light-mode version of 03's content (transcript-ready, empty notes). Off-white background, dark text, "Record another meeting" now rendered black-on-light.

- What is happening now / next / who / when-where: same as 03.
- What can I do right now: same options as 03.
- Eye lands first on: "Your meeting is ready to read."
- Competes with it: the solid black "Record another meeting" button — in light mode it is the darkest object on the page, even more attention-grabbing than its dark-mode counterpart.

### 09 — light-library
Light-mode version of 04's content (populated meetings list). Same four rows, same sentence-fragment title, same search box, now on an off-white background.

- Same answers as 04.
- Eye lands first on: the marketing headline.
- Competes with it: the sentence-fragment title, now in full dark-on-light contrast — the blackest, longest line in the list.

### 10 — light-meeting-b
Light-mode version of 06's content. The warning banner is now pale cream/yellow instead of dark amber; "Generate note" is a solid black button.

- Same answers as 06.
- Eye lands first on: "Meeting · Sep 1, 2026" — in light mode the pale banner no longer out-ranks the title in salience, so the title wins instead.
- Competes with it: the black "Generate note" button.

---

## 1. Information hierarchy
The marketing headline ("Capture the conversation. Keep your own judgment.") holds the most visually dominant position — top-left, largest bold type — on every "home" screen, including the two where meetings already exist and are listed below it (04, 09). It never yields ground once the list is populated. On the meeting-detail screens (05, 06, 10), the amber/cream status banner out-ranks the page's own title in color salience even though the title is set in larger type — color wins over size. Settings (07) is the one screen with a clean, single hierarchy: eyebrow, then a giant title, then subtitle, then grouped facts.

## 2. What competes
- Headline vs. "Allow system audio" button (02, 04, 09): the setup button is the strongest filled shape on the page, but it's a one-time permission action, not the recurring task.
- Warning banners vs. page titles (05, 06, 10): the amber/cream banner visually outranks the meeting's own title.
- The transcript-fragment title (04, 09) vs. its neighboring "Meeting · [date]" rows: it is the longest, boldest line in the list purely because no title was generated for it, and it draws the eye because it looks broken, not because it's important.
- "Record another meeting" (08) and "Generate note" (06, 10): both solid buttons compete with the headline directly above or beside them for the eye's first stop.

## 3. Subtractive pass
- The onboarding sheet's three cards (01) could collapse to a shorter passage or an inline three-step strip; three full bordered boxes at near-body size is heavy for a one-time dismissible explainer.
- "Meeting context" plus its own empty-state line (05, 06, 10) duplicates the "What is this meeting for? (optional)" field shown on the transcript-ready screen (03, 08) — same concept, two labels, two presentations. Pick one.
- "Listen to saved audio" splits microphone and system audio into two buttons plus an explanatory caption — three lines of UI for what reads as one idea (play the recording).
- The marketing headline on the populated list (04, 09) could be demoted or removed once meetings exist; it's onboarding copy that outstays its welcome.
- The "Transcript Ready" pill (03, 08) restates a fact already carried by the top-right status dot ("Ready to read") and the section headline ("Your meeting is ready to read") — the same status stated three times.

## 4. Interaction semantics
- "Allow system audio" (02, 04, 09) is styled as the strongest, most clickable-looking button on the page, but its own caption reads "Microphone access is ready" — it's unclear whether the button grants something or is just a status chip dressed as a button.
- "Manage" next to "Rename" (05, 06, 10) gives no indication of scope — delete, export, and retention override are equally plausible from the label alone.
- "Retry transcript" (06, 10) sits in a card that says "Run a local retry, then compare it with the retained transcript before deciding" — "deciding" implies a follow-up choice UI that never appears anywhere in this set.
- The top-right status indicator (colored dot + word: "Audio setup," "Ready to read") sits in the same row, size, and font as the "Meetings" link beside it, which is clearly clickable — it's not clear from the screen whether the status text is interactive too.

## 5. Copy
- "Capture the conversation. Keep your own judgment." repeats verbatim on every home screen (02, 04, 09) regardless of whether meetings already exist.
- "No meeting note yet." repeats on both meeting-detail screens (05, 06) but is followed by two different second lines with different meanings ("No transcript was created for this retained meeting." vs. "No admitted note is available. Retained transcript text remains available.") — same headline, different state, no visual distinction between them.
- The on-device-only promise is re-explained in at least four different phrasings across the set: "Saved on this Mac," "Stored on this Mac," "Recording access and on-device storage live here," and "Nothing. Recording, transcription, and notes run and stay here." It's clearly the app's core trust claim but it's never stated once and simply referenced afterward.
- The unresolved meeting title "um, rule-based like system where it's like these products and these fields," (04, 09) reads as a live, unedited transcript fragment used as a title, filler word and trailing comma included — the one line in the set that looks like a defect rather than a choice.
- "Yawn could not verify whether this recording was paused." (05) and "Recording was paused 1 time, for 0:03 in total." (06) use identical banner styling for what read as very different severities — one a possible integrity problem, the other routine trivia.

## 6. Typography, color, spacing, shape, depth
- Two distinct dark surfaces appear: the main app's near-black (01–06) and the Settings window's dark, green-tinted panel (07) — these read as two different apps' dark modes rather than one consistent theme.
- The amber/brown warning color (05, 06) doesn't distinguish severity — a serious warning and a harmless FYI share identical color, weight, and shape.
- Card borders are thin, low-contrast hairlines against near-black (05, 06), hard to find at a glance; the same borders read much more clearly in light mode (10), so legibility isn't equal between themes.
- Depth is nearly flat throughout — no shadows, no elevation; the modal sheet (01) is the only place true depth (dim overlay plus centered card) appears.
- Type scale is wide: the marketing headline (02, 04, 09) and "Settings" (07) both compete for largest type in the set, while functional status text (banners, captions) stays small and comparatively low-contrast.

## 7. Window and chrome
- The main window (01–06, 08–10) uses a plain top bar: wordmark left, status indicator + "Meetings" link + gear icon right. No sidebar, no tab bar, nothing beyond the macOS traffic lights for window controls.
- Navigation between home and a meeting's detail is a single linear model: click a list row, click "Back to meetings" to return. No breadcrumb beyond that link.
- The Settings window (07) is a separate OS window titled "Yawn Settings," with only the red (close) traffic light shown active — the other two read disabled, meaning this window apparently can't be minimized or zoomed, an unusual choice for a settings panel this size.
- No screen in the set uses window width for anything beyond a single centered column; even the widest screens (02, 04) leave the right two-thirds of the lower page empty.
- No resizable columns, no split view, no side-by-side meeting comparison anywhere in the set — one meeting, full width, mostly empty.

## 8. Empty and failure states
- The "no meetings yet" empty state (02) instructs "Press Record to start a private meeting" but no Record control appears anywhere on that screen — the copy references an action the screen doesn't show.
- "No meeting note yet." appears twice with two different meanings (05 vs. 06), as noted above, with no visual cue distinguishing the cases.
- "No pre-meeting context was written for this meeting." (05, 06, 10) is a formal way of saying a field was left blank.
- No screen shows a true error state — no failed recording, no crashed transcription, no denied permission. The closest is the ambiguous, non-actionable "could not verify whether this recording was paused" banner (05), which offers no fix, retry, or explanation of consequence.

## 9. Sheets and secondary windows
- Only one sheet appears (01): a first-run onboarding modal, centered, dimmed backdrop, showing the launch screen through it.
- Settings (07) is a separate OS-level window, not a sheet, distinguished by its own title bar and disabled window controls — a different pattern from the in-app modal.
- No other secondary windows — no player window, no export dialog, no confirmation dialog for a destructive action like delete — appear anywhere in this set.

## 10. Dark versus light
- The warning/status banner is the clearest cross-theme weakness: in dark mode (05, 06) it's a saturated amber/brown block that reads urgent; in light mode (10) the same message is a very pale cream, low-contrast against the off-white page and easy to miss. The same fact carries very different visual urgency depending on theme.
- Settings (07) was only captured in its dark, green-tinted variant — there's no light-mode Settings screen in this set to compare, so whether the green accent and disabled window controls persist in light mode is unknown from these images.
- Buttons invert cleanly ("Allow system audio," "Record another meeting," "Generate note" go from white-on-dark to black-on-light) and read clearly in both — the strongest, most consistent part of the pairing.
- Light mode reads as the fully resolved design (08, 09, 10 all look complete and intentional); dark mode's low-contrast borders and the banner-severity problem are the weaker half, not the reverse.

## 11. Category read
- The main window (01–06, 08–10) reads as an installed native app: a plain fixed top bar with a wordmark, no browser chrome, no URL bar, standard macOS traffic-light controls. The typography, generous negative space, and absence of any card-grid/hero-image layout support a native read.
- Settings (07) reads slightly less native: disabling minimize/zoom on a settings window is an unusual choice for macOS, and the green-accented "YAWN" eyebrow with an oversized "Settings" word-mark reads closer to a marketing splash than a system preferences pane.
- The meeting-list rows (04, 09) — plain text, a trailing chevron, no icons, no avatars, no thumbnails — read as a native table/list view.
- The one element that reads more like a web page than a native app is the "Back to meetings" / "Back to Meetings" text, styled in link-blue with no button chrome, unlike every other control in the set.

## 12. Element classification
- Top bar wordmark "Yawn": correct.
- "Meetings" nav link: usable but visually weak — same weight as the passive status text beside it, ambiguous as interactive.
- Status dot + label ("Audio setup," "Ready to read"): usable but visually weak — unclear if clickable.
- Settings gear icon: correct.
- Marketing headline: appealing but functionally wrong on populated-list screens (04, 09) — repeats onboarding pitch after meetings already exist.
- "Allow system audio" button: appealing but functionally wrong — looks like the primary CTA, but its own caption suggests the permission may already be granted.
- "Recent meetings" empty-state prose: usable but visually weak — references a Record action absent from the screen.
- "Try it: record a 30-second note to yourself" hint: unnecessary as placed — low-priority, greyed, paired with no visible Record control.
- Meeting list rows: correct.
- "Find a meeting by title" search: correct.
- Fragment-sentence meeting title: defect — reads as an unedited transcript leaking into a title field.
- "Back to meetings" link: correct.
- "Rename" / "Manage" buttons: usable but visually weak — "Manage" gives no hint of scope.
- Page title "Meeting · [date]": correct.
- Amber/cream warning banners: appealing but functionally wrong — identical treatment for a real integrity problem (05) and routine trivia (06).
- "MEETING NOTE" card: correct concept, but its two different second-line messages (05 vs. 06) are a defect — same headline state, different meaning, no visual cue.
- "Generate note" button: correct.
- "Transcript retry" card / "Retry transcript" link: usable but visually weak — references a post-retry "deciding" step not shown anywhere.
- "Full transcript" card / "Open" link: correct.
- "Listen to saved audio" card (Play microphone / Play system audio): usable but visually weak — two buttons plus a caption for what reads as one concept.
- "Meeting context" section + "Your notes" section: unnecessary duplication of the "What is this meeting for?" field shown on the transcript-ready screen.
- Onboarding sheet three-card layout: usable but visually weak — heavier than a one-time dismissible explainer needs.
- "Got it" button: correct.
- Settings "YAWN" green eyebrow: appealing but functionally wrong — introduces an accent color used nowhere else in the set.
- Settings disabled minimize/zoom controls: defect — unusual, unexplained restriction on a settings window.
- Settings "Checking speech model" / "Checking" pill: usable but visually weak — a loading state with no progress indication beyond the static word.
- Settings three-fact card ("What leaves this Mac," etc.): correct — the clearest, most complete copy hierarchy in the set.

## Verdicts
- 01-dark-first-run-sheet: revise — three-card layout is heavier than a one-time dismissible explainer needs, and the dimmed background text stays legible enough to compete with the modal.
- 02-dark-first-run-empty: revise — no visible Record action despite copy that promises one; "Allow system audio" outweighs the actual primary task.
- 03-dark-launch-view: revise — "Transcript Ready" pill, headline, and top-right status all restate the same fact three times.
- 04-dark-library: revise — onboarding headline persists after meetings exist, and one meeting title is an unedited transcript fragment.
- 05-dark-meeting-a: revise — a real integrity warning shares color and weight with routine status text elsewhere, with no visible way to resolve or dismiss it.
- 06-dark-meeting-b: revise — "No meeting note yet." is reused with a different, unflagged meaning versus 05; "Manage" and "Retry transcript" don't indicate what they do.
- 07-dark-settings: revise — disabled minimize/zoom controls and an unexplained green accent read as inconsistent with the rest of the app's chrome.
- 08-light-launch-view: accept — content is clean, hierarchy is clear, the theme inversion reads as fully resolved.
- 09-light-library: revise — same fragment-title defect and same persistent-headline issue as 04, in light mode.
- 10-light-meeting-b: revise — the warning banner loses nearly all visual urgency in light mode, undermining whatever severity it's meant to convey.

## Three findings to fix first
1. The warning banner on meeting-detail screens carries no severity distinction: a possible integrity problem ("Yawn could not verify whether this recording was paused," 05) and routine trivia ("Recording was paused 1 time, for 0:03 in total," 06) use identical color and weight, and in light mode (10) both nearly disappear.
2. One meeting in the list (04, 09) has a raw, unedited transcript fragment as its title — the longest, boldest line in the list, and it reads as a defect rather than a feature.
3. The empty home screen (02) tells the user to "Press Record to start a private meeting" but shows no Record control anywhere on screen — the one action the screen's own copy promises is missing.

## What a stronger version would feel like
A stronger version would let the meeting, not the app, be the loudest thing on screen: the pitch-line headline would step aside once a meeting exists, the warning banner would carry real severity levels instead of one shared color for everything from a possible data problem to a three-second pause, and every "on this Mac" trust claim would be said once, clearly, and then simply carried forward rather than re-explained in four different sentences. The window would feel less like a set of independent panels stitched together and more like one continuous place — a single dark theme instead of two, a title field that never leaks a raw transcript, and a home screen whose own instructions match the controls actually on it.
