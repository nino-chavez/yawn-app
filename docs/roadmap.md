# Yawn product roadmap — trust before reach

Status: active product direction as of 2026-08-17.

This roadmap sequences product work. It does not claim that a listed feature is
shipped. The current product contract remains [the product brief](product-brief.md).

## The decision

Yawn should make the transcript easier to trust and correct before it expands
into broader meeting intelligence.

Local vocabulary is now visible and editable in meeting review, and its exact
replacements reach future generated notes without changing the retained
transcript. Recording-quality evidence and the source-bound transcription retry
journey are now built in Preview. A reader can compare the current transcript
with a separate retry, keep the current version, or promote the retry without
silently regenerating the note. The exact fixture now proves the read-only
review, quality, device, playback, Keep-current, and Promote-retry states;
generated-note invalidation and production-package gates remain open.
Cross-meeting questions can follow once the source record is reliable.
Cloud accounts, automatic call detection, meeting bots, and live meeting chat do
not enter the roadmap through competitor comparison alone.

## Design direction rethink — 2026-09-02

The operator rejected the visual direction: the installed app rendered a
marketing hero over a list inside a native window while the brief said
Mac-native and light chrome, and the mismatch dated from the first commit of
the reset. Diagnosis in `design-rethink-2026-09-02.md`; the rethink ran the
judged-screen pattern's own path: a blind cold review of the installed build
(nine of ten frames revise), device captures of six installed comparables,
an experience brief with the platform strategy written for the first time,
three divergent whole-screen concepts rendered on the same four states in
both appearances, and a selection ADR. **Concept A, source list beside
document, was selected** (`design-direction-decision.md`); `DESIGN.md` now
carries the visual system and `DIRECTION.md` points at the ADR. The rendered
alternatives B and C stay in `apps/desktop/ui-harness/concepts` with their
frames. Owed next: the implementation wave that rebuilds Home and the
meeting view to concept A, carrying the D-LOCK, D-TOAST, and R-series
findings unchanged, then a cold review of the rebuilt installed app.

**Implementation wave landed — 2026-09-02 afternoon.** Three worktree agents
rebuilt the surface to concept A: the main window (toolbar, day-grouped
sidebar, document pane, inspector; 123 UI tests), the native shell (1080x900
window, File and View menus emitting menu events, Preferences-style
Settings, a cached runtime-manifest verification that ends the permanent
"Checking speech model" state), and the token port (`packages/design-tokens`
in Minder's schema, regenerating `ui/tokens.css`, `npm run tokens:check`).
Installed captures and their manifest: `evidence/screen-reviews/captures/`
under the build hash. Two defects the honest surface exposed:

- **D-READ, fixed (89fe963).** The library reader returned an unavailable
  note with no handles for a recovered-interrupted meeting whose capture
  files were partials (`.mic.wav.partial`). Root cause, reproduced with a
  `scan_and_recover` fixture: a quit mid-finalize can leave a genuine
  `capture-session/2` completion receipt on disk before the partial legs are
  renamed and before `meeting.json` learns the receipt exists. Recovery's
  incomplete-lifecycle path reused that receipt verbatim, and the receipt
  verifier correctly refused a complete-session receipt naming partial legs,
  so the meeting was quarantined instead of recovered. The fix is in
  `bind_interrupted_artifacts` (session-core): a stranded but parseable
  completion receipt is replaced; anything else stays quarantined untouched.
  The note response gains the state value `recovered-interrupted`, and
  `LibrarySnapshotRow` gains `durationSeconds` (from the retained leg's own
  header and real file size) and `recovery` for R10.
- **D-MODEL, cause found; two fixes.** The preview bundle's runtime is
  `app-runtime/1` with no `model_catalog`, because the preview lane in the
  distribution runbook stages `build-alpha` (bundled Whisper), while
  `DEPLOYMENT.md` and the model-hosting section stage
  `build-alpha-external`. On a bundled-model runtime the backend answers
  "this build does not use downloadable speech models", which is a fact,
  and `settings.js` swallowed it in a bare catch and printed the failure
  copy. The stored active model verifies cleanly (catalog, receipt, digests).
  Fix one: the preview lane now stages `build-alpha-external` so the
  Settings surface reviews the model chooser the brief lists; the runbook is
  corrected. Fix two, folded into R11: Settings renders the bundled-model
  case as a fact, not a failure.
- **D-OPEN, fixed.** The symptom filed as D-READ ("the recovered-interrupted
  meetings open to unavailable") had a second cause that survived both
  reader fixes: every meeting opened by clicking a row after another meeting
  was open came back `stale`, not only the interrupted ones. The reader
  spends all row handles when a note is opened (`open_note_current` clears
  them, a capability boundary from 2026-08-02), and the old list-then-detail
  navigation refreshed the library on Back to Meetings before any second
  open. Concept A's list beside the document removed that step, so the
  second click reused a spent handle. Found by tracing `library_open_note`
  in the installed preview after a probe test over a copy of the real
  storage showed the reader itself admitting both meetings. Fix: openMeeting
  takes a fresh snapshot before opening, the step reopenSelectedMeeting
  already performed. Both reader fixes stand: the stranded-receipt shape
  (89fe963) and the honest released-audio message (4180458) were each real,
  and the second one was the only defect in the reader for the meetings on
  this Mac.
- **D-GATE, fixed (9fe067d).** Settings disabled "Download and use" with
  "Finish the current meeting before changing speech models" while no
  meeting was running (seen on the 88da6b6 captures, 04 and 07). Opening a
  past meeting restores the reducer to `TranscriptReady`, and both model
  gates read any non-Idle capture as a meeting in progress. The mic and tap
  are released in that state, and the app already treats
  `Idle | TranscriptReady` as safe for other operations. One helper now
  owns the check for both gates, with a unit test.
- **D-NOTE-STAGE, open: no packaged build can generate a note.** Pressing
  Generate note on the transcript-only meeting in the installed preview
  ends in "This action is not available right now. Try again." The cause is
  packaging, not the meeting: `worker/note_generator_mlx.py` imports
  `mlx_lm`, and `note_bridge.py` (bf082ce, 2026-08-16) looks for it in a
  private `python-runtime/lib/python3.12/generate-site-packages/` tree that
  `worker/build_runtime.sh` never stages. The 2026-08-16 end-to-end run in
  `note-runtime-decision.md` used a hand-installed tree, and the preview
  lane's `runtime-fresh` step rebuilds the runtime without it. Neither the
  preview bundle nor `/Applications/Yawn.app` has the directory today. Two
  consequences: the toast says "Try again" for a condition that cannot
  change, and the Generate note control is offered without checking that a
  generator is admitted. The attempt also leaves a nonterminal operation
  record that refuses every later attempt for that meeting (the known gap
  the decision document names); the one from this session was removed by
  hand. Fix in two slices: stage the isolated tree from a hash-pinned lock
  in `build_runtime.sh` with a verify step, and surface generator admission
  to the meeting snapshot so the control and its copy state the fact.
  Second slice landed (167a9f1 + 06aed65): `library_open_note` now carries
  `noteGenerationAvailable` and one of two reasons, "Download a note model
  in Settings first." or "This build cannot generate notes."; the control
  renders disabled with the reason as its help. First slice merged
  (adae21a): `build_runtime.sh` installs `requirements-generate.lock`
  (34 hash-pinned packages, mlx-lm 0.31.3 with its own mlx 0.32.2) into
  `generate-site-packages` after the runtime copy, and verify proves the
  isolated import and that the shared mlx stays 0.29.3. The isolated tree
  adds 410 MB to a 938 MB runtime. Two defects found on the way, both fixed
  before merge: the tree was first created under the stage before
  `cp -R`, which nested the interpreter; and the version check read an
  attribute mlx does not have. Installed verification then found a third
  gate: `SecurityCodeVerifier` refuses the generator in an ad-hoc bundle by
  design, and the preview sign step used ad-hoc even with a Developer ID
  identity on the machine. With the identity present the preview lane now
  does the release lane's nested signing minus the Apple submission
  (a8eed6f), and Generate note ran the child end to end on the installed
  preview for the first time since 2026-08-16. Found by an opt-in trace
  (`YAWN_NOTE_TRACE=1`, ee66fc2 and 41973bd) that names the stage an
  unavailable run failed at; before it the only evidence was the generic
  toast. The one meeting tried has a zero-turn transcript, and the child
  honestly rejected it ("note-rejected"); a real meeting has not been
  generated yet.
- **Known gate failure, unrelated.** `local-meeting-notes-session-core`
  fails one test, `the_packaged_question_receipt_describes_the_files_it_measured`,
  since `worker/embedding.py` changed in 4205c32 (2026-09-01) after the
  corpus-question receipt was produced. The test's own message names the
  remedy: re-run the probe. It fails identically at 956537c, before any
  work in this wave.

Blind cold review of the rebuilt installed app
(`evidence/screen-reviews/all-surfaces-2765401-installed-cold.md`): seven of
seven revise, and the category read moved from "native by chrome" to
"window mechanics native, interior treatment web-in-a-window." Refit intake
from it, all inside the selected direction:

| Id | Finding | Remedy |
|---|---|---|
| R8 | "No meeting note yet." outranks the meeting's own title (01, 04) | Note-card empty state at body weight; the title stays the largest line |
| R9 | The unavailable pane says "Reopen Meetings" while its one button says "Back to meetings," and the toolbar keeps naming the meeting that failed (02, 05) | Copy names the control; the toolbar title resets to "Yawn" on failure; D-READ removes the case for these two meetings |
| R10 | Three rows read "Meeting · Sep 1, 2026" and differ only by fine print (01) | Rows carry length and a note excerpt when one exists (needs the snapshot row to expose duration and recovery state: the UI agent's request); an untitled meeting still reads "Meeting · date" |
| R11 | Settings repeats the same model error three times (03, 06); card hairlines near-invisible in dark | One line per error; dark separators at the contrast token |
| R12 | First-run sheet reads as a web modal (centered card, X, pill button) with the blurred library bleeding through (07) | Render as a native-shaped sheet attached to the title bar, opaque, no X |
| R13 | Settings groups read as bordered divs, not grouped lists; "Open" beside Full transcript reads as link text | Grouped-list material for Settings; a button or chevron for Open |
| R14 | Operator, on the refit build (dcbc1c6, 2026-09-02): "fonts sizes are inconsistent and too small to comfortably read. buttons are not consistently sized or spaced or laid out either." Counted on the frame: five text sizes in the document pane, most prose at 13; three button treatments (bordered pair by the title, filled Generate note, a bordered card for the transcript disclosure) at different heights; section headings indented differently from body; hairlines between every section; the transcript card wider than the reading measure; the same fact stated twice ("No meeting note yet." and the reader's message; audio deleted and cannot be retranscribed) | Port the document pane to concept A and the specimen, not another patch: three sizes (22 title, 15 prose, one 13 caption), one `.btn`, one left edge, hairlines only where DESIGN.md puts them, each fact once. DESIGN.md now states where each size goes |
| R15 | First run (08): the orientation sheet says "Got it" while the system-audio permission gate sits dimmed behind it with its own button, never mentioned by the sheet | Sequence the two asks: the sheet, then the permission step as the empty library's one action; or fold the permission sentence into the sheet |
| R16 | Interrupted meeting (02, 06): the toolbar drops to "Yawn" although a meeting is selected, and the needs-attention message sits left-of-center in dead space | The toolbar carries the selected row's title in every state, including needs attention (revises R9's reset, which was written for an unreadable meeting; this one is readable); center the pane's block on both axes at the reading measure |
| R17 | Notes box (01, 03, 05): about 200px of fixed empty space between the placeholder and its own caption; the dark-mode placeholder is nearly body contrast | Size the box to its content with a small minimum, put the caption directly under the field, and set the placeholder to label-3 in both appearances. The 62ch measure and the right gutter are by design (DESIGN.md Composition); not a finding |
| R18 | Settings (04, 07): content clipped at the 720 window with no scroll cue; bordered cards read as a web settings page; five to six type levels; "1.61 GB" and "In use" hard to read | Grouped-list rows without the card border, a scrollable window that shows its scrollbar, type held to 13 body and 11 caption, size and badge at body size |
| R19 | Button family (08, all): "Got it" is a near-white one-off; Record is bare red text that reads as a link | "Got it" becomes the specimen `.btn.primary`; Record keeps its color but takes the `.btn` bezel |
| R20 | Needs attention (02, 06, review of 88da6b6): the block's only action, Move to Trash…, wears the accent primary fill, so a destructive action reads as the thing to press | A destructive action never takes the primary style, even alone (10c434a; DESIGN.md Button row) |
| R21 | Seen while running Generate note on the installed 88da6b6 preview: the startup card and the toast were the retired design (hero headline, uppercase kicker, big-radius card, yellow card), and every sheet still carried the 38px semibold button family and an eyebrow | Every legacy `.button` takes the specimen `.btn` shape; sheet heads use the 22 title and 15 body; the startup surface is the needs-attention shape; the toast is a panel at body size; the twelve eyebrow kickers are removed (08fc1c3). Verified in the harness (new `mode=startup`); installed capture pending |
| R22 | Model setup (first launch with no speech model; harness `mode=model-setup`): retired hero headline at 43px and two 292px option cards | Title at 22, copy at 15, options as grouped-list rows with one primary. Not yet done; needs stub options to render |
| R23 | After a rejected generation (lifecycle summary-failed, seen on the installed preview): the caption reads "Summary Failed", the Generate note control and the audio fact vanish, and nothing on the page says what happened or offers the retry the view-model defines ("Your meeting note needs another try." with Regenerate note) | Render the summary-failed recovery presentation in the document and keep the control; the reader should never see a bare enum label as the only explanation. Not yet done |

Landed 2026-09-02 evening, verified on the installed preview: R15 (92ab547,
opaque backdrop, copy names the permission step), R16 (d04e143 and 73d68c9,
title kept, block centered with one left edge), R17 (d04e143, notes box sized
to a few lines, caption under it, placeholder at label-3), R19 (d04e143, Got
it is the specimen primary; Record already carried the bezel and reads bare
only because it is disabled in the preview). R18 (8ef4f94 grouped list,
three type sizes, one primary per group, scrollable window; 88da6b6 buttons
at the specimen size and weight). Captures of 88da6b6 filed under
`docs/evidence/screen-reviews/captures/88da6b6-installed/`; blind review
filed (all-surfaces-88da6b6-installed-cold.md): six accept, two revise, both
windows read as native. R20 (10c434a) and D-GATE (9fe067d) verified on the
installed preview; frames under
`captures/10c434a-installed-addendum/`. Not yet captured on any installed
build: During, Paused, and a meeting with a generated note and its inspector.

## What the live apps changed

The 2026-08-16 desktop comparison checked Granola 7.478.0, Wispr Flow
1.6.447, and the installed Yawn 0.5.7 without starting a recording or changing
meeting data. It produced three decisions.

**Evidence labels.** “Observed” means the rendered desktop app was inspected
directly. “Verified in source” means the current Yawn code or product brief was
read. “Inferred” means the recommendation combines those two evidence classes;
it is product judgment, not a claim about an unobserved competitor behavior.

| Journey | Granola — observed | Wispr Flow — observed | Yawn — observed or source-verified | Yawn decision — inferred |
|---|---|---|---|---|
| Home and history | Meeting library leads; search and secondary workspace structure remain nearby | Product hub exposes history, usage, settings, and adjacent tools | Installed 0.5.7 returned from a readable transcript to an empty library; the 0.5.8 preview continuity journey passed | Keep one local meeting library and make continuity a release gate |
| Recording entry | Capture stays subordinate to the meeting and note journey | Notetaker entry is one part of a larger dictation product | Explicit consent and start action are source-verified product boundaries | Preserve explicit consent; do not add automatic call detection |
| Review attention | Generated note occupies the primary reading path; transcript and search disclose nearby | Repair and settings surfaces make learned words and retry actions visible | Summary-first note, separate personal notes, and transcript evidence are source-verified | Keep the note primary; disclose transcript, provenance, and repair at the point of doubt |
| Correction and recovery | Source context stays available beside generated material | A failed Insights view exposed one retry and recovered; dictionary entries are visible and editable | Speaker correction now preserves the transcript and records a separate local operation | Add exact repair actions, local vocabulary, and versioned retries without replacing the last usable result |
| Expansion pressure | Team spaces, templates, connectors, sharing, and chat broaden the workspace | Scores, streaks, quotas, referrals, transforms, and scratchpad broaden the product hub | Yawn is a private local meeting reviewer | Do not copy adjacent engagement, collaboration, or account architecture |

**Take Granola's meeting posture, not its workspace.** Granola keeps the note in
the main reading path and puts the transcript, transcript search, and source
context behind nearby controls. Yawn should preserve that progressive
disclosure. It should not add team spaces, connectors, sharing defaults,
calendar organization, chat, or templates to reproduce Granola's information
architecture.

**Take Wispr Flow's repair clarity, not its product hub.** A failed Insights
view exposed one **Try again** action, and the retry recovered the view. Its
dictionary also makes learned words visible and editable. Those are useful
models for Yawn's exact-repair and local-vocabulary work. Usage scores, streaks,
quotas, referrals, transforms, and a separate scratchpad solve other jobs and
stay out.

**Treat journey continuity as a release gate.** The installed Yawn identified
itself as 0.5.7 while this source declares 0.5.8. A completed transcript was
readable in the installed app, but **Back to Meetings** opened an empty library.
That observation does not prove the 0.5.8 source has the same defect. It does
prove that a packaged build must pass the complete-meeting, reopen-from-library
journey before its note or library work counts as shipped.

## The source baseline

The Yawn 0.5.8 source already provides the part many meeting tools treat as the
end goal:

- A generated note leads with an overview
- Decisions, follow-ups, ideas, and open questions are separated
- Generated claims link back to retained transcript evidence
- The full transcript remains available for verification
- Personal notes remain separate from generated claims
- Capture, transcription, and meeting storage stay on this Mac

The summary-first note is the baseline. It is not another roadmap item.

## Roadmap at a glance

| Order | Product outcome | State | Governing constraint |
|---|---|---|---|
| 0 | A completed meeting remains visible and reopens from Meetings in the packaged app | Preview passed; release package pending | A readable artifact must not disappear from its own journey |
| 1a | The reader can see who said what | Implemented in source and preview | Render the attribution already carried by each transcript turn |
| 1b | The reader can correct who said what | Implemented, tested, and packaged in Preview; rendered save-and-regenerate journey pending | Never hide uncertainty or overwrite the source transcript |
| 2 | Names and jargon stay correct across meetings | Review controls, bounded storage, and provenance-safe note application are implemented, tested, and packaged in Preview | Vocabulary remains local, visible, editable, and bounded |
| 3 | The reader can tell whether the audio caused a bad transcript | Quality guidance, bounded device context, retained-audio playback, and source-bound retry comparison and decisions are implemented, tested, packaged in Preview, and rendered with exact fixtures; generated-note invalidation and production-package gates remain | Quality evidence stays distinct from capture-integrity evidence |
| 4 | Every recoverable problem leads to its exact repair | Existing recovery controls and the current stable reopen errors route to the selected meeting or Meetings; warnings without a real destination remain future work | Recovery actions must not imply that a failed operation succeeded |
| 5 | The reader can find source passages across past meetings | Decision closed: keep semantic source finding out of the shipped interface until retrieval evidence establishes usefulness | Search remains local, source-linked, and unavailable during capture |

## Delivery plan

The roadmap ships as dependent wedges, not as one large branch. The first wave
finishes the correction foundation while starting two independent foundations.
Later waves wire those foundations into the product only after their contracts
survive combined review.

```text
verified baseline
├── A. corrected transcript -> note generation
├── B. local vocabulary domain and storage contract
└── C. exact recovery presentation for current meeting warnings
        │
        └── combined integration and packaged preview gate
                ├── D. vocabulary controls and transcript application
                ├── E. capture-quality evidence and transcription retry
                └── F. narrow cross-meeting source finding decision
```

### Wave 1 — completed in parallel

| Packet | Outcome | Owned files | Must not do | Merge gate |
|---|---|---|---|---|
| A — corrected note input | Note generation consumes an immutable corrected transcript projection while source locators continue to resolve against the retained transcript | Desktop Rust correction and note-operation path | Do not rewrite the retained transcript or change the browser UI | Tests prove corrected names reach generation, source digest remains pinned, and no-correction behavior is unchanged |
| B — local vocabulary core | A bounded local domain model can add, edit, disable, delete, and deterministically project exact replacements | New session-core vocabulary module and its `lib.rs` export | Do not add UI, Tauri commands, fuzzy matching, or mutate past transcript artifacts | Tests cover restart-safe serialization, exact scope, disabled entries, deletion, ordering, and size bounds |
| C — exact recovery presentation | Existing warning and failure states map to one contextual next action without claiming success | Desktop UI view model, rendering, styles, and UI tests | Do not add backend commands, settings changes, or generic help routing | Tests cover action, unavailable-action, retrying, failure, and preservation of the last usable result |

### Wave 1 merge order

Packet B merges first because it changes only session-core. Packet A merges
second because it completes the current speaker-correction slice. Packet C
merges last because its copy must be checked against the combined backend
states. The orchestrator resolves integration edits; workers do not edit outside
their owned files to make another packet compile.

After merge, the full Rust and UI suites must pass together. A separately
identified Preview bundle must then prove this release journey: reopen a completed
meeting, inspect the original speaker label, apply a correction in a disposable
fixture, regenerate the note, follow a source link, return to Meetings, and
reopen the same meeting. The installed production app remains untouched.

### Wave 2 foundation — completed in parallel

| Packet | Delivered foundation | Remaining product work |
|---|---|---|
| Vocabulary projection | Exact replacements travel as bounded original-source ranges. Changed-length prompt text maps evidence back to the retained transcript. The current store is re-attested before durable note replacement. | Dedicated controls and current-meeting application counts are now built. **Always correct this** remains a later shortcut into the same ledger. |
| Recording-quality evidence | New capture receipts persist resolved microphone identity and a separate `capture-quality/1` block for silence, clipping, low input, and steady background energy. Legacy receipts report quality as unknown. | Reader-safe guidance, bounded device context, retained-audio playback, and versioned retry are now built. Exact-fixture review states are rendered; mutation and production-package gates remain. |
| Withheld-turn recovery | A valid withheld row now exposes one source-bound restore action. The command, capability, build contract, and shell contract are synchronized. | Add the remaining exact destinations only after their owning controls exist. |

### Wave 2 integration — built; exact-fixture state receipt recorded

1. **Built and packaged in Preview:** expose local vocabulary through a small
   dedicated sheet. Keep every entry visible, editable, disableable, and local.
2. **Built and packaged in Preview:** show current-meeting application counts
   through the same immutable projection already used by note generation.
3. **Built and packaged in Preview:** create a retry only from reverified
   retained audio and the current transcript. Store the candidate separately.
   Never replace the active transcript during retry creation.
4. **Built and packaged in Preview:** surface reader-safe recording-quality
   guidance, compare the current and retry transcripts, and require an explicit
   keep-or-promote choice. Promotion clears the stale note pointer but does not
   regenerate a note automatically.
5. **Built and packaged in Preview:** route the current stable reopen errors to
   the selected meeting or Meetings. A failed playback attempt offers a refresh
   that mints new single-use handles instead of leaving dead controls.
6. Complete future additions to the exact-repair map only after their
   destinations exist. A button that opens a placeholder does not count as
   recovery.

The remaining product work is now the note-changing journey and warnings whose
destinations do not exist yet. The exact fixture renders retry review and both
decisions, quality, device, and playback states. Speaker-correction save,
regeneration, generated-note/source-link, invalidation of a note that actually
exists, recovery-toast, and exact installed-production-package evidence remain
open.

### Wave 3 decision — closed, do not ship yet

The local source-finder backend already exists. It uses bounded transcript
windows, returns quoted passages with turn provenance, reports prepared-window
coverage and near ties, mints opaque transcript handles after the worker round
trip, and refuses semantic work during capture. The product shell deliberately
keeps the command unregistered.

The committed 200-meeting scale probe explains why. The best bounded unit found
7 of 10 intended meetings and 3 of the 5 questions that exact search could not
answer. Its own registered conclusion says that this decides the storage unit,
not whether retrieval is useful. Human usefulness was explicitly unreachable
from that synthetic corpus.

Do not re-admit the command or build a global semantic-search surface from this
evidence. Keep title search, per-meeting exact transcript search, and claim-level
**Show source** as the shipped recognition paths. A later experiment must use
disposable data, preserve quoted source passages and honest coverage, show near
ties, abstain when evidence is missing, and remain unavailable during capture.
Only then should the candidate journey be reconsidered.

A focused independent review checked the committed probe, dormant command,
shell exclusion, and current title, transcript, and claim-evidence routes. It
reported zero material findings in this no-ship decision.

## Category-review intake — 2026-08-31

A review of the forty apps in awesome-mac's note-taking list, each read from
its own site or repository and filtered against the product brief, produced
nine adoptable capabilities. Evidence class: fetched vendor and repository
documentation, not observed installs — weaker than "observed" in the table
above, and OATS in particular deserves a hands-on install before its README is
treated as its ceiling.

These enter the roadmap as proposed work. The brief remains the contract, and
none of this claims a sequence ahead of the open Order 0–4 gates.

| # | Proposed outcome | Source pattern | Governing constraint |
|---|---|---|---|
| I1 | A recording can pause and resume without ending the session | OATS | A pause is a capture-integrity event: the gap lands in the receipt and shows plainly in review |
| I2 | A global hotkey summons operator-note capture during a meeting | Stik, Quick Note, SideNotes, nuttyartist/notes | Notes land only in the operator canvas; no floating sticky-note surface, no new data model |
| I3 | Pre-meeting context notes inform the generated overview | Notion AI Meeting Notes | Context is a labeled operator input; it never appears as transcript-backed generated content |
| I4 | A transcript turn shows which generated claims cite it | MarginNote 4 | Extends the existing claim→source link on the existing surface; no cross-meeting linking |
| I5 | A meeting can be locked, with re-auth on export and audio playback | Bear, Standard Notes | State the honest claim — a local-access deterrent — unless encryption at rest actually ships |
| I6 | Restore never overwrites: every history feature copies out | Joplin, Standard Notes | Already true of keep-or-promote; becomes the standing rule for any future editing surface |
| I7 | The on-disk meeting record is readable without Yawn installed | Knopo, Notable, Noteship, FSNotes | Byte-stable re-serialization; an app-independent format; plain-file trust stated in product copy |
| I8 | Export ships as one compact archive and one plain per-item form | Joplin, Quiver | The claim→evidence structure survives export; an export is a local file, not sharing |
| I9 | Deleting a meeting or retry is recoverable within a window | QOwnNotes, Anytype | Local trash only; the no-recovery-past-window disclosure is stated plainly |

**Intake Wave 1 status (2026-08-31):** I1, I2, and I3 are implemented and
merged to main from three isolated worktree packets, with the combined suites
green after integration (session-core, desktop, UI, worker pytest, Swift
capture tests). Implemented and merged is the claim — packaged, installed,
and shipped remain separate states, and a real recorded meeting is still owed
before any of the three counts as proven in use. Facts the merge established:

- Pause is a true hardware release: the audio engine stops and macOS's mic
  indicator goes off while paused; silence-while-paused is test-proven against
  fake sources. Pauses land in the worker-attested capture receipt as an
  additive `capture-pauses/1` block (wall-clock sample offsets), written
  inside finalize so the receipt digest covers it; receipts without the block
  read as "not paused" everywhere, and the no-pause writer output is
  byte-identical to before.
- **Semantic change:** `capture_elapsed_samples` (and `capture_elapsed_s` in
  the health block) now means recorded span net of pauses, not wall clock.
  Without this, any pause over ~2 s would fail the
  `leg_ended_before_capture_stop` integrity floor. The floor interaction is
  not exercisable by the test lanes — it is a live-run checkpoint.
- The note-capture hotkey (⌃⌥Y) is armed only while capture is active and
  disarms on stop and on every capture-failure path; a failed registration is
  logged and the meeting proceeds without the hotkey, never claiming it armed.
- Pre-meeting context is a `meeting-context/1` sidecar mirroring the
  operator-note module; it rides the durable generation request (crash
  recovery replays it), a no-context request stays byte-identical, and a
  context-only assertion cannot become a claim (test-proven; enforcement is
  structural — context never receives an evidence alias).
- Known gaps carried forward: context is only editable from Arming onward (no
  meeting directory exists in Idle; a draft-staging decision is open), context
  is not yet displayed on the read-only library surface, and the worker
  pytest suite showed one non-reproducing flake (tracked separately).
- Finding for a later packet: `note_validator.py`'s evidence gate is not
  uniform across claim types — decision/action claims require an
  agreement-signal match in the cited excerpt, while summary/proposal/question
  claims require only a resolvable alias, leaving their prose unchecked
  against the excerpt.

**Intake Wave 2 status (2026-08-31):** I4, I7, I8, and I9 are implemented and
merged to main from three isolated worktree packets; the combined suites are
green after integration (session-core 531, desktop 189, UI 55; no Python or
Swift files changed this wave). Implemented and merged, not packaged or
shipped; the release journeys owed are: reopen a restored meeting from Trash,
and open an exported bundle with no Yawn installed. Facts the merge
established:

- Reverse citations are derived at read time from verified claim locators —
  never stored — and share the claims list's staleness lifecycle by
  construction. Pre-meeting context now displays read-only on the library
  note surface. (I4; also closes the Wave 1 display-after gap.)
- Whole-meeting deletion now moves to a local Trash under the storage root
  with a 30-day purge riding the retention tick; restore reinstates the
  organization row last and refuses ghosts. Two bugs were caught in build:
  a crashed purge could leave a ghost entry whose restore would have
  quarantined the library's organization, and the purge-in-progress check
  failed open on an unreadable deletions directory. Both are fixed with
  regression tests. Audio retention keeps running inside Trash — the
  privacy promise is never deferred. Transcript and audio deletion remain
  immediate. (I9, with I6's restore-as-copy rule as the governing shape.)
- Export writes plain files plus a zip inside the meeting's own directory
  (the capability contract denies dialog authority, so no save dialog):
  note.md preserves every claim's turn references and quoted excerpts,
  transcript.md renders withheld turns as withheld, operator files carry
  provenance headers, receipts copy byte-verbatim, and any artifact failing
  digest verification is withheld and named in README.txt and the command
  result — never exported silently. (I7, I8.)
- The desktop suite's two timing flakes were reproduced at the unmodified
  base commit by the trash packet — pre-existing, not wave-caused; tracked
  as separate work alongside the worker-pytest flake.

**Design Wave 3 status (2026-08-31):** D1, D3, D6, and D8 are implemented and
merged (session-core 544, desktop 196, UI 65 all green after integration).
Facts the merge established:

- Meeting rows preview the note overview's first sentence, read from the
  digest-verified claim projection at listing time — generated content only,
  absent when there is no admitted note, never a placeholder. A projection
  API hazard was found and fixed in build: reading claims per-row cleared
  earlier rows' sealed handles; previews are now read in one pass before any
  handle is minted. (D1)
- Reading measure and leading are named tokens (--measure-reading 68ch,
  --leading-reading 1.65, --leading-transcript 1.58); transcript body text
  moved 13 → 14 px as the app's longest-form reading surface. (D8)
- The retry comparison highlights word-level differences before keep-or-
  promote: a dependency-free Myers diff over word tokens, withheld turns
  excluded as opaque boundaries, punctuation counted, both budget caps
  skipping honestly ("too long to highlight") rather than blocking, and
  absence of highlights never rendered as "identical." A tokenizer-parity
  check ships per-turn word counts so a Rust/JS split mismatch renders plain
  rather than misplacing a highlight. Known bound to carry: the worst-case
  diff trace can transiently allocate ~250 MB inside the comparison call
  while the storage lease is held — bounded, then skipped; tune
  MAX_EDIT_BUDGET down if a live run ever shows it. The packaged-preview
  fixture now *stages* all three diff states (computed with differences,
  computed with none, and skipped over budget) plus capture pauses, a
  trashed-and-restorable meeting, an export withheld-artifact manifest, a
  real validator-passing generated note (row preview and reverse citation
  map), and read-only pre-meeting context — `rendered-review-fixture`'s own
  test suite proves each one against the real product code paths
  (`transcript_retry_diff`, `capture_quality`, `meeting_trash`,
  `verify_artifact_ref`, `NoteRevisionRef::validate`). No rendered walk of
  the packaged app against these new states has been performed yet — that
  observation, and its own dated receipt in this file, remain owed before
  the next release gate. (D6)
- Privacy is legible in-product: a three-row what-happens table in Settings
  ("Three facts about this Mac, not a policy promise") sitting directly
  above the model card its second row points at; one plain sentence on the
  start sheet; "on this Mac" as the canonical locality phrase across eight
  status and save-state strings, including live "Transcribing on this Mac."
  The read-first pass confirmed every pre-existing locality claim true;
  "encrypted" and "audited" remain deliberately unused — neither is
  currently a checkable claim. (D3)
- The three suite flakes were root-caused and fixed on a separate branch
  (fix/yawn-flaky-tests: a racy pid-file handoff, a fixed-sleep exit
  assumption, a mocked os._exit raising in a daemon thread), verified over
  fifty double-contention runs, and merged after review. Both cargo lanes
  and the worker suite are deterministic again.

**Wave 4 status (2026-09-01):** I5, the D9 audit and its fixes, the
Wave 2–3 fixture coverage, and the render-architecture repair are implemented
and merged (session-core 545, desktop 233, UI 81 all green after
integration). Facts the merges established:

- A meeting can be locked (I5). The lock is a plain meeting-lock/1 sidecar;
  enforcement lives in Rust behind single-use, action-scoped tokens minted
  only after LAContext deviceOwnerAuthentication (Touch ID with the login
  password as macOS's own fallback; dependency: objc2-local-authentication
  0.3.2, the wave's one new compiled crate). Opening a locked meeting for
  reading, exporting it, and playing its audio each take their own
  confirmation; reading re-issues its token so ordinary refreshes do not
  re-prompt. Locked rows keep title and date, suppress the note preview, and
  keep transcript_available truthful. Deletion and regeneration are
  deliberately ungated — a forgotten-auth meeting must never become immortal
  — and locking is refused on a Mac that can never confirm the owner. The
  shipped copy states the honest claim ("a local barrier, not encryption"),
  enforced by a claim-checking test. **Honest ceiling, two facts:** the
  sidecar is deletable from the filesystem, and the corpus index writes
  derived content of locked meetings outside the meeting directory (no
  reachable read path today — corpus_search is unregistered — but any
  honest statement of the deterrent includes both).
- The D9 native-text audit found two unconditional focus-loss bugs (library
  search on every debounce; rename and speaker fields on every background
  poll tick) and the substitution hazard on exact-match fields — all fixed
  and pushed — plus the structural finding: render() replaced root.innerHTML
  wholesale, resetting WebKit undo history every 900 ms tick. That is now
  repaired by in-place DOM patching (ui/dom-patch.mjs) with a WKWebView
  verification harness at apps/desktop/ui-harness/.
- Confirming that repair live in the packaged preview bundle (2026-09-01)
  surfaced a second, independent D9 blocker: the app menu carried only the
  Yawn submenu, so macOS had no key-equivalent route for cmd-Z/X/C/V/A into
  the webview — undo stayed unreachable from the keyboard even with its
  stack preserved. The standard Edit submenu (predefined items binding the
  native selectors) is now installed in main.rs. Observed in the rebuilt
  preview bundle: cmd-Z reverts typing across 900 ms poll ticks, including
  native autocorrect reversal, and cmd-A selects all. The harness cannot
  catch this class — it drives undo via execCommand, which bypasses menus —
  so keyboard-route regressions stay a packaged-app check.
- The packaged-preview fixture now stages the Wave 1–3 states (diff computed
  /identical/skipped, pause spans, a trashed meeting, export withholding, a
  validator-passing note with row preview, citations, and context), each
  proven against real product logic; the fixture's note document was built
  through the packaged Python interpreter and validator, not hand-written.

Live-run receipts now owed, in one list: the GUI walk of the packaged
Fixture app across the staged states (including the not-yet-staged locked
states — extend the fixture first); the real Touch ID prompt, password
fallback, and cancel-returns-to-locked; a real recorded meeting exercising
pause and the elapsed-vs-integrity-floor interaction; reopening a restored
meeting from Trash; opening an export bundle with no Yawn installed; and the
D9 audit's five-item live checklist (undo across ticks was observed live in
the preview bundle on 2026-09-01, Edit menu in place; the five listed items
— word-select, option-arrow, smart quotes, spellcheck, context menu — remain
owed).

**Wave 5 status (2026-09-01):** D5 and the lock-hardening follow-ups are
implemented and merged (session-core 552, desktop 237, UI 94 all green after
integration), and the exact-search decision memo is delivered. Facts the
merges established:

- Evidence disclosure has three depths (D5): hover or focus a claim's source
  affordance and the cited span previews in place from span data batched with
  the note response (chosen over per-locator fetches, which would have
  rebuilt the entire library projection per call); Show source opens the
  transcript beside the note at the highlighted span, falling back to the
  single-column layout below ~1100 px; while both are visible the transcript
  tracks the note's topmost fully-visible claim via rAF-batched geometry,
  suspending for the reader's own transcript scrolls. Frame rate in the live
  webview and split proportions at real window sizes remain live-run checks.
  The old per-claim inline evidence path is retained unused for rollback and
  owed a removal pass.
- Locked meetings are structurally excluded from the corpus index (digest
  treats an excluded meeting as absent, so lock and unlock transitions force
  a real resync) and from the dormant search path — every hit kind filters
  before counting, and a hit sealed before a lock refuses at open. The
  search commands stay unregistered; the hardening exists so no future
  registration decision can ship the bypass the decision memo found. The
  fixture stages a locked meeting through the real sidecar shape and can
  never fake an unlock — the fake confirmer is test-gated out of the
  packaged app by design.
- The cross-meeting exact-search decision memo is with the operator. Its
  recommendation: hold until the (now-landed) lock exclusion, then decide on
  a one-week local usage probe of the already-built dormant command. The
  decision remains open.

**Desktop-design audit — 2026-09-01.** A two-half audit of Yawn as a macOS
app: a source-derived inventory of every screen, sheet, and state (the first
such map to exist anywhere), and a hands-on pass observing launch, landing,
and window behavior beside Bear and Agenda. Evidence classes: source claims
carry file-and-line citations; observed claims come from driving the
installed build (which predates this week's merges) live.

Facts the audit established:

- **The reopen contract is broken.** Closing the window hides it
  (deliberate, correct for a tray-resident app), but no reopen handler
  exists — activating the app with zero windows fronts it with nothing on
  screen, observed live and confirmed absent in source. Tray "Open Yawn"
  is the only recovery. Bear reopens on activation.
- **⌘W and ⌘M bind to nothing** — the menu carries App and Edit submenus
  only; there is no Window menu. File, View, and Help are defensibly
  absent (no document model); Window is not.
- **The observed "lands on the last meeting" is not persistence.** No
  window-frame or last-view persistence exists in source; the behavior is
  the capture state machine restoring a never-dismissed finished meeting,
  which re-captures the landing on every launch until Back to Meetings is
  clicked. A dismissed-state cold boot lands on Home. Agenda's observed
  counter-example (frame memory, place reset to an empty overview) is the
  named landing anti-pattern; Bear (frame, note, and scroll all restored)
  is the restoration bar.
- **Honest-waiting-state gaps:** the library's loading line renders
  identically at 200 ms and forever (no stall affordance); the tray shows
  the same alarming glyph for first-run model setup as for real failures;
  a finished, unread transcript gets no tray signal at all. Error-recovery
  actions are coupled to exact backend strings — a rewording silently
  downgrades to a dismiss-only toast.
- **Passes worth recording:** single-instance show-and-focus; all seven
  sheets share one consistent custom-modal idiom with coded Escape
  precedence and no click-outside dismissal; sensible window minimums;
  no Services/share/print/login-item code half-exists; no rendered dead
  ends — every meeting-detail state keeps an unconditional route back.
- Lock precedence hides co-existing recovery states until unlock (by
  design, now recorded), and the locked barrier's copy says removal is the
  only way in while its button offers read-only Confirm-to-open — a
  wording mismatch to fix in passing.

Decisions taken from the audit's five calls: the two adopts are merged —
RunEvent::Reopen now shows and focuses the window only when none are
visible (the old .run() shorthand discarded every run event, which is why
reopen never fired), and the Window menu ships Minimize, Zoom, and a ⌘W
that composes with the existing hide handler through the native
performClose: chain, verified in the vendored crate sources. The Dock-click
round trip itself is live-run evidence and joins the receipt list.
The remaining calls were decided by the operator on 2026-09-01:

- Honest waiting states: merged. The library's loading line escalates to
  honest stall copy with a retry after ten seconds; first-run model setup
  gets a calm setup glyph instead of the failure mark; a finished, unread
  meeting gets its own tray state ("Your meeting is ready to read");
  the locked barrier's copy matches its actual actions. Error-recovery
  actions now key on stable machine codes with a two-sided drift test
  against a shared registry (user copy byte-identical). The D2 timing
  receipts are merged: every successful start writes capture-timing/1
  separating operator time from app latency; the budget gets stated from
  live-run numbers.
- Tray-menu scope: the tray becomes minimally state-aware — Open Yawn
  always, Stop recording only while Recording or Paused (the brief's "one
  obvious way to stop", reachable while the window is hidden), and a
  standard Quit. No pause from the tray; pause keeps the window's context.
  Merged: the tray inserts Stop recording within one tick of Recording or
  Paused, routes through stop_meeting's own path off the main thread, and
  Quit is the native terminate: selector by construction.
- **Landing: Home-always, closed as deliberate.** The brief's
  open-to-next-action rule makes Home the landing; Bear-style place memory
  suits resuming writing, not reopening finished meetings, and the
  genuinely-unfinished case is already handled by capture-state recovery.
  No place memory and no window-frame persistence; revisit only if a
  live-run annoyance receipt argues otherwise.
- Exact search: the memo's recommendation is adopted. The one-week local
  usage probe is merged — the dormant commands registered but gated on
  a local marker file, default-off and byte-identical to today without it,
  logging only invoked/opened timestamps, riding the existing search box,
  unavailable during capture, locked meetings excluded by the W5-B
  hardening. The ship/hold decision follows the week's log.

**Live-run receipts — 2026-09-01, packaged preview, agent-driven test
recording (operator authorized).** Three consecutive takes produced the
first packaged evidence for the pause journey and the D2 budget, and found
two shipping bugs:

- Take 1 failed at the pause click: the bundled Swift capture helper was
  twelve days older than the app (`invalid_control` — it predated the pause
  protocol). Take 2, after a targeted helper rebuild, failed at finalize:
  the bundled Python worker was equally stale and refused the now-required
  `pauses` argument. Root cause for both: nothing gates runtime-staging
  freshness against source at packaging time (a gate is in flight as its
  own task). Both failure surfaces behaved exactly as designed — plain
  statements, no fake completion, diagnostics naming the codes.
- Take 3, on a fully rebuilt runtime: the complete journey passed —
  consent (1-day retention), Recording, a ~3.5 s pause with the observed
  "Nothing is being recorded / both audio sources are released" state,
  resume, stop, worker finalize accepting the pause, transcript-ready.
  The receipt carries capture-pauses/1 with the real span
  (samples 267504→323013) inside the worker-attested digest. RECEIPT:
  pause round trip observed in a packaged build.
- **First capture-timing/1 receipt: app span (consent → Recording) 532 ms;
  sheet render 1 ms.** D2's budget can now be stated from measurement — a
  "recording starts within two seconds of consent" budget holds with 4×
  margin on this hardware. Operator span (3.5 s) was scripted clicking,
  not a human baseline.
- The recovery queue salvaged take 1 into a readable transcript and
  refused the integrity-rejected take 2 as ineligible, logging the
  refusal — recovery observed behaving honestly on real failures.
- Findings for a later packet: the Arming step's elapsed counter renders
  an epoch-garbage value until Recording starts; a transcript-ready
  meeting with zero turns lists as "note only" (technically true,
  ambiguous copy); the preview packaging script's empty manifest_args
  broke under macOS bash 3.2 on an app-runtime/1 staging (fixed on main).

**Live-run defects — FIXED 2026-09-02 (chip session, reviewed and merged).**
Root cause went one layer deeper than the original filing: a committed
transcription-queue item held a live claim with no release receipt (the
worker died between commit and claim-release), and scan recovery's
release_claim refused it forever — retrying at ~4 Hz (48,650 diagnostics,
190 MB, pruned to samples) and starving every later transcription. Retention
then collapsed "transcription still needs this audio" with "damaged
meeting", and any quarantine flipped a global flag that blocked all
recording. Fixed: release_claim skips the source gate for committed/terminal
items; retention defers instead of quarantining pending-transcription
meetings; a per-meeting quarantine never refuses app-wide readiness;
scan_and_recover classifies captured meetings (finalized -> awaiting
transcription; quit-mid-finalize -> honest recovered-interrupted, stale
queue obligation cleared); the router keeps the library reachable from
every needs-attention state; the no-pending-retry toast is silenced.
Verified against the preserved quarantine-evidence meeting in throwaway
storage. Owed: the cold screen review of the changed surfaces (blocked on
the screen-recording re-grant), and the packaged preview rebuild.

**Original filing — 2026-09-01 evening (installed-app capture pass).**
Two real defects found and evidenced (docs/evidence/screen-reviews/captures/
e7e96f9-installed/MANIFEST.md):

- **D-LOCK, severe: quit-during-finalize hard-locks the app.** A recording
  stopped normally; the app was quit seconds later, mid-worker-finalize.
  The meeting (lifecycle captured, session never finalized) is refused by
  the transcription queue, quarantined by retention, and then blocks all
  recording — Check again loops, and Back to Meetings routes INTO the
  blocker, so the library is unreachable and the meeting cannot be deleted
  in-app. Zero in-app recovery; unblocked only by moving the meeting dir
  aside by hand (preserved at quarantine-evidence/). Crash-during-recording
  has recovery; quit-during-finalize has none. Fix shape: scan_and_recover
  classifies unfinalized-captured meetings (salvage or quarantine WITH a
  rendered destination); one bad meeting must never block recording
  globally; Back to Meetings always reaches the library. This is Order 4's
  "warnings without a real destination" made real, at maximum severity.
- **D-TOAST: internal-state copy leaks.** "The installation check is not
  waiting for a retry." surfaced as an operator-facing toast during the
  blocked state — no operator meaning, no action offered.

**Refit intake — 2026-09-01 (judged-screen reviews of e7e96f9).** The cold
and conformance reviews (docs/evidence/screen-reviews/, browser-render,
PROVISIONAL pending installed-app captures) set design_intent: refit in
DIRECTION.md. The direction stands — the primary journey passed cold; these
are presentation changes inside it. Strings flagged by the cold review were
verified at source as shipped copy, not fixture artifacts.

| # | Proposed outcome | Source finding | Governing constraint |
|---|---|---|---|
| R1 | The generated note is the first readable content at every window width | Conformance drift: at 960 px the notes/context aside precedes the note in document order | The brief's own sentence: the note is the first readable result after capture; the aside discloses nearby, not first |
| R2 | Home carries one Record affordance | Cold: two differently-styled Record buttons on one surface | One obvious way to start; the topbar and hero may not both spend the record color |
| R3 | The withheld-turn surface explains itself | Cold: "the voice check" never defined; "Restore this turn" states no consequence (real copy: main.js:894, :1718; library_reader.rs:1184) | Trust surfaces carry their own explanation at the point of doubt; copy changes route through DIRECTION.md's content-reads table |
| R4 | Increased contrast produces a legible response | Both reviews converged: no prefers-contrast rule exists; the state renders byte-identical to default (large text already reflows correctly) | An accessibility state that changes nothing misstates what the system asked for |
| R5 | The color budget holds on idle surfaces | Conformance drift: idle Ready dot, always-coral Record, green In-use tag in Settings | Color only for recording or attention plus one evidence accent; per-element decisions owed — a record-colored Record control may be the budget used correctly, a green third status color is not |
| R6 | A locked row says why its summary is absent | Cold: the LOCKED row's missing preview reads as ambiguous (real render, I5's deliberate suppression) | Suppression is deliberate; ambiguity is not — one word of explanation, no content leak |
| R7 | The first-run teaching copy never names a control that is not rendered | Installed-app cold review, confirmed against the genuine frame: the empty state says "Press Record" while a true first run renders only "Allow system audio" | Teaching copy describes the screen the stranger is actually on; if setup precedes Record, the copy walks through setup first |

All six stay provisional until the installed-app capture pass, which
supersedes the browser-render verdicts and may return design_intent to
preserve or amend this list.

Remaining decisions and design intake: the exact-search call (memo
delivered), D2's stated capture speed budget (live-run-shaped), and D7's
enforcement note.

I1–I3 change what the operator can do during and around capture; I4–I9 harden
trust in what already exists and can travel as independent packets. Two
boundaries from the same review:

- Cross-meeting **exact** full-text search surfaced as a candidate and
  deliberately did not enter this table. It is adjacent to the closed Wave 3
  decision — exact rather than semantic, but still an expansion of the named
  recognition paths — and needs its own product decision first.
- The Quiver-sourced half of I8 rests on snippet-only evidence (its repository
  was unfetchable); verify against the app before building to its specifics.

Everything else the category competes on — folders, tags, saved views,
calendar coupling, tasks, dashboards, sync, accounts, engagement mechanics,
plugin ecosystems — matched the brief's banned list by name and stays out.

### Design-lens pass — 2026-08-31

A second pass over the same forty apps reviewed information architecture,
journeys, trust surfaces, interaction, and look and feel against the brief's
interface rules. Evidence class: described — vendor pages, docs, and design
press fetched in-session — except where a same-day hands-on pass upgraded a
claim to observed: Bear (no-ceremony first run; typography defaults 15 pt,
1.5 em line height, 48 em measure — via its own bespoke face, not the system
stack), Agenda (default view shows notes fully expanded in one chronological
scroll, which strengthens D1; the empty "On the Agenda" sidebar slot is the
manual-attention decay D7 predicts, observed live), and Craft (hard account
wall before any note exists — its polish claims stay described, so D5's
hover-preview rests on MarginNote's documented model). OATS 0.20.0's home is
observed via operator screenshot: a Today/Meetings/Todo hub with ⌘K search, a
"No upcoming meetings" scheduling slot, a mascot prompt in engagement register,
two recording entries with no visible consent step — and, on a fresh install,
"Could not load meetings." as bare text with no retry: an empty state rendered
as a failure, the anti-pattern Order 4's exact-repair work exists to prevent,
live on the competitor's first screen. Its recording and review journeys
remain unobserved; the review-gap finding stands.

The orienting finding: OATS documents install, record, and a summary landing
in a flat library, and describes no review or editing journey at all. Yawn's
primary journey is the competitor's silent gap. The design intake below
serves that difference.

| # | Proposed design outcome | Source pattern | Governing constraint |
|---|---|---|---|
| D1 | A meeting row previews the note's outcome, not only its title | Agenda's reviewed weakness, inverted | Real generated content only; never a placeholder line |
| D2 | Recording presence is ambient: menu-bar pill plus hotkey entry, one consent-gated action, a stated seconds budget | OATS, Quick Note, Stik | The pill speaks the existing status vocabulary; entry never bypasses consent |
| D3 | Privacy is legible in-product: a what-happens table and "processed on this Mac" as live status copy | OATS's backend table; Anytype/Standard Notes/Noteship trust register | Mechanism and checkable fact only; no claim that cannot be verified |
| D4 | Show source behaves as identity: exact span, scrolled and highlighted, durable across reopen | MarginNote's card model | The claim and its transcript span are one stored unit, never a render-time lookup |
| D5 | Evidence disclosure has three depths: hover preview, split view, synced scroll | Craft, MarginNote, MiaoYan | The note stays primary at every depth; affordances reveal on intent, not as chrome |
| D6 | Retry comparison shows word-level diff before keep-or-promote | Inkdrop's timeline shape plus the diff it lacks | The delta is visible before the choice; browsing versions alone is not comparison |
| D7 | "Needs attention" is always system-derived | Agenda's decayed manual flag, inverted | No operator-curated pins or stars; attention states come from evidence |
| D8 | Note and transcript reading measure is a designed decision | Bear, MiaoYan | Fixed defaults, not user typography settings; settings stay small |
| D9 | Every webview text field passes the native-text test | Knopo's stated bar | Word-select, option-arrow, smart quotes, spellcheck, context menu — a checkable QA list |

Standing refusals confirmed with named counter-examples: decorative color in
the library (Zoho's random card colors), theme pickers (Inkdrop, Quiver, and
Bear's nine themes are not a target), default-maximal chrome and calm-mode
toggles (QOwnNotes's six panels, Notable's Zen mode), tab and split-window
furniture (VNote), and the router tell — when a palette or module nav becomes
necessary, the surface has already sprawled (massCode). Performance is a
design property: synced evidence and diff views hold frame rate or retreat to
tabs, so it gates release for D5 and D6.

### Drift controls

- Every packet starts from the same verified commit in its own linked worktree
- File ownership is exclusive during a wave; widening scope returns to the orchestrator
- Shared contracts are changed once, at integration, rather than copied into each branch
- Each worker reports its commit, changed files, tests, assumptions, and unresolved gates
- The orchestrator reviews branch diffs and source, not completion claims
- A packet may be merged only when its own tests pass and its assumptions match this roadmap
- “Implemented,” “packaged,” “installed,” and “shipped” remain separate states
- No worker signs, installs, uploads, deploys, or changes `/Applications/Yawn.app`

## Current build receipt

**Directly observed on 2026-08-16.** The separately identified Yawn Preview
bundle rendered speaker labels beside transcript turns. Its **Back to Meetings**
action returned to a Recent meetings list containing the completed preview
meeting. No recording was started, and no meeting content was copied into this
roadmap.

**Verified in source.** Transcript rendering now displays the attribution
already stored on a turn and uses **Unattributed** when no claim exists. A
desktop regression test also proves that an active meeting is excluded from the
library while its lease is held and enters the library when that lease ends.

The correction increment now stores each speaker-name change as a separate,
meeting-local operation bound to the transcript digest and original source
speaker group. The latest operation changes only the rendered attribution. The
retained transcript file remains byte-for-byte unchanged, earlier corrections
remain in the local history, and choosing the source label restores the source
projection.

**Directly observed after the correction build.** The packaged Preview transcript
rendered each speaker name as a correction control. Opening it showed the source
label, the exact number of matching turns, the unchanged-source promise, and an
explicit source-label recovery action. No correction was saved during this
rendered inspection.

**Wave 1 source and package receipt.** Three isolated worktrees started from the
same baseline and merged through disjoint file ownership. The merge added the
corrected-note input, the bounded local-vocabulary core, and contextual recovery
presentation. Cross-review caught and removed unrelated formatting edits. It
also added an incremental output bound before the vocabulary packet merged.

The corrected-note path re-derives the active correction under the same meeting
lease used for durable note replacement. The generator applies the validated
speaker-label overlay only after reading and digest-checking the retained
transcript. The generated note and its locators keep the original transcript
digest. No correction keeps the former request shape.

**Verification commands.** The session-core suite passed 428 unit tests, 17
process-fault tests, and 8 doc tests. The desktop suite passed 131 tests, the
shell contract passed 5, and the UI suite passed 22. The rebuilt packaged
runtime passed 209 worker tests. `npm run preview-build` and
`npm run preview-verify` produced and verified the separately identified
`Yawn Preview.app` bundle.

**Rendered verification boundary.** The earlier Preview observation above is
still direct evidence for speaker controls and meeting continuity. The new
bundle launched, but computer-use could not obtain an accessibility snapshot
from that process. No real correction was saved and no private note was
regenerated to manufacture a passing visual check. The source and package gates
are green; the rendered save, regenerate, source-link, Back, and reopen journey
remains a release gate.

**Wave 2 source and package receipt.** Vocabulary replacements are now stored
in durable note-generation requests as bounded source ranges. The note child
checks the retained transcript and each source-span digest before model use.
It changes only model-facing excerpts; generated-note provenance and locators
continue to name the original transcript. Empty vocabulary keeps the former
wire shape.

New captures now persist resolved microphone identity and a separate versioned
quality block. Capture integrity remains the existing pass/fail floor; silence,
clipping, low input, and steady background energy are guidance evidence and do
not change that verdict. Rust recovery accepts the two named optional receipt
fields while continuing to reject unrelated fields and changed audio.

A withheld transcript row now exposes **Restore this turn** only when the
current meeting, transcript digest, source row, and idle capture state agree.
The backend repeats those checks and refreshes the meeting only after the
existing immutable restoration operation succeeds.

The combined session-core suite passed 429 unit tests, 17 process-fault tests,
and 8 doc tests. The desktop suite passed 132 tests, the shell contract passed
5, and the UI suite passed 23. The rebuilt runtime passed 217 worker tests.
The refreshed Preview bundle built and passed bundle verification. The installed
production app remains unchanged.

**Wave 2 integration receipt.** The meeting review now has a dedicated local
vocabulary sheet. It lists every exact Before → After row, its enabled state,
and its application count in the current non-withheld transcript. Add, edit,
enable, disable, and two-step delete actions are source-digest-bound and run
under the meeting lease. They do not rewrite the visible transcript or trigger
note generation. Future note regenerations use the saved local projection.

The retry foundation now accepts only the current retained transcript, capture
session, microphone audio, and system audio digests. The worker rechecks those
artifacts before and after creating an immutable candidate. Session-core checks
the same five-part binding under the meeting lease, stores the candidate beside
the source, and leaves the active transcript and note unchanged. A later
explicit promotion changes the transcript pointer and clears the stale note
pointer while preserving all prior files.

The merged session-core suite passed 441 unit tests, 17 process-fault tests, and
8 doc tests. The desktop suite passed 133 tests, the shell contract passed 5,
and the UI suite passed 24. The worker suite passed 222 tests with 10 platform
skips. One long note-budget case first hit its 240-second deadline while Rust
and Python suites ran concurrently; it passed alone in 191 seconds, and the
full worker suite then passed alone in 188 seconds. The internal worker runtime
rebuilt and verified. `Yawn Preview.app` rebuilt and passed bundle verification.
The installed production app remains unchanged.

**Wave 2 product-integration receipt.** Meeting review now exposes the retry as
a separate action after the generated note and before the full transcript. The
comparison sheet shows the current and candidate transcripts side by side on a
wide window and stacked on a narrow one. Withheld rows remain withheld. It also
shows a closed set of product-authored recording-quality observations; raw
metrics, device names, paths, audio digests, and receipt text do not cross into
the web view.

Retry creation is bound to the current transcript, capture session,
microphone audio, and system audio. The worker rereads and hashes all retained
sources before and after transcription, then the desktop rereads the immutable
candidate by its content address. Creation does not change the active
transcript or note. Only **Use retry** changes the transcript pointer. That
decision clears the stale note pointer, preserves every prior file, and leaves
note regeneration as a separate reader action. **Keep current** records the
decision without changing the meeting. Closing the sheet records nothing.

Pending comparisons survive restart through the same session-core authority
that owns retry decisions. Discovery is bounded and fails closed on multiple,
stale, released, corrupt, cross-meeting, or malformed candidates. The desktop
handler, capabilities, permission schemas, and shell contract expose the same
three commands.

Meeting review now projects recording-device context as `identified` or
`unknown`. The web view receives neither the microphone name nor its index or
host API. The identified state says only that an identity was recorded. The
unknown state offers **Check audio input** without claiming which device was
used.

Retained microphone and system audio now have separate **Play** controls. Each
control spends one opaque, source-specific handle. Native code rechecks and
opens the private artifact, passes that open file to fixed `/usr/bin/afplay`
through standard input, and owns the child until it completes or is stopped.
No recording path, bytes, digest, or generic file or shell authority crosses
into the web view. While that child is active, scheduled retention defers its
storage pass. The next pass may release due audio only after the child has been
stopped or reaped.

Failed playback and the current stable reopen errors now offer a refresh for
the selected meeting. That refresh reloads Meetings, falls back to the home
view if the meeting disappeared, and otherwise mints fresh single-use handles.
The build command list, handler, capability, generated permissions, and shell
contract now expose the same speaker-correction and playback commands.

The combined session-core suite passed 456 unit tests. Its 17 process-fault and
8 doc tests were unchanged from the prior combined gate. The desktop suite
passed 142 tests, the shell contract passed 6, the build matrix passed 5, and
the UI suite passed 37. The worker suite passed 222 tests with 10 platform
skips. The internal worker runtime rebuilt and verified. `Yawn Preview.app`
rebuilt and passed bundle verification.

Independent read-only review first found a missing speaker-correction command in
the build manifest, spent playback handles without a recovery action, three
unmapped stable reopen errors, unsafe use of file descriptor 3, and scheduled
retention that could release audio while the owned player still held it. The
fixes synchronized the command contract, completed the exact map for current
stable reopen errors,
moved playback to inherited standard input, and made active playback defer the
retention pass without touching storage. A second focused review reported zero
material findings in that scope. Those reviews inspected source and assertions;
the executable test evidence above comes from the build session.

**Rendered Preview receipt — 2026-08-17.** The first running Preview process
predated the rebuilt bundle, so it was not accepted as release evidence. After
confirming that it had no capture artifact open, Computer Use closed it and
launched the exact packaged `Yawn Preview.app` by its full path. The fresh
process completed its local-engine check and rendered the transcript-ready
state.

The live walk observed Recent meetings, title search, reopening a retained
meeting from Meetings, **Generate note**, progressive **Full transcript**
disclosure, separate **Play microphone** and **Play system audio** controls,
the separate personal-notes area, and **Back to meetings** continuity. Settings
rendered on-device storage, speech and note model controls, and separate
microphone and system-audio access controls. The walk returned to Meetings
without changing any setting.

The retry comparison, quality and device messages, playback state and Stop,
recovery actions, speaker-correction save and regeneration, decide-later close,
keep, promote, and post-promotion note state still lack rendered evidence. No
real meeting audio was played, no private content was reproduced, no field was
edited, and no permission was accepted to manufacture a passing check.

**Rendered fixture receipt — 2026-08-17.** The exact signed `Yawn Fixture.app`
with bundle id `com.ninochavez.local-meeting-notes.fixture` ran against the
exact marker-bound root
`/Users/nino/Library/Application Support/com.ninochavez.local-meeting-notes.fixture`.
The fixture contained only deterministic invented content and two 8-second
silent WAVs. A verified public speech-model copy cleared startup. This is
fixture evidence, not a release claim.

The direct Computer Use walk observed Recent meetings, title search, opening
the meeting detail, **No meeting note yet**, **Generate note** progressive
**Full transcript**, separate personal notes, and a retry review. The retry
modal showed current versus candidate transcript, silence and low-input
caution, no material clipping issue, unavailable background-noise evidence,
and a bounded device-identity disclaimer with no raw device name. **Decide
later** closed without mutation and preserved **Review retry**. Microphone
playback showed active state and **Stop**; explicit Stop returned **No recording
is playing**. **Back** returned to the same Recent meetings list.

At that point, the walk had not observed **Keep current**, **Use retry** or
promotion, speaker-correction save, regeneration, generated-note/source-link
and post-promotion state, recovery-toast journeys, or the exact installed
production package. No Settings change, permission change, recording, private
content, or real meeting audio was used.

**Fresh-state fixture support.** A marker-bound archive command now moves the
exact synthetic fixture to a recoverable same-parent archive under the canonical
writer lock. It uses an exclusive atomic rename, refuses existing destinations,
and never deletes or automatically reseeds data. Separate fresh roots were used
for the **Keep current** and **Use retry** rendered walks.

**Rendered fixture mutation receipt — 2026-08-17.** **Keep current** closed the
comparison, reported that the retained transcript was kept, replaced **Review
retry** with **Retry transcript**, and preserved that state across Back and
reopen. On a second fresh root, **Use retry** promoted the candidate and returned
to transcript-only detail with **Generate note** and **Retry transcript**.

The promoted fixture had no generated note, so this walk did not prove clearing
an existing note. It exposed a copy defect instead: the success toast claimed a
previous note was cleared even though none existed. Promotion continuity across
Back and reopen also remains to be observed after that copy is fixed. The three
synthetic states are preserved as recoverable fixture roots; no private content,
real audio, Settings change, permission change, or recording was involved.

**Retry-promotion copy correction — source-verified, 2026-08-17.** The success
toast no longer claims a clearing event. It now reads `The retry transcript is
now current. Generate a new note when you're ready.` in every case. The command
is handed one settled outcome and no note fact, and a replayed promotion clears
nothing because the pointer already moved, so conditional copy would have been
guessing rather than reporting. The mapping was extracted into a pure function
with exact-copy coverage for all three outcomes.

Promotion's own note invalidation was untested until now — every retry fixture
started with `current_note: None`, including the one behind the earlier rendered
walk. A new session-core test gives the meeting a real two-file note revision,
promotes, and proves the pointer is cleared, the lifecycle returns to
`TranscriptReady`, and the note bytes survive on disk. Removing the clearing
line makes that test fail, so the pre-decision modal warning is now evidenced
rather than assumed.

**Rendered confirmation — 2026-08-17.** On a freshly seeded root, Computer Use
directly observed the corrected toast reading exactly `The retry transcript is
now current. Generate a new note when you're ready.` on a meeting whose note
card said **No meeting note yet.** Promotion continuity then held across **Back
to meetings** and reopen: the promoted title persisted, the note stayed absent,
and the control read **Retry transcript** rather than **Review retry**. The
library list refreshed to the promoted title, so the decision command still
invalidates the preview library.

**Modal warning defect found by that walk — fixed, 2026-08-17.** The same
journey exposed a second instance of the defect. The pre-decision modal stated
`Using this retry clears the current generated note.` on a meeting that had no
note, asserting an object that did not exist. The earlier source reading had
recorded this warning as correct; the rendered walk is what falsified that.

The warning now renders only when a note exists, gated on the same
`transcript-only` signal the note card reads, compared strictly so an unknown or
still-loading note state keeps the warning. Suppression was chosen over new
copy: with no note there is nothing to warn about, and the detail view already
offers **Generate note**. Computer Use confirmed the modal now shows the
comparison and the three decisions with no warning bar.

The note-existed direction of that warning is not rendered evidence. It rests on
the strict-equality guard and a mechanical assertion in the UI suite, because no
synthetic note fixture exists yet. Clearing an existing note likewise remains
proven in source and not yet rendered.

**Developer ID-signed local-bundle receipt — 2026-08-17.** The unreleased source
build at `target/release/bundle/macos/Yawn.app`, built from app source commit
`97ff8c9`, passed the `internal-alpha` admission check. The bounded local lane
then signed 169 Mach-O files with Developer ID and hardened runtime, rebuilt the
runtime manifest from those signed bytes, signed the outer bundle, and passed
strict signed-bundle verification. The verifier confirmed identifier
`com.ninochavez.local-meeting-notes`, Team `34VZ63G58M`, and the hardened-runtime
flag. The signed outer bundle's CDHash is
`5b878972e00a7a42657fda2abf06076f00b388eb`.

This is signed local evidence, not a release. The lane did not check a notary
profile, submit to Apple for notarization, staple, build a DMG, run Gatekeeper,
install, or replace an app. The recorded before/after comparison found the
existing DMG and checksum unchanged by inode, size, and modification time. It
found the same for the installed `/Applications/Yawn.app` binary. Developer ID
signing still uses Apple's secure timestamp service, so “local” does not mean
offline.

**Remaining release gate.** The base completed-meeting reopen and Back journey
now passes in the separately identified Preview package. The Developer
ID-signed local bundle remains unnotarized and uninstalled. The remaining
stateful journeys above require a further disposable fixture or explicit human
review, followed by the same walk against the exact installed production
package, before these changes can be called shipped.

**Remaining slice 1b gate.** Source acceptance is complete: note generation uses
the corrected attribution while source links stay bound to the retained
transcript. Shipping still requires the non-destructive packaged journey above
to be completed with a disposable fixture or explicit human review.

## 1. Show and correct who said what

**Outcome.** The visible transcript names each known speaker. A reader can fix a
wrong or missing label once and apply the correction to every matching turn.

**Why this is first.** A useful summary can still assign a commitment to the
wrong person. Speaker uncertainty also made the earlier in-person 630 meeting
hard to review. Fixing attribution improves the transcript, the note, and every
future search result.

**Existing foundation.** Transcript turns carry an optional `speaker` value.
The copied and rendered transcript now expose it, and the visible label opens
the meeting-local correction control.

**Build sequence.** Slice 1a renders the existing attribution without changing
stored data. Slice 1b adds a durable correction operation, a corrected
projection, and note regeneration from that projection. The visible label must
land first so the correction control has an honest object to edit.

**Current state.** The durable correction operation, corrected transcript
projection, exact-group UI, reopen behavior, source-label recovery, and note
regeneration from the corrected projection are built and packaged in Preview.
The rendered save-and-regenerate release journey remains unverified.

**Scope.**

- Render `Me`, `Them`, named speakers, and `Unattributed` explicitly
- Let the reader rename one speaker and apply that name across the transcript
- Preserve the original attribution and store the correction as a separate,
  reviewable operation
- Regenerate the note from the corrected transcript projection
- Keep uncertain attribution visibly uncertain

**Done when.**

- A rendered transcript shows the same speaker information as its copied form
- One correction updates every intended turn and no unrelated turn
- Reopening the meeting preserves both the correction and the original source
- A regenerated note uses the corrected attribution and retains source links
- Tests cover named, unnamed, withheld, and incorrectly grouped turns

**Not in this slice.** General-purpose room diarization. Yawn currently requires
the operator to attest that they are the only person near the microphone. A
multi-person in-room mode needs a separate capture, consent, and evidence
decision before the product can claim it works.

## 2. Keep a local vocabulary of names and jargon

**Outcome.** The reader can teach Yawn the names, organizations, products,
acronyms, and preferred spellings that matter in their meetings.

**Scope.**

- Add, edit, disable, and delete vocabulary entries on this Mac
- Support direct replacements such as a mistaken spelling to the intended one
- Offer **Always correct this** after an explicit transcript correction
- Apply vocabulary through a deterministic corrected projection; never rewrite
  the retained source artifact silently
- Show where an entry was applied and allow the reader to undo it
- Warn when the vocabulary becomes large enough to increase overcorrection risk

**Done when.**

- A saved entry survives restart and affects the next eligible transcript
- The original transcript remains recoverable and byte-identical
- A correction cannot change text outside its declared match
- Removing an entry does not rewrite past source artifacts
- Note regeneration uses the current corrected projection and preserves evidence

**Boundary decision.** Vocabulary lives in a dedicated meeting-review sheet.
It does not widen Settings beyond audio access and model storage.

## 3. Explain recording quality and allow a safe retry

**Outcome.** When a transcript looks wrong, the reader can inspect verified
quality evidence, confirm whether a microphone identity was recorded, listen to
retained audio, and compare a source-bound retry before blaming the speech or
note model. A recorded identity does not prove that it was the intended input.

**Current state.** Capture persists integrity evidence, resolved microphone
identity, and a separate quality block. Meeting review exposes only verified,
product-authored quality guidance. The retry comparison, explicit keep or
promote decisions, retained-audio playback, and bounded device explanation are
built and packaged in Preview. The exact fixture renders review, quality,
device, playback, Keep-current, and Promote-retry states. Clearing a generated
note that actually exists is proven in source but not yet rendered; note
regeneration and exact installed-production-package verification remain open.

**Scope.**

- Say whether a microphone identity was recorded without exposing its name,
  index, or host API, and link an unknown state to **Check audio input**
- Surface silence, clipping, low input, and material background noise
  as separate observations
- Let the reader play retained audio while the retention period allows it
- Retry transcription from the unchanged recording
- Compare the retry with the current transcript before replacing the active
  projection
- Keep every retry versioned and source-bound

**Done when.**

- Each quality message names the observed condition and the next useful action
- Integrity evidence and quality guidance remain separate fields and labels
- Retrying transcription cannot alter or delete the recording
- The reader can keep the current transcript when the retry is worse
- If retained audio is gone, the interface says that the meeting cannot be
  retranscribed

## 4. Route each problem to its exact repair

**Outcome.** A warning or failed state ends with one action that opens the place
where the reader can fix it.

Examples:

- Recording device not verified → open audio access guidance
- Misspelled name → open the local vocabulary
- Wrong speaker → open speaker correction for that turn
- Incomplete transcript → listen to retained audio and retry transcription
- Weak generated note → regenerate without discarding the current note first

This is an in-app routing contract, not an email campaign. A generic help page
does not satisfy the outcome when Yawn already knows the failing meeting and
operation.

**Current state.** Note retry, Meetings, withheld-turn restoration, local
vocabulary, speaker correction, transcript retry, failed playback, and the
current stable reopen errors have real destinations. Future warnings remain
blocked until their owning controls exist.

**Done when.** Every recoverable warning has one primary action, lands at the
correct meeting or control, and preserves the failed artifact until the repair
succeeds.

## 5. Find source passages across meetings

**Outcome.** The reader can ask a narrow question such as “Where did we discuss
the launch date?” and receive quoted passages linked to the meetings that contain
them.

**Existing foundation.** A local corpus-question command, bounded vector store,
quoted-passage response, coverage report, and source-handle path already exist
in the desktop backend. The command is deliberately not registered in the
shipped interface.

**Decision.** Do not ship the semantic source finder yet. The committed scale
probe selected 128-word windows as the least costly valid storage unit, but its
best arm retrieved 7 of 10 intended meetings and only 3 of the 5 questions that
exact search could not answer. That probe explicitly did not establish human
usefulness. The shell contract continues to keep the command out of the product.

If a later disposable-fixture experiment supplies stronger evidence, the
candidate journey is Meetings → **Find something I remember** → quoted result
cards → the existing meeting detail anchored at the source turn. It must reuse
the existing transcript reader rather than create a second answer or chat
surface.

**Required boundaries.**

- Search and answer generation stay on this Mac
- Every answer quotes and links to retained source passages
- Coverage gaps are stated instead of hidden
- Search remains unavailable during capture when it would compete with the
  transcription worker
- No email, calendar, Slack, or web context is added through this slice

**Not yet.** Global semantic source finding and live “What did I miss?”
summaries. The current evidence does not establish the first as useful. The
worker boundary also makes capture the priority, and a live summary would add an
uncapped interpretation path while the source is still being recorded.

## Ideas this roadmap does not adopt

The Wispr Flow emails also promoted writing styles, reusable dictation snippets,
spoken list formatting, automatic call detection, connected services, and AI
tool integrations. Those ideas solve different jobs or cross Yawn's current
privacy and consent boundaries.

They stay out unless a later product decision supplies a Yawn-specific user
need, a local authority model, and evidence that the added complexity improves
the private meeting-note job.

## Invariants across every slice

- Original recordings and transcripts are never silently rewritten
- Corrected and generated material remains distinguishable from source evidence
- A failed retry never replaces the last usable result
- Capture, transcription, corrections, vocabulary, and notes remain local
- Recording still begins through an explicit consent and start action
- UI copy states what happened, what is uncertain, and what the reader can do
- A feature is not marked shipped until it is rendered and verified in the
  packaged desktop app

## Evidence behind the direction

Two Wispr Flow emails received on 2026-08-08 and 2026-08-10 introduced the
comparison. Their useful product ideas were checked against the linked public
material and Yawn's current source. Private email contents are not copied into
this repository.

- [Why transcription quality fluctuates](https://wisprflow.ai/post/transcription-quality)
  describes microphone changes, background noise, input volume, audio review,
  transcription retry, transcript feedback, and dictionary overcorrection.
- [Wispr Flow Notetaker](https://wisprflow.ai/notetaker) presents named speakers,
  one-step relabeling, topic-organized summaries, source-linked questions, and
  live catch-up as its product direction.
- [Yawn's product brief](product-brief.md) owns the local storage, consent,
  evidence, note, and interface boundaries this roadmap preserves.
- The current desktop sources own the implementation facts:
  [`main.js`](../apps/desktop/ui/main.js),
  [`view-model.mjs`](../apps/desktop/ui/view-model.mjs),
  [`main.rs`](../apps/desktop/src-tauri/src/main.rs), and
  [`capture_health.py`](../apps/desktop/runtime/spike/capture_health.py).

External URLs were resolved and checked against the cited claims on 2026-08-16.
