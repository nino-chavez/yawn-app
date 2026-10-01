# Product brief — Yawn

Status: reset on 2026-08-10; automatic note generation retired from the product
contract on 2026-09-30. Implementation and release are separate states.

Future sequencing lives in the [product roadmap](roadmap.md). The roadmap may
propose work, but this brief remains the current product contract until amended.

## Who this is for

One person needs to stay in a conversation and still keep a useful private
record. They should be able to start capture clearly, jot down what matters,
and find the finished note later without learning a new workspace.

The reader knows how to use a Mac. They should not need to understand audio
routing, speech models, library handles, or internal product history.

## What must stay true

- Capture, transcription, and retained meeting data stay on this Mac. The app
  does not imply an account, cloud sync, a meeting bot, calendar access, sharing,
  or task creation.
- Recording starts only after the operator confirms participant consent,
  headphones, and that they are the one person near the microphone.
- Audio retention is chosen explicitly: 1, 7, or 30 days.
- The operator's own notes are distinct from previously saved AI drafts.
  Saved claims keep their evidence state and source links. A withheld turn
  never becomes invented transcript text.
- New AI note generation and note-model downloads are unavailable. This does
  not delete saved notes, transcripts, audio or installed model files.
- An interrupted or failed run is stated plainly. It is never presented as a
  completed meeting.

## What real meetings add

This reset is also grounded in private source material: local recordings,
captions, and note exports from real meetings. The source material itself is
not copied into this repository or product brief.

Those meetings make one interface requirement non-negotiable: a note can be
accurate and still leave out a commitment, a deadline, or the difference
between two nearby ideas. A tidy summary must never look like a complete record.

- During a meeting, the operator needs an unconstrained place to write their
  own reminders before a detail disappears.
- After a meeting, the operator can edit their own notes and read, search,
  copy, open or export the retained transcript. Yawn does not offer generation
  or regeneration as the next step.
- Previously saved AI decisions, follow-ups and open questions keep a clear
  path back to their retained source text. These drafts remain reviewable AI
  output, never a final or complete account.
- Selected transcript excerpts remain supporting evidence. Label them
  **Transcript highlights** and do not imply a new summary was generated.
- The full transcript stays available in the same meeting view. It is the
  record for checking a decision, owner, or follow-up that matters.
- The library remains organized around individual meetings. It does not become
  an action-item dashboard merely because a meeting contains commitments.

Historical Loom material will be incorporated by the same rule when its local
copies or links are available: derive product requirements, never copy private
meeting content into the app or repository.

## The product, from first principles

Yawn is a private meeting notepad with a recorder attached. It is not a
workspace, dashboard, task manager, CRM, team wiki, or calendar.

There are three moments that matter:

1. Before a meeting: one clear record action and a short consent check.
2. During a meeting: a calm note canvas, a visible recording state, and one
   obvious way to stop.
3. After a meeting: a readable note with the transcript available when needed,
   then a simple list of past meetings.

The operator's notes and retained transcript are the destination. The list
exists to reopen meetings; it is not the home-screen subject. Settings remain
a small auxiliary window.

## Interface rules

- Open to the next useful action, not a dashboard of features.
- Keep the recording control visible without surrounding it with setup,
  diagnostics, folders, templates, or planned features.
- Give the operator a plain place to type during capture. Their notes guide what
  they need to remember; they are not a form to complete.
- Make personal notes and the retained transcript useful after capture. Keep
  saved AI drafts readable with their source links. Do not hide or relabel a
  saved draft as the operator's own writing.
- Keep the status language concrete: recording, preparing, finishing,
  transcript ready, or needs attention.
- Start with Apple speech when it is ready on a fresh setup. Otherwise offer
  the smallest cataloged speech download. Keep other models in Settings and
  preserve an existing selection. Prepare Apple language files only through
  an explicit action; the operating system manages those files.
- Keep Settings focused on audio access, speech selection, optional speaker
  analysis and storage. Show which speech engine is in use. Allow switching only when capture and
  queued transcription are idle, and never remove the active downloaded model.
- Keep recording, personal notes and transcript access usable without a note
  model. Remove note-model setup and generation controls. A stale command must
  refuse before a download, generation job or meeting change starts.
- Use a short, quiet Mac-native surface: generous reading width, one main
  content column, light chrome, and color only for recording or attention.
- Never use fake counts, placeholder meetings, promised automation, or a
  synthetic example as if it were data from this Mac.
- A first run must teach without counterfeiting. The app may explain itself
  in its own words — the before/during/after moments, what a finished note
  is, where things live — and may invite the operator to learn by recording
  something real and disposable. It must never seed a meeting, transcript,
  claim, or count the operator did not produce: in this product a meeting is
  a provenance chain, and the first Show source must resolve to something
  actually said. (Amended 2026-09-01. Clearly labeled sample content is how
  the category onboards; it stays out of Yawn not because labeling is
  dishonest but because a sample meeting either fabricates provenance or
  breaks the loop it exists to teach.)

## Not part of this reset

Do not add folders, saved views, action dashboards, templates, calendar sync,
meeting bots, sharing, collaborative workspaces, chat over every meeting, or
automatic task creation just to match the category. They need a separate
product decision and a real underlying capability.

## Research inputs, not copied product direction

Wispr Flow's current desktop Scratchpad is lightweight, keyboard-reachable, and
meant to stay out of the way of the work already on screen. That supports a
capture-first, low-chrome interaction model, not its account or sync model.

Granola's meeting notepad keeps a plain editor available during the meeting and
distinguishes a person's own notes from AI-enhanced material. That supports a
local note canvas and provenance distinction, not Granola's cloud integrations
or collaboration model.

Notion demonstrates bot-free system-audio capture, consent, and a searchable
record after a meeting. It is a useful category comparison, but its shared
workspace and automation are deliberately outside Yawn's scope.

Sources checked for this reset:

- [Wispr Flow Scratchpad update](https://wisprflow.ai/whats-new)
- [Granola's meeting-notepad explanation](https://www.granola.ai/blog/announcement)
- [Granola's note-editor documentation](https://docs.granola.ai/help-center/taking-notes/taking-notes-in-granola)
- [Notion AI Meeting Notes](https://www.notion.com/en-US/product/ai-meeting-notes)
