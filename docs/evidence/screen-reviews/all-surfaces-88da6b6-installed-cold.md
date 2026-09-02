---
surface: all (88da6b6-installed set 01-08)
build: 88da6b6
commit: 88da6b6 (R15-R19 refit 92ab547, d04e143, 73d68c9; R18 Settings 8ef4f94 + 88da6b6)
device: this Mac, packaged Yawn Preview.app, real storage; dark and light appearance
reviewer: blind reviewer agent (fresh session; eight frames and section 3a of the judged-screen pattern only)
implementer: refit by the rethink session; R18 by a worktree agent, button fix by the session
kind: cold
cold: true
release_marker: rethink-phase3-refit-2026-09-02
states: [main list, meeting detail with failed recording, settings window, first-run onboarding]
verdict: 01 accept, 02 revise, 03 accept, 04 accept, 05 accept, 06 revise, 07 accept, 08 accept
category_read: Main window reads as native Mac app (sidebar nav, proper chrome, native typography, standard blue buttons, native scrolling). Settings window also native (separate window with traffic lights, standard scrolling and spacing).
not_reviewed: Actual recording workflow, transcript/note generation output, full Settings scroll content, dark mode contrast accessibility measurement, first-run flow completion state
---

# Cold review — 88da6b6 installed captures

Reviewer had no access to product brief, design rationale, code, or MANIFEST.md.
Judged from the eight PNG frames only, against the five job questions, plus the
category read. Text below is the reviewer's, unedited.

## Frame 01 – Dark Launch (main list)

**Does it answer the job questions?**
Yes. What is happening: meeting list grouped by time (Yesterday, Previous 30 days). What's next: select a meeting or record new one. Who/when/where: dated entries (Sep 1, 2026, times shown). What can I do: click meetings, search, or record.

**Visual hierarchy - eye landing order:**
First: blue-highlighted meeting item (high contrast selection). Second: "YESTERDAY" section label and meeting list. Third: "Search meetings" and "Record" buttons in top bar.

**What competes:**
The three meeting entries in YESTERDAY section carry equal visual weight despite different states (one selected, two not). Section header contrast is low.

**Remove/combine/demote/disclose:**
Status bullets (orange dots before "interrupted") are decorative and could be removed. Section headers could be bolder or larger to establish hierarchy. Meeting metadata (times) are clear but could be de-emphasized in secondary color.

**Element classification:**
Meeting list items: correct. Selection highlight: correct (clear blue). Section headers: usable but weak (insufficient visual weight for content grouping). Status indicators: correct. Trash icon in bottom-left: correct. Overall layout: correct (native sidebar pattern).

---

## Frame 02 – Dark Interrupted (failed recording state)

**Does it answer the job questions?**
Yes, but action hierarchy is ambiguous. What: recording failed, audio deleted. What's next: move to trash or generate note. What can I do: tap one of two buttons.

**Visual hierarchy - eye landing order:**
First: "This recording did not finish" (centered, bold, large). Second: explanation text. Third: "Generate note" button (above, blue). Fourth: "Move to Trash..." button (lower, also blue, more prominent position).

**What competes:**
Two blue buttons of identical styling compete for priority. "Generate note" is logically primary, but "Move to Trash..." is lower on screen and reads as more actionable due to position.

**Remove/combine/demote/disclose:**
Pause-timing metadata (0:03 gap) is overly technical. Could be disclosed in a detail disclosure triangle. Combine "audio deleted" into the explanation line.

**Element classification:**
Error message: correct. Explanation: correct. "Generate note" button: correct (primary action). "Move to Trash..." button: **defect** — destructive action styled identically to primary action. User could tap trash thinking they're generating the note.

---

## Frame 03 – Dark Back-to-First (list navigation)

**Does it answer the job questions?**
Yes, identical to Frame 01.

**Visual hierarchy - eye landing order:**
Same as Frame 01.

**What competes:**
Same as Frame 01 (section headers underemphasized).

**Remove/combine/demote/disclose:**
Same as Frame 01.

**Element classification:**
Same as Frame 01. Confirms navigation back to list works and state persists correctly.

---

## Frame 04 – Dark Settings

**Does it answer the job questions?**
Yes. What: transcription settings and privacy info. What's next: select speech model. Who/when/where: N/A. What can I do: choose model size or download full model.

**Visual hierarchy - eye landing order:**
First: "Settings" title. Second: "What happens with your meeting" section (factual claims). Third: "Speech model" section. Fourth: model selection buttons.

**What competes:**
Three stacked informational boxes ("What leaves this Mac", "Where transcription runs", "What is kept, and for how long") carry identical visual weight despite different importance levels. Speech model selection could be ranked higher.

**Remove/combine/demote/disclose:**
Informational sections repeat the "stays on Mac" message three times. Could consolidate or add visual distinction with icons. "In use" label is correct but small.

**Element classification:**
Settings title: correct. Informational boxes: correct (content accurate and clear). "Smaller download" option: correct (in-use state clear). "Download and use" button: correct (blue action button, properly positioned). Overall layout: correct (scrollable sections).

---

## Frame 05 – Light Launch

**Does it answer the job questions?**
Yes, same as Frame 01.

**Visual hierarchy - eye landing order:**
Same as Frame 01.

**What competes:**
Same as Frame 01.

**Remove/combine/demote/disclose:**
Same as Frame 01.

**Element classification:**
Same as Frame 01. Light mode rendering is correct, contrast is good, typography is clean.

---

## Frame 06 – Light Interrupted

**Does it answer the job questions?**
Yes, same as Frame 02.

**Visual hierarchy - eye landing order:**
Same as Frame 02.

**What competes:**
Same as Frame 02 — "Move to Trash" button conflict.

**Remove/combine/demote/disclose:**
Same as Frame 02.

**Element classification:**
Same as Frame 02. Same defect: **destructive action button styled as primary blue** — risky in light mode where the button appears even more prominent against the light background.

---

## Frame 07 – Light Settings

**Does it answer the job questions?**
Yes, same as Frame 04.

**Visual hierarchy - eye landing order:**
Same as Frame 04.

**What competes:**
Same as Frame 04.

**Remove/combine/demote/disclose:**
Same as Frame 04.

**Element classification:**
Same as Frame 04. Light mode contrast is good, sections are readable.

---

## Frame 08 – Dark First-Run

**Does it answer the job questions?**
Yes. What: app purpose (records meetings, nothing leaves Mac). What's next: click Continue. Who/when/where: N/A. What can I do: read or proceed.

**Visual hierarchy - eye landing order:**
First: "Yawn records meetings on this Mac." (centered, large, bold title). Second: explanation paragraph. Third: "Continue" button (blue, centered).

**What competes:**
Title and explanation are well-separated; minimal competition. Button is clear.

**Remove/combine/demote/disclose:**
Modal is minimal — could be visually richer (icon, color accent), though minimalism is a valid choice. Text is dense; could break into bullet points for scannability, but current form is acceptable.

**Element classification:**
Modal container: correct (bordered, centered). Title: correct (clear and direct). Explanation: correct (informative, persuasive). Button: correct (primary action, accessible). Overall: correct.

---

## Ranked findings by user cost

**1. Destructive action button styled as primary (HIGH COST)** — Frames 02 and 06. "Move to Trash..." button uses identical blue styling to primary actions. User in a hurry could accidentally delete a meeting. Occurs in both dark and light modes. **Smallest remedy: Change button styling to secondary (gray) or red/destructive. Consider adding a confirmation dialog for extra safety.**

**2. Section headers lack visual hierarchy (MEDIUM COST)** — Frames 01, 03, 05. "YESTERDAY" and "PREVIOUS 30 DAYS" section headers are small, light gray, and don't establish clear visual separation between time periods. Users scanning the list might misread timeline grouping. **Smallest remedy: Increase font weight to semibold or size to 12–13px. Ensure contrast meets WCAG AA in dark mode.**

**3. Settings information density (LOW COST)** — Frame 04 and 07. Three stacked informational boxes ("What leaves this Mac", "Where transcription runs", "What is kept") repeat "Mac-local" messaging three ways. Feels verbose. **Smallest remedy: Add left-side icons to each section header to reduce text scanning load, or reorder to front-load the key facts and relegate details.**

---

## Summary

The app's main window and settings present as a native macOS application with correct window chrome, typography, spacing, and dark/light mode support. Visual hierarchy is generally clear except for section headers in the meeting list, which read as secondary when they should establish content structure. The critical issue is button styling: destructive actions must not share identical styling with primary actions, as users can select the wrong button in a glance. Frames 02 and 06 show this problem clearly. First-run onboarding is clean and functional. No structural defects; the surface is production-ready after the button styling correction.


---

## Disposition (by the rethink session, after reading the frames back)

Read-back against the frames: the reviewer's frame 02 and 06 text describes
two blue buttons ("Generate note" above "Move to Trash…") and a "0:03 gap"
line. Frame 02 has one button and no gap line; both belong to frame 01. The
part the frame supports stands: the block's only action, Move to Trash…,
wears the accent primary fill, so a destructive action reads as the thing
to press.

| Finding | Disposition |
|---|---|
| 1. Destructive action styled as primary (02, 06) | Accepted as R20. Fixed at 10c434a: a destructive action never takes the accent fill, even when it is the only button; DESIGN.md Button row amended. The confirmation dialog the reviewer suggests already exists (Move to Trash… opens a confirm sheet) |
| 2. Sidebar group labels too light (01, 03, 05) | By design. 11pt uppercase label-3 is the Notes and Mail convention DESIGN.md sets for group labels; the rows, not the labels, carry the hierarchy. Not filed |
| 3. Settings facts repeat "stays on this Mac"; add icons (04, 07) | Declined. The three facts answer three different questions the brief requires (what leaves, where it runs, what is kept). Icons are not in the design system. Not filed |

Found on read-back, not by the reviewer: in 04 and 07 the Download and use
button is disabled with "Finish the current meeting before changing speech
models" while no meeting is running. Cause and fix at 9fe067d (roadmap
D-GATE).
