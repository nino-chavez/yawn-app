---
surface: all (desktop rethink baseline, 2026-10-15)
build: installed 0.6.10–0.6.14 frames (captures 2026-09-29 to 2026-10-09); installed app at review time 0.6.16, whose only interface change since is a Settings › Updates row not shown here
device: this Mac, packaged Yawn.app, real storage; light and dark appearance
reviewer: blind reviewer agent (fresh session; seven frames and the protocol only)
kind: cold
cold: true
release_marker: rethink-phase1-baseline-2026-10-15
states: after (transcript ready) light/dark; after with saved AI draft light/dark; transcribing after Stop (dark); settings (light); after, transcript just landed (light)
verdict: 01 revise, 02 revise, 03 revise, 04 revise, 05 revise, 06 revise, 07 revise
not_reviewed: recording in progress, first run, needs attention, large text, increased contrast, iPad
protocol: judged-screen pattern section 3a, five job questions, plus the category read
---

# Cold Screen Review — desktop rethink baseline

Job questions used: what is happening now, what is next, who, when and where, what can I do. Element classes: correct, usable but weak, appealing but wrong, unnecessary, defect.

Note on frame 07: the packet describes it as a meeting right after its transcript landed. The frame itself shows the meeting status as "Transcribing" in both the sidebar row and the subtitle. I reviewed what is on screen. If the transcript had landed, the frame does not show it.

## 01-after-light

Appearance: A two-pane window with a lavender-grey sidebar listing two meetings and a pale main pane with a very large bold title, "Meeting · Oct 7, 2026". Below the title sit a stack of explanatory sentences, a retry card, an audio-playback block, a boxed "Your notes" field, and a collapsed "Full transcript" row at the bottom.

Job questions:
1. What is happening now: the meeting is a past, finished one with a transcript available (sidebar says so, subtitle says "Transcript"). Answered, though only by small grey text.
2. What is next: unclear. The only invitations are "Retry transcript" and two Play buttons. Nothing says "read the transcript".
3. Who: not answered. No speakers, no participants.
4. When and where: when is answered (date, 12:58 AM, 30:32). Where is answered only by "Saved on this Mac".
5. What can I do: Rename, Manage, Retry transcript, Play microphone, Play system audio, type notes, expand the transcript. Seven or eight actions at similar visual weight.

Eye order: the title first, by a wide margin. Second, the "Transcript retry" heading and its 16px-ish body sentence, which are larger and darker than the notes box. Third, the two outlined Play buttons. The "Your notes" box is fourth or later. The transcript, which is the point of the page, is last and closed.

What competes: "Transcript retry" with its Retry button competes with "Listen to saved audio" and with the notes box; all three are card-or-heading-sized blocks of equal rank. The larger body sentences ("Run a local retry...", "Microphone and system audio are separate recordings.") are bigger than the labels above them, which inverts normal label-over-detail rank.

Remove, combine, demote, disclose:
- "Automatic note generation is no longer available. Your notes and transcript remain available." Unnecessary here; it tells the user about a feature they are not using. Remove or move to Settings.
- "Transcript retry" block: usable but weak. A repair action shown on a healthy meeting. Disclose behind Manage.
- "Listen to saved audio" and "No recording is playing": "No recording is playing" is unnecessary; the Play buttons already say the state. Combine the two Play buttons into one audio control and demote it.
- "Saved separately from the transcript. These are your notes, not generated claims." Unnecessary at this size; one explanation is enough and it can disappear once notes exist.
- Full transcript collapsed: appealing but wrong for the app's purpose. It should be open or at least the second thing on the page.
- Rename and Manage: usable but weak; fine as they are, but two small buttons far at top right, detached from the title they act on.

Verdict: revise. The page leads with housekeeping and puts the transcript behind a disclosure at the bottom.

## 02-after-dark

Appearance: The same layout in a dark grey-violet theme. The selected sidebar row has a lighter outlined pill; the Record button is a soft red-pink outline. Contrast between secondary text and the background is lower than in light.

Job questions: same answers as 01. Nothing about the dark theme improves or breaks them.

Eye order: title, then "Transcript retry" and its large sentence, then the Play buttons, then the notes card. The notes card is a slightly lighter panel that nearly dissolves into the background; its border is faint. The transcript row at the bottom is almost invisible.

What competes: same as 01. Additionally the grey helper lines ("Saved separately from the transcript...", "Saved on this Mac", "No recording is playing") sit close to the background tone and are hard to read.

Remove, combine, demote, disclose: same list as 01. In dark, the grey helper captions are usable but weak on contrast and should be larger or removed rather than dimmed.

Verdict: revise. Same hierarchy defects as 01, plus faint secondary text and a barely distinguishable notes surface.

## 03-saved-draft-light

Appearance: Same shell. The title "Harness meeting" is followed by the subtitle "Sep 30, 2026 · Saved AI draft", then two short headed lists, "Decisions" and "Follow-ups", each with a single bullet that has a dotted underline. Below are the retry block, the notes box with different text, and the collapsed transcript.

Job questions:
1. What is happening now: an old meeting with a saved AI draft. The label "Saved AI draft" is a quiet subtitle tag.
2. What is next: not stated. Nothing says to read, edit, or verify the draft.
3. Who: "Send the pricing note to finance." has no owner shown.
4. When and where: the date is there; sidebar says 11:06 AM; no where beyond "Saved on this Mac".
5. What can I do: edit notes, retry transcript, open transcript, Rename, Manage. Whether the dotted underlines on the bullets are interactive is not clear.

Eye order: title, then the two bullets (large body text, high contrast), then "Transcript retry" heading, then the notes box. The AI draft outranks the person's own notes, which is the reverse of the stated point of the app.

What competes: the AI draft bullets and the "Transcript retry" heading and button. The retry block appears below a pair of confident-looking decisions, which is odd: a meeting with a decision list also offers to redo its transcript.

Remove, combine, demote, disclose:
- Dotted underlines on the bullets: appealing but wrong unless they are meaningful; they read as links or spelling errors. If they link to transcript evidence, nothing says so.
- "Saved AI draft" status: usable but weak; as the only mark that this text is machine-written it is too small next to text that looks authoritative. Either label the block itself or demote the block.
- Retry block: same as 01, disclose behind Manage.
- Notes box: usable but weak; it should sit above the machine draft.

Verdict: revise. The machine's text outranks the person's text, and its provenance is a 12px subtitle.

## 04-saved-draft-dark

Appearance: The dark version of 03. Layout and contents identical; the bullets and their dotted underlines are lighter on the dark ground.

Job questions: same as 03.

Eye order: title, bullets, retry heading, notes card. Same as 03; the notes card is again faint against the page.

What competes: same as 03.

Remove, combine, demote, disclose: same as 03. The dotted underline is harder to see in dark, so the interactive question is even less answerable.

Verdict: revise. Same as 03.

## 05-transcribing-dark

Appearance: A different, plainer layout. The left sidebar has one row, "Transcribing on this Mac" with "0:00". The top bar centre says "Transcribing on this Mac" and the same phrase appears again as text at top right beside the search field. The main pane is mostly empty dark space with small grey lines at the top, a large grey placeholder "Write down the detail you will want to verify later.", and at the very bottom "Stored on this Mac" and a disclosure "What is this meeting for? (optional)".

Job questions:
1. What is happening now: answered, but at least four times (sidebar, top bar centre, top right, body line 2) plus a fifth sentence in line 1. The phrase repeats until it stops being information.
2. What is next: "Your notes remain available while Yawn makes the transcript" says the user can keep typing; nothing says how long it will take. "0:00 Waiting for transcription status" gives no progress.
3. Who: not applicable.
4. When and where: "on this Mac" repeated; no date or title. The sidebar row has no meeting name and a "0:00" duration.
5. What can I do: type into the placeholder. It is not drawn as a field at all; there is no box, border, or label.

Eye order: the large grey placeholder line first (largest text), then the small status lines above it, then the empty space. The top-right "Transcribing on this Mac" is read as a button by habit and is not one that can be told apart from a label.

What competes: three near-identical status strings; the placeholder text competes with the two sentences above it, which are small and dim.

Remove, combine, demote, disclose:
- Sidebar row title, top bar title, top-right status: combine into one. Two of the three are unnecessary.
- "Recording stopped. Audio saved on this Mac..." and "Transcribing on this Mac · The captured audio is saved. Yawn is making the transcript on this Mac.": two sentences saying the same thing; combine and cut to one line.
- "0:00 Waiting for transcription status": usable but weak; a bold time with no meaning here.
- The notes area: defect. The person's writing surface has no visible edge, no label ("Your notes" appears in other frames), and the placeholder is bigger than the status text. This frame uses a different notes treatment from frames 01 to 04 and 07 with no apparent reason.
- Bottom-left "Stored on this Mac" and "What is this meeting for? (optional)": the disclosure is a form-like question pinned to the bottom edge, far from anything it relates to; usable but weak.
- Large empty middle: unnecessary at this height; the notes surface should use it.

Verdict: revise. The most repetitive and least structured frame; the user's own notes surface is the least visible thing on it.

## 06-settings-light

Appearance: A separate window with the title "Settings", a one-line promise, four underlined purple text links (Recording, Transcription, Speakers, Storage), then white rounded cards on a lavender-grey ground, each with a small bold name, a grey sentence, and a purple pill ("Authorized", "Selected", "Ready"). The page continues below the fold.

Job questions (as a settings screen):
1. What is happening now: the state of access and transcription is answered by pills, clearly.
2. What is next: nothing is wrong, so nothing; "Check again" is the only action.
3. Who: not applicable.
4. When and where: "stay on this Mac" appears in the subtitle and again in "What leaves this Mac: Nothing. ... Yawn has no account and makes no network calls with meeting data."
5. What can I do: Check again; a model link; the four section links.

Eye order: "Settings" title, then the three pills (Authorized, Authorized, Selected), then the card headings. The pills are the strongest colour on the page and mark states that are all normal.

What competes: the pills compete with each other and with the headings, because every row has one in the same colour. "Ready" and "Selected" and "Authorized" carry the same purple, so a real problem state would have to be a different colour to stand out.

Remove, combine, demote, disclose:
- Nav links: the four section names are drawn as underlined hyperlinks in a web-like row. Appealing but wrong; a Mac settings window uses a toolbar or sidebar. They also jump within one long scroll.
- Intro sentences under every section heading ("Yawn needs both sources before it can record a meeting.", "Choose how Yawn transcribes on this Mac. The selected option is used for your next transcription.", "An optional anonymous speaker-cluster model. It only runs when you choose Analyze speakers..."): most are unnecessary; the card beneath says the same.
- "Ready" plus "Ready when you choose Analyze speakers for a finished meeting.": same fact twice in one card.
- "Model: NVIDIA Nemotron-3-Diarization (openmdw-1.1)." in bright default-blue underlined link style: defect. It is the only element in the system-blue link colour and it clashes with the purple scheme; it is also engineering detail.
- "Apple speech" card with a single option and a "Selected" pill: a choice with one choice. Unnecessary as a card; show as a line.
- "What happens with your meeting / What leaves this Mac" block: the privacy statement repeats the subtitle.

Verdict: revise. Clear state pills, but the page explains the product to the user at every row and uses web-style navigation and one off-palette link.

## 07-transcript-landed-light

Appearance: The same main layout as 01 but pared back. Title "Meeting · Oct 9, 2026", subtitle "Oct 9, 2026 · Transcribing", one explanatory paragraph, "Listen to saved audio" with two Play buttons, then the "Your notes" box with a grey placeholder. The page ends at the notes box and the lower half is empty.

Job questions:
1. What is happening now: transcribing, said in the sidebar ("17:59 · transcribing"), the subtitle, and the paragraph. Answered three times.
2. What is next: "The transcript will appear here when it's ready." This is answered, but gives no wait time.
3. Who: not applicable.
4. When and where: date, time and duration in the sidebar; "Stored on this Mac" in the notes card.
5. What can I do: play either audio track, type notes, Rename, Manage.

Eye order: title, then the "Your audio is saved..." paragraph and "Microphone and system audio are separate recordings." (larger than the label), then the Play buttons, then the notes box. Of the seven frames this one puts the notes box closest to the top, but the audio block still sits above it.

What competes: the Play buttons and the notes card; both are boxed controls of equal weight, and the Play buttons come first.

Remove, combine, demote, disclose:
- Paragraph "Your audio is saved on this Mac and is being transcribed...": the subtitle already says Transcribing; combine.
- Play buttons: for a recording that has only just stopped, listening to raw tracks is a secondary job; demote below the notes.
- "No recording is playing": unnecessary.
- The notes card placeholder: usable but weak; "Write down the detail you will want to verify later." is at least on-purpose copy, but is dimmer than the surrounding helper text.
- The page does not mention the transcript at all other than a promise, and there is no collapsed row hinting where it will go (compare frames 01 to 04).
- Large empty lower half: the notes surface should grow into it.

Verdict: revise. Honest about its state but repetitive, and still ranks listening above writing.

## Category read

It reads as a web page in a window, in a good-looking and careful way, with some Mac borrowing. Specific tells:

- The window title bar region holds a centred title, a search field and a coloured Record button in one flat strip. This is a custom toolbar, not a standard unified toolbar with separate items.
- The sidebar's selected row is a lavender pill with a full border, not the system selection highlight. Section headers "TODAY" and "PREVIOUS 30 DAYS" are small tracked caps, which is a web convention.
- The buttons (Rename, Manage, Retry transcript, Play microphone) are white rounded rectangles with a faint border and bold 13px labels. They are not system push buttons and they are not toolbar buttons.
- The notes box is a card with a left rule inside it and a resize glyph at its bottom right corner. The left rule makes it look like a block quote, and the resize glyph is a web textarea affordance.
- The Settings window uses underlined hyperlink text for section navigation and a default blue link for the model name.
- The 05 frame, with a placeholder as large display text and no field border, looks like a prototype state that nobody finished.
- Positive tells that do read native: the sidebar toggle icon, the trash row at the bottom of the sidebar, the traffic-light-free clean window, consistent system-like type, sensible dark appearance.
- The page centres a roughly 480px column inside a much wider pane and pushes Rename and Manage to the far right, which is a web layout habit.

Overall: closer to a polished web app than to a native Mac app, and 05 reads as a prototype.

## Hierarchy

The operator's own writing surface is the "Your notes" card. In the meeting view it sits fourth or fifth in reading order: after the title, after one or two status or AI-draft blocks, after the retry block, and after the audio block. Above it in every frame sit the title, the subtitle, status sentences, and either the AI draft (03, 04) or the retry and audio controls (01, 02, 07). Only the transcript, which should rank equally, sits below it, collapsed.

The order does not match what the app is for. The brief says the person's notes and the transcript are the point; the page puts system repair (Retry transcript), playback of raw tracks, and in 03/04 a machine summary above both. Two stronger orderings: notes first, transcript second, audio and retry under Manage; or transcript beside notes. In frame 05, where the person is meant to keep writing, the notes surface has no edge and is dominated by repeated status.

## Copy

Sentences that explain the app or product rather than name a thing or a state:

1. "Automatic note generation is no longer available. Your notes and transcript remain available." (01, 02)
2. "Run a local retry, then compare it with the retained transcript before deciding." (01 to 04)
3. "Microphone and system audio are separate recordings." (01, 02, 07)
4. "Saved separately from the transcript. These are your notes, not generated claims." (01 to 04, 07)
5. "Your audio is saved on this Mac and is being transcribed. The transcript will appear here when it's ready." (07)
6. "Recording stopped. Audio saved on this Mac. Your notes remain available while Yawn makes the transcript." (05)
7. "The captured audio is saved. Yawn is making the transcript on this Mac." (05)
8. "Meeting audio, transcripts, and notes stay on this Mac." (06)
9. "Yawn needs both sources before it can record a meeting." (06)
10. "Yawn uses this to capture your voice." (06)
11. "Yawn checks access to audio from your meeting app. This check does not save audio." (06)
12. "Choose how Yawn transcribes on this Mac. The selected option is used for your next transcription." (06)
13. "Ready to transcribe with Apple's on-device speech engine. No Yawn speech-model download is needed." (06)
14. "An optional anonymous speaker-cluster model. It only runs when you choose Analyze speakers for a finished meeting." (06)
15. "Finds anonymous microphone speaker clusters. It never names people or changes your transcript." (06)
16. "Ready when you choose Analyze speakers for a finished meeting." (06)
17. "Where meeting data stays and when it is deleted." (06)
18. "Nothing. Recording and transcription run and stay here. Personal notes, transcripts, and saved drafts stay on this Mac. Yawn has no account and makes no network calls with meeting data." (06)

That is 18 explanatory passages. Repeats of the same fact:
- Data stays on this Mac: at least 8 times across the frames (05 sidebar/body twice plus "Stored on this Mac", 06 subtitle, transcription card, privacy block, plus "Saved on this Mac" / "Stored on this Mac" in the notes card of every frame).
- Transcribing is in progress: 5 times in frame 05, 3 times in frame 07.
- Notes are separate from the transcript and are the person's own: stated in every notes card as a caption, and again in 05 and 01's banner.
- Speaker analysis only runs on request: 3 times in one card in 06.
- Audio is saved: 3 times across 05.

## Three findings to fix first

1. Reorder the meeting page so the person's notes come first and the transcript second, open by default. Move "Transcript retry" and the two Play buttons under Manage or into a single audio control below. Today the transcript is a collapsed row at the bottom of a page that spends its upper two thirds on repair and playback.
2. Collapse the repeated status copy to one statement per fact. In 05 reduce five "transcribing / saved on this Mac" strings to one line and one sidebar label, and give the notes surface a visible edge and a label like the one in the other frames. In 07 drop the paragraph that restates the subtitle. In every frame keep one "stored on this Mac" instead of a caption per card.
3. Make provenance and affordance honest. In 03 and 04, mark the AI draft as a draft on the block itself, put it below the person's notes, and either explain or remove the dotted underlines. In 06, replace the underlined link row with a native sidebar or toolbar for sections, use one pill colour for normal state and a different one for problems, and restyle the blue model link to the page palette or remove it.

## What a stronger version would feel like

Opening a past meeting would feel like opening a document of your own. Your notes are on the page, large and plain, with the transcript beside or directly under them, both readable without a click. The title is smaller, the date and duration sit quietly beside it, and the only controls in view are the ones you would reach for in the next minute: search the transcript, copy, share. Audio playback is one slim bar. Retry, rename and delete live behind a single Manage menu and appear only when they matter. While a transcript is being made, one line says so and gives some sense of how long, and the notes area stays a clearly edged, labelled surface you can keep typing in. Settings looks like a Mac settings window: a toolbar or sidebar, short rows with a state and a control, and one sentence about privacy instead of eight.

## Not reviewed

Recording in progress, first run and onboarding, a needs-attention or error state, a meeting with no notes and a transcript already open, the full transcript expanded, search results, the Manage menu, rename, trash, large text sizes, Increase Contrast, Reduce Motion, narrow window widths, the lower half of Settings (Where transcription runs, retention, Storage and Updates), and the iPad app.
