# Decision — what runtime executes the note-generation model

## Implementation plan — retire new generation and preserve saved content (2026-09-30)

The operator approved planning and implementation through dispatch. The change
blocks new generation and note-model downloads. Existing AI drafts, personal
notes, source links, recording, transcription and transcript export stay usable.
No model files or meeting data are deleted. This is a source change; installation
and release are separate work.

1. Add one native retirement policy. Refuse generation and install commands
   before they accept an operation, write meeting state, read model weights,
   start a download or launch inference. Keep the saved-note reader available.
2. Remove generation and regeneration controls, note-model Settings, retry
   prompts and stale generation status. Preserve truthful transcript-promotion
   warnings and the personal-note editor.
3. Test stale commands with valid source/model fixtures and side-effect
   sentinels. Test saved-note/source reading, personal-note saving and transcript
   access. Render the changed production frontend with synthetic data and get
   a separate cold screen review.
4. Integrate only the worker's owned changes into the parent worktree. Compare
   branch tips, verify cleanup and record source versus installed/released state.

Implementation is complete in source in an isolated worktree using the required
Operator routing gate. The source starts at `72bce78` with the uncommitted
evaluation edits preserved. The worker owns the desktop source and focused
tests; the parent owns product/design documentation, integration and final
judgment. No additional local-model experiment is authorized by this work.

### Implementation verification — 2026-09-30

New generation and note-model downloads refuse locally before model access,
operation acceptance or inference. Existing data and model assets were not
deleted. Saved drafts still open their cited transcript passages. Generation-only
vocabulary controls were removed while their stored entries remain intact.

| Check | Observed result |
|---|---|
| Desktop Rust tests | 272 unit tests and two integration groups of 6 passed; 284 total. |
| Frontend tests | 155 passed. |
| Native WKWebView retirement checks | Transcript-only, saved-draft, historical failed-draft and Settings fixtures passed in dark and light modes. Personal-note saves, transcript open/export and saved-draft source inspection passed against the synthetic bridge. |
| Editor-preservation harness | Capture, library, smoke and sheets modes passed. |
| Geometry contract | Retained toolbar, typography, widths, spacing and editor checks passed. The approved retirement removes generation controls and their dependent absolute vertical positions. |
| Negative control | Injecting a known generation button caused exit 1 and failed the absence check. |
| Cold screen review | Initial findings corrected; revised screens accepted by a separate reader. See [the review](evidence/screen-reviews/note-retirement-2026-09-30-cold.md). |

Native unit-test compilation temporarily linked existing staged/bundled resources
because this worktree had no runtime staging. Those links were removed after
the tests. This is not runtime freshness, packaging or release evidence.
No real recording, speaker inference, model download or filesystem export ran.
No commit, push, installation, signing or release ran. The original evaluation
checkout and its uncommitted source remain unchanged.

## Current decision — no tested local solution meets Yawn's requirements (2026-09-30)

No tested local replacement meets the registered accuracy, completion and speed
requirements. Recommend deprecating the current generator until a replacement
earns acceptance. Keep local recording, transcription and transcript export.
This is a decision about the tested Yawn configurations on this Mac, not a claim
that local models cannot write useful notes. The operator authorized continued
viability testing; the three new candidate experiments below are now closed.

| Candidate and configuration | Measured time | Finding |
|---|---:|---|
| Gemma 4 26B A4B, native reasoning before the existing inventory/note calls | 64.8 seconds | The model immediately closed an empty thought channel. The adapter refused it before an inventory or note existed. Note quality is unmeasured. |
| Qwen3.5 35B A3B, one complete-source pass with native reasoning | 101.7 seconds | A complete final note arrived within the target, but consequential factual errors reject it. |
| Qwen3.5 27B, one complete-source pass without reasoning, fuller accuracy instructions | 300-second child timeout; failure receipt 307.4 seconds | No complete note. Saved partial output has more detail but prematurely settles historical-order migration. |

The Qwen MoE time includes loading, generation and parent verification. The
dense timeout receipt measures from child launch through the failure check;
it does not provide a complete-note time or terminal generation metrics.

### The final dense candidate did not finish within five minutes

This trial revisited the earlier strongest local candidate with a materially
different configuration: general accuracy instructions, less forced compression,
a 4,096-token final-answer allowance and thinking disabled. It used the pinned
Qwen3.5 27B 4-bit model at revision
`45797d2985a12c55e6473686e9ea91b95e959553`. The complete source entered one
native call; gold ledgers and frontier notes never entered its prompt.

The parent terminated the child at the registered 300-second deadline. There
is no completed proposal or normal-stop receipt. The saved raw prefix contains
1,792 tokens and ends during the final follow-up list. Its decoded tokens match
the saved text exactly. Source, model, runtime and run-code provenance remained
unchanged. The two unused trials are closed; no retry or allowance increase ran.

The partial draft covers more material than the fast MoE answer, including
gift-card codes versus internal IDs, the original liability timeline, numeric
store credit, retained coupon value and Nino's Avalara assignment. Its complete
accuracy and completeness cannot be scored because it never finished. One
observed issue is independently source-checkable: it says closed old orders
will remain in the legacy system with no migration required. The transcript
settles how to move the open portion of backordered orders, but leaves historical
order migration under discussion pending customer-journey analysis. The partial
draft turns that open choice into a final policy.

The tax section is headed "Reported Concern," which supplies some attribution,
but its wording states integration failure more categorically than the discovery
discussion. That wording needs care; these notes establish no fact about the
actual Avalara integration. The timeout alone fails the registered acceptance
rule, regardless of this partial-content review.

### The fast Qwen note changes what the meeting said

The final answer says the existing tax integration calculates tax on the whole
order rather than at product level. The transcript explicitly says tax is
calculated at product level. The concern is whether splitting an order changes
its exemption eligibility incorrectly. The note also reverses the example's
direction, calls the two brands examples of states, and invents assignments to
a Gift Card Team and a Development Team. These errors are sufficient to reject
the draft without treating completeness as the only quality measure.

The run used the pinned 4-bit conversion at revision
`1e20fd8d42056f870933bf98ca6211024744f7ec`. All 477 source turns entered one
native call. The model generated 2,526 reasoning tokens and 834 answer tokens,
then stopped normally. Neither source nor response was truncated. Child loading
and inference took 83.0 seconds; parent verification brought the total to 101.7.
MLX peak allocation was 20.8 GiB. That allocation and the separately polled
process RSS are different measurements.

Every frozen critical item was checked against the complete final answer and
primary source quotes. Operating diagnostics: one fully covered, twelve partly
covered, seven missing and one incorrect. The model's reasoning text contributes
no coverage to the meeting note. Review is by the parent model, not blind review
or human acceptance. Exact source, output and native-token hashes were checked.

### The evaluation bar needs an honest comparison

The registered rule requires every critical item to be fully and faithfully
covered, zero consequential invented claims, a normal stop and total elapsed
time of at most 120 seconds on each of three sources. It remains unchanged.
However, this completeness bar is stricter than the earlier usable-draft
comparison. Reopening the original supplied OpenAI and Claude notes showed that
they also omit material or strengthen assignments and certainty. The OpenAI
reference omits the customer-migration requirement, the customer-facing gift
code versus internal-ID distinction, and Shane's explicit metafield-testing
assignment. Therefore failing completeness alone does not prove inferiority to
every frontier draft. The Qwen note's consequential errors independently reject
it. The reference models, prompts and settings are unknown; this is not a
controlled ranking of providers.

All three experiments are closed after their first failure. Their unused
attempts were closed, not transferred or retried. Private evidence remains in
`yawn-evaluation/constrained-json/thinking-preparation` and
`yawn-evaluation/qwen-moe-one-pass`, plus
`yawn-evaluation/qwen-dense-full-note`: registrations, unchanged inputs and ledgers,
run receipts, exact native outputs, source snapshots and source-based review.
The installed app is unchanged. Research source and decision changes remain
local and uncommitted. No generator was installed, feature removed or release
published. The earlier generic comparisons and failed Gemma source-check
designs remain historical evidence below; no old failed allowance was reopened.

### What would change this decision

A complete local candidate must preserve the meeting's material facts and
explicit assignments without consequential inventions, meet the registered
120-second total target on this Mac, and then pass both held-out meetings.
Different hardware, a different latency requirement or a materially different
generation method could change the result, but none is validated here. No
remaining inference follows from these closed experiments. Preserve their
evidence and stop repeated formatting changes or unbounded model downloads.

Focused comparison/adapter checks passed before inference: 36 tests plus one
additional native full-note transport test that both accepts a normal stop and
rejects length truncation. Actual dense-model tokenizer checks verified complete
source admission, disabled reasoning and a context-overflow negative control
on all three sources. These checks establish execution behavior, not note quality.

## Prepared scope — Gemma native reasoning (superseded by results above)

Test Gemma 4's native reasoning mode before considering another model download.
The runner has explicitly disabled it in every recent trial. The complete
non-thinking draft was fast enough but failed material coverage, dates and
resolution status. Reasoning may help with those failures; that is a hypothesis,
not a quality result.

Google documents `enable_thinking=True` and separate native thought/answer
delimiters. The pinned model's actual tokenizer renders that mode correctly.
Google's documentation establishes how to enable it, not that it produces good
meeting notes. [Thinking mode](https://ai.google.dev/gemma/docs/capabilities/thinking),
[prompt formatting](https://ai.google.dev/gemma/docs/core/prompt-formatting-gemma4).

| Approach considered | Decision |
|---|---|
| Native reasoning with the current model and two-call design | Prepare this test. It uses retained weights and isolates an untested inference setting. |
| Download Qwen3.5 27B again | Defer. The earlier record reports a stronger but incomplete migration draft and 251.1 seconds including verification. Its private raw evidence was removed during cleanup; held-out quality remains open. |
| Add another fact-and-resolution extraction call | Defer. It adds latency and another generated factual bottleneck. Previous self-checks did not reliably catch omissions. |

The opt-in research flag `--thinking` requires the existing inventory/note
design and constrained JSON. Stage instructions, source bytes, final schemas,
validators, model/runtime pins, seed, temperature and scoring rules stay fixed.
Each call may generate up to 2,048 native reasoning tokens before its existing
4,096-token answer allowance. Both reserves count toward context admission.
The source is never shortened. This additional reasoning allowance is part of
the proposed experiment; it is not transferred from a closed trial.

The JSON decoder starts after the model's native closing thought token. It does
not constrain the thought text. This small adapter is needed because the pinned
Outlines processor would otherwise constrain the response to JSON from its first
token. The runner uses the model's native delimiters and MLX streaming. It never
forces a closing marker, repairs an answer or retries. Missing, empty, unfinished
or oversized reasoning is refused. Each second-stage prompt receives only the
inventory answer, not the previous thought text. Raw tokens and the exact final
answer are saved separately for review.

### Offline checks passed; generated-note quality is still unmeasured

The protocol, adapter and decoder suite ran 57 tests: 54 passed and three
real-tokenizer checks were skipped in that invocation. Two additional checks
with the pinned tokenizer passed. They tested the actual thinking template,
shared source prefix, native channel boundary and final-answer grammar. Valid
answers pass; unknown source IDs, incomplete JSON and null conditions fail.
Synthetic tests also refuse absent/empty thought channels and exhausted budgets,
preserve partial raw output on stream failure, and confirm two separate stage
processors. These checks establish execution boundaries, not semantic accuracy.

Private preparation is saved in
`yawn-evaluation/constrained-json/thinking-preparation`: unchanged inputs and
frozen ledgers, model/runtime plans, prompt counts, code snapshot, preparation
receipt, registration and task record. No weights were loaded, no model was
downloaded and no inference ran. The installed app remains unchanged.

### Proposed allowance — three sequential trials, stop at the first failure

The prepared order is migration, academic-1, academic-2. At most six native
calls may run, with no retry or allowance increase. Each trial keeps the
300-second child deadline, 120-second total target and 4 GiB free disk reserve.
Any structural, semantic, resource or target-time failure closes the remaining
attempts. A complete refused raw note is still reviewed for accuracy. Reasoning
text is never counted as meeting-note content.

Every frozen critical item must be fully and faithfully covered. Consequential
unsupported dates, owners, commitments or certainty prevent acceptance. The
parent must compare the new migration answer against the complete non-thinking
v3 answer and primary source, then apply the same requirements to the two
academic meetings if the first trial passes. This uses an existing baseline,
not a simultaneous controlled timing comparison. Review is model-assisted and
not blind; human acceptance and product integration remain separate.

This test needs a fresh explicit inference allowance under the operator's
closed-experiment rule. The prior failures remain closed. Preparation does not
authorize native calls, app integration, installation or release.

## Latest result — the note arrived within two minutes, but is not trustworthy (2026-09-30)

Keep this Gemma 4 candidate out of Yawn. The section-reference trial completed
both calls in **97.7 seconds**. Both responses were valid JSON and stopped
normally; the inventory passed section-reference validation. The finished raw
note failed the required inventory-reference overlap check. Independent review
also rejects its accuracy and coverage. The first-failure rule closes this
experiment after one attempt, with two unused attempts closed.

| Stage | Output tokens | Completion | Result |
|---|---:|---|---|
| Inventory | 571 | Normal stop | Valid ordered sections and section-local references. |
| Note | 1,773 | Normal stop | Complete raw draft; six inventory topics lacked required note-reference overlap. |

Reference overlap is a structural requirement, not a semantic score. A topic
can appear in prose while citing other turns, so those six failures do not mean
six topics are absent. The independent transcript comparison is the reason
this draft cannot be adopted even if that overlap requirement were relaxed.

### The complete draft fails accuracy and coverage

The next-meeting claim says the 13th at noon Central, but its `date` field
invents **`2025-05-13`**. The spoken source says two weeks, the 13th, noon to
one Central; it supplies no calendar month or year. The note also asks whether
gift cards work universally across channels as an unresolved question, even
though the transcript explicitly answers that one card works in all channels.

The draft omits customer-migration data and business rules, the unresolved
store-credit compliance question, conditional legacy returns/refunds and Narvar
confirmation, delayed security/app DB registry information, and the ownership
Google Sheet with sandbox checks. It captures Avalara discovery but drops the
explicit Nino/team assignment and urgency. It loses the original gift-card
liability timeline and the confirmed metafield-permission solution. Its project
status attaches the one-to-two-week estimate to backend readiness; the source
says the backend already transacts and estimates connecting front and back.

As operating diagnostics, all 21 frozen critical items were reviewed: three
fully covered, thirteen partly covered and five missing. These are judgments
about the complete **refused raw draft**, not an accepted end-to-end note or a
new frontier-model comparison. The unsupported date alone breaches the scoring
rule against consequential invented dates.

The inventory also fails semantically despite valid locations. It calls the
first section non-material even though that section contains the agreed
open-order treatment and customer-journey follow-up. Its section-seven tax
discovery label points to gift-card intermediary-service discussion. Code can
constrain source locations without making the model understand those passages.

### Preserve the evidence and stop formatting-only revisions

Loading and execution inside the child took 84.1 seconds; total time includes
parent verification. The model, source and runtime provenance remained unchanged.
Parent review independently recomputed artifact hashes, replayed inventory and
note-shape validation, compared all frozen critical items with the complete
note, and checked the consequential errors against source passages. No response,
claim, date, ID or citation was repaired. No accepted candidate artifact exists.

Private evidence is retained in
`yawn-evaluation/constrained-json/section-reference-preparation`: registration,
raw responses, native stage records, receipt, coverage audit, full draft
diagnostic review, decision and exact run source. Neither academic source ran.
The installed app is unchanged; source and decision edits are local and uncommitted.

This run establishes that this candidate can complete both calls within the
time target. It still fails meeting-note accuracy and coverage. Further
evaluation needs a hypothesis that addresses those semantic failures, such as
a different model or a different source-reading approach. More formatting-only
revisions are not justified by this result. The closed allowance authorizes no
retry or app integration; this result does not establish that all local models
are unsuitable.

## Tested change — enforce each section's references during generation (2026-09-30)

The research decoder now permits only the correct source IDs in each inventory
row. A fixed schema per array position binds the section name, row order and
reference set. The JSON output shape stays the same. The three-reference cap,
complete transcript, final-note stage and independent validator remain in place.
This is `ordered-inventory-note/3`; the previous trial stays closed.

Three representations were checked against the installed Outlines Core library.
Modern JSON Schema `prefixItems` supports fixed ordered rows and was selected.
Legacy tuple-style `items` is unsupported. A keyed object works but would change
the model response, validator and artifacts without improving these constraints.
The chosen design retains the compact finite-ID grammar, applied separately to
each section. It adds no call or output allowance.

The actual decoder first reproduced five failing controls: it accepted
cross-section references, reordered rows and duplicate rows. After the change,
these controls fail as intended. Valid boundary IDs pass; missing/extra rows
and IDs outside the source remain blocked. The protocol, adapter and synthetic
decoder suite passed 48 tests with one tokenizer-only skip. Three real-tokenizer
checks also passed, covering section binding, the three-reference cap and shared
full-source prefix.

Offline replay of the unchanged failed inventory shows the old grammar accepts
it and the new grammar rejects it. The independent validator still reproduces
its original section-reference refusal. A separate synthetic endpoint-reference
inventory passes; changing a reference to another section makes it fail. Neither
the original response nor the synthetic control is an accepted meeting note.

All three full-sized inventory grammars compiled with the pinned tokenizer.
Preparation times were 4.4, 6.2 and 6.0 seconds. The new inventory prompts contain
19,454, 27,929 and 31,817 tokens, respectively; the actual note prompt is counted
again after inventory generation. The larger first-stage schema fits the pinned
model's context, but its effect on complete-run timing is unmeasured. Source and
frozen-ledger identity checks passed. No weights were loaded and no inference ran.

These constraints prove reference locations. They do not prove that an inventory
label describes those passages, that all material topics were found or that a
final note is accurate. Private preparation evidence is retained in
`yawn-evaluation/constrained-json/section-reference-preparation`.

A fresh allowance was authorized for at most three sequential trials: migration,
then the two academic meetings only if prior trials pass. Each permits two calls
capped at 4,096 tokens, a 300-second hard deadline, a 120-second target, a 4 GiB
disk reserve and no retries, stopping at the first failure. One migration trial
ran both calls and failed reference coverage and independent semantic review.
The two unused attempts are closed. No previous closed experiment was reopened.
The installed app is unchanged; these source and decision edits remain local
and uncommitted.

## Previous result — the shorter first pass finished, but its references were misplaced (2026-09-30)

Keep this candidate out of Yawn. The compact-reference migration trial finished
its inventory with **1,727 tokens** and a normal stop, resolving the observed
truncation in this run. It then failed source-section validation. No note call
ran. The full failed run took **84.1 seconds**; complete-note timing and quality
remain unmeasured. The registered first-failure rule closes this experiment
after one attempt, with two unused attempts closed.

The inventory returned all eight section rows in the expected order and kept
each topic at three references or fewer. Parent review found **19 of 92
reference occurrences outside their declared section**. Replaying the
canonical validator reproduces `inventory reference is not offered in its
source section`. For example, section two should cover `turn-0065` through
`turn-0128`, but its customer-journey topic cites `turn-0045`, `turn-0050` and
`turn-0063`. These are real offered source IDs placed in the wrong section;
the location error is not evidence that the underlying discussion is invented.

The decoder limits reference count and permits only IDs from the complete
source. It does not restrict each section row to that section's IDs. The prompt
requests section-local references, while the validator enforces that requirement
after generation. This gap allowed a structurally complete response that the
runner correctly refused. No IDs, rows or outputs were repaired.

Loading and execution inside the child took 70.3 seconds. The stage record
reports 23.4 seconds for shared source prefill and 27.5 seconds for generation.
These components belong to the refused inventory, not a successful note.
The source, model and runtime provenance stayed unchanged. Parent review
independently recomputed child artifact hashes and preserved the exact run
source. Private evidence is retained in
`yawn-evaluation/constrained-json/compact-reference-preparation`: the
registration, raw inventory, stage record, receipt, reference audit and decision.
Neither academic source ran. The installed app is unchanged; source and
decision edits remain local and uncommitted.

The next design change to evaluate is to enforce section-specific reference
sets during generation. It must retain the independent validator and full
transcript. Whether that produces accurate notes within the time target remains
open. This closed allowance authorizes no retry or product integration; the
result does not establish that all local models are unsuitable.

## Tested change — limit the first pass to representative references (2026-09-30)

The research runner now permits one to three representative source IDs per
inventory topic. The constrained decoder blocks a fourth reference; the
validator independently refuses longer lists before the note call. The prompt
asks for anchors to the material statement, condition or final resolution.
Both calls still receive the complete transcript. The final note can cite
additional turns. No transcript, topic count or final-note reference list was
shortened. This is `ordered-inventory-note/2`; the prior run's exact source is
preserved with its receipt.

The saved failure made unbounded reference lists the strongest explanation
for its truncation. Verbose topic labels and repeated topics remained possible
contributors. This change addressed only the reference-list size. The native
trial above now confirms that the inventory completed in this run; it does not
establish semantic quality or predict a completed note's time.

In an offline size comparison, the saved fragment's eleven complete topic
objects retained their labels while reference lists were synthetically reduced
to first, middle and last anchors. Their serialized token count fell from
3,213 to 406 with the pinned tokenizer. This compares those objects only;
it is neither a complete inventory nor new model output. The refused fragment
was not repaired or accepted, and anchors were not judged for semantic support.

Regression tests first reproduced the validator and decoder accepting four
references. After the change, both refuse the fourth. The protocol, adapter
and synthetic-decoder suite passed 46 tests with one tokenizer-only skip.
Two additional real-tokenizer checks passed, including the reference limit
and shared full-source prefix. Full-sized inventory grammar preparation and
source/ledger identity checks also passed for all three sources. These checks
load no model weights and perform no inference.

Private preparation evidence is retained in
`yawn-evaluation/constrained-json/compact-reference-preparation`. The earlier
failed experiments stay closed. The user authorized a fresh allowance for at most
three sequential trials: migration, then the two academic sources only if
previous trials pass. Each permits two calls capped at 4,096 tokens, a
300-second hard deadline, a 120-second target, a 4 GiB disk reserve and no
retries, stopping after the first failure. The compact-reference migration trial
used one call and failed. Its two unused attempts are now closed.
The installed app is unchanged; these changes are local and uncommitted.

## Previous result — the source review ran out of space before writing a note (2026-09-30)

Keep this candidate out of Yawn. The newly authorized source-inventory trial
failed on the migration meeting. Its first call reached the **4,096-token
output limit** and stopped mid-response. No second call ran and no note was
produced. The full run took **128.7 seconds**, exceeding the 120-second target.
The first-failure rule closes this experiment; its two unused attempts stay closed.

The native stage record reports `finish_reason: length`. Loading and execution
inside the child took 114.3 seconds. The inventory stage reports 24.6 seconds
for the shared source prefill and 70.5 seconds for generation. These are
components of the failed run, not a successful note-generation benchmark.

The raw fragment contains long lists of consecutive source IDs. It starts
section six of eight and truncates within a reference string. There are 457
reference occurrences in the fragment; their quoted strings occupy 5,027 of
its 7,384 characters. This is a diagnostic of output cost, not useful coverage.
The schema requires references but sets no maximum per topic, and the prompt
does not limit them. The response was neither repaired nor treated as a
complete inventory. Semantic note quality remains unassessed.

The source, model and runtime provenance remained unchanged during the run.
Parent review independently recomputed the child artifact hashes. Private
evidence is retained under `yawn-evaluation/constrained-json/` in
`inventory-note-preparation`: the registration, raw inventory, stage record,
receipt, trial decision and exact run source. No public academic source ran.

This result rejects the tested inventory representation for adoption. It does
not establish that local models cannot write useful notes. A next design would
need compact, bounded evidence references before another trial. That change
could reduce output cost; its accuracy and timing benefit are untested.
Increasing the output limit alone would not address the missed time target.
No retry, model change or product integration is authorized by this closed run.
The installed app is unchanged; these decision updates are local and uncommitted.

## Previous result — constrained output helps, but this candidate still fails (2026-09-29)

Keep the candidate out of Yawn. The new constrained-output run produced valid
JSON for both calls, but the source check failed and the complete run took
**157.5 seconds**, exceeding the 120-second target. Loading and inference inside
the child took 143.2 seconds; the total also includes parent verification. The
registered first-failure rule closes this experiment after one attempt. The two
unused attempts cannot be run under this closed authorization.

| Stage | Output | Native completion | Result |
|---|---:|---|---|
| Draft | 1,872 tokens | Normal stop | JSON and draft structure passed. |
| Source check | 2,655 tokens | Normal stop | JSON parsed, but source binding failed. |

Neither stage hit the 4,096-token cap. The runner refused the result with
`check claim quote is not an exact offered-source quote`. Independent comparison
of the raw checker response with the offered source found 25 invalid bindings
among 62 quotations. The validator allows a nonempty exact excerpt of the
stated turn; the other 37 quotations meet that rule. An earlier full-turn
comparison counted 37 differences but included 12 valid shortened excerpts.
The corrected audit replays the actual validator and still reproduces refusal.
Invalid quotes merge, change or mislocate source text. The checker also added
a `C25` assessment absent from the draft.
It reported no material omissions. No quotes, IDs or responses were repaired.

The complete draft is now available for diagnostic review, even though no
complete accepted draft-and-check result exists. Parent review against the
frozen migration ledger found material problems: it reopened an answered
cross-channel gift-card question, weakened an agreed open-order treatment into
an option, and omitted customer-migration rules, the unresolved store-credit
compliance question, conditional legacy returns and delayed security information.
The model's `supported` labels did not establish a useful meeting record.

This result shows that constrained decoding can solve the observed JSON syntax
problem. It does not establish that Gemma 4 or all local models are unsuitable.
The tested two-call design still fails accuracy and timing requirements. A next
design should resolve evidence text from source IDs in code and address material
coverage before writing the note. That is a proposal for a separately bounded
evaluation, not authorization for retries, a larger cap or product integration.

The private `Application Support/yawn-evaluation/constrained-json` directory
retains the pinned weights, source files, frozen ledgers, raw responses, native
stage records, run receipt, quote audit and draft diagnostic review. The run's
source/model/runtime provenance remained unchanged. The public academic sources
were prepared but not run. The installed app is unchanged; the code and decision
updates are local and uncommitted.

## Tested design — review source sections, then write the note

The next research version is implemented behind `--inventory-note`, which also
requires `--constrained-json`. Preparation used no model inference. The user
subsequently replied "next" to the explicit request for a fresh three-trial
allowance. That authorized the bounded trial reported above; it is now closed
after its first failure. Earlier failed experiments remain closed.

### The source review comes before prose

We compared three designs against the saved failure:

| Design | What changes | What the saved failure shows |
|---|---|---|
| Draft, then check IDs only | Code copies quotes; checker emits source IDs. | Removes quote transcription and reduces response size, but still relies on the checker that missed material omissions. |
| Ordered topic inventory, then note | Review each source section before prose; write from the whole source and inventory. Code copies citations. | Directly targets missing late topics and eliminates generated quotations. Chosen for the next bounded evaluation. |
| One-pass note with a checklist | A single call asks for topics and note together. | Could reduce latency, but provides no separately inspectable source inventory. Keep as an untested alternative. |

The chosen design takes code-resolved citations and closed source-ID schemas
from the first option. It retains the separate source review from the second.
It does not add a third call or adopt the one-pass shortcut.

The first call must return one ordered row for every consecutive 64-turn source
section. Each row identifies material topics with source IDs from that section,
or explains why it contains no material content. The second call receives the
complete transcript and that untrusted inventory. It writes concise claims
while preserving final resolutions, conditions, explicit assignments and dates.
Both calls use the same token-identical source prefix with separate cache state.

Code rejects missing, duplicate or reordered sections, references outside their
section, unknown final source IDs and declared inventory topics without a note
citation. The constrained decoder permits only offered source IDs. Code copies
citation text from the immutable transcript; the model does not generate quotes
or assess its own claims as `supported`.

These gates prove reference locations and declared inventory coverage. They
cannot prove that the model found every material topic, that a cited passage
supports a claim, or that the final note is accurate. Output remains an
unverified research draft. A note with correct citations can still be wrong.

### Preparation evidence and the closed allowance

Synthetic tests cover both native calls, independent decoder/cache state,
source-specific schemas, withheld-content exclusion and refusal controls.
Thirty-nine protocol/adapter checks and five decoder checks passed without
inference. The new schemas and shared prompt prefix also passed checks using
the pinned Gemma tokenizer. Full-sized grammar compilation succeeded for all
three sources: both stages together took 9.4, 10.6 and 10.0 seconds in these
preparation observations. A compact finite regex encodes exactly the offered
source IDs; the initial large-enum preparation was cancelled while spending
CPU time in Outlines Core. These timings measure grammar preparation only.

Canonical-loader preflight exposed an invalid import shape in the previously
prepared public sources before any model call. New inputs retain all raw turns
and use Yawn's existing QMSum loader. Its standard normalization preserves
speech, removes nonspeech annotation markers and omits annotation-only turns.
The corresponding source ledgers were rebound to those canonical turns and
validated: 724 turns and 723 turns. The original source files and closed
experiment's registration remain unchanged. The new import manifest records
both raw and canonical hashes; no reference answers enter prompts.

Offline replay copied the saved draft's 87 cited source turns without changing
their text. With the independently frozen ledger injected as a **synthetic
inventory**, the new gate rejects that incomplete draft. The ledger was never
passed to a model. This replay tests the gates; it does not measure a new model's
inventory, note quality or timing. Private evidence is retained in
`constrained-json/inventory-note-preparation`.

The separately authorized allowance was at most three native attempts using the same
Gemma revision, runtime and three meeting sources, with the new canonical imports
and independently authored ledgers frozen before inference.
Each attempt allows two calls of at most 4,096 tokens each, a 300-second hard
deadline, a 120-second end-to-end target and a 4 GiB disk reserve. Run migration
first, then the two public academic sources only if prior attempts pass. Stop on
the first format, resource, timing or semantic-quality failure. No retries,
repair rounds, larger model or product integration are included. All three
must pass to support further adoption work. One attempt ran, using only the
inventory call. It failed before note generation. The two unused attempts are
closed; this allowance cannot reopen any failed experiment.

## Source-check candidate decision — not ready for Yawn (2026-09-29)

Do not connect the full-transcript/source-check candidate to the app. The
pinned Gemma 4 runner reached real inference five times across the three
sources, but **zero of five runs produced a complete draft and valid source
check**. The content quality of an accepted note therefore remains unmeasured.
This rejects the tested protocol for adoption; it does not show that no local
model can produce good meeting notes.

| Source and pass | End-to-end seconds | Result |
|---|---:|---|
| Architecture, initial | 106.3 | Draft stopped normally, but its fenced body was invalid JSON. |
| Presentation, initial | 86.6 | JSON parsed, but `conditions: null` violated the required draft shape. |
| Migration, one allowed correction | 123.1 | Draft reached the 4,096-token cap and was truncated. |
| Architecture, one allowed correction | 187.4 | Draft schema passed; the stopped source-check response was invalid JSON. |
| Presentation, one allowed correction | 121.9 | Draft stopped, but its body was invalid JSON. |

All five runs stayed inside the 300-second deadline. Only two met the 120-second
target, and those durations are failed-run timings, not timings for an accepted
note. A sixth attempt started before the first meeting's model output, then the
4 GiB disk guard stopped it; it yielded no note and no quality result. That
resource failure and the five inference runs exhaust the six-attempt allowance.
The single generic correction was used. No further model run or product
integration is authorized by this experiment.

The responses commonly arrived inside one JSON Markdown fence. The adapter
already accepts exactly that envelope without repairing JSON, fields, IDs, or
source references. Failures remained after removing the envelope. The corrected
architecture draft contained 18 schema-valid claims, but its checker response
was malformed, so no claim received an accepted independent source check.
There is no end-to-end note to score for missing topics, unsupported owners, or
preserved conditions. Treat semantic quality as **unassessed**, not as a pass or
as proof that local note generation is impossible.

The private run directory under Application Support/yawn-research was removed
during the September 29 disk cleanup. This decision record retains the aggregate
results, but the private drafts, source ledgers, and run receipts are no longer
available for reinspection. The runner code remains in `spike/`. The installed
Yawn behavior is unchanged. Retain this candidate as unadopted research; any new
model or protocol evaluation needs a new bounded authorization because the
registered allowance is exhausted.

## Constrained-output evaluation — closed after its first failure

We tested the same Gemma 4 candidate with generation constrained to the required JSON
schema. The operator approved the proposed three-attempt follow-up with "go".
This new experiment does not reopen the closed six-attempt allowance. Its purpose
is to determine whether enforcing the output shape lets the existing two-call
approach produce useful, accurate notes.

The research runner now forwards the configured deadline to its child. Two
regression checks exposed the mismatch before the fix. The previous evaluation
used the 300-second default, so that fix does not change its recorded result.

### Verify the library bridge before requesting inference

The pinned [MLX LM 0.31.3 generation source](https://raw.githubusercontent.com/ml-explore/mlx-lm/v0.31.3/mlx_lm/generate.py)
exposes `logits_processors` to constrain token selection. The fetched
[Outlines MLX documentation](https://raw.githubusercontent.com/dottxt-ai/outlines/c52af8472c6fe8a607a733ec11b709476af77a96/docs/features/models/mlxlm.md)
documents JSON-schema generation, and its
[MLX adapter source](https://raw.githubusercontent.com/dottxt-ai/outlines/c52af8472c6fe8a607a733ec11b709476af77a96/src/outlines/models/mlxlm.py)
passes its processor into MLX generation. These sources were fetched successfully
on 2026-09-29. The implementation now uses Outlines 1.3.3 and Outlines Core 0.2.14
with the unchanged MLX 0.32.3, MLX LM 0.31.3, and Transformers 5.17.0 pins in an
isolated Python 3.13.14 research environment. Dependency hashes are retained in
`spike/requirements-constrained-note.lock`.

The research adapter's opt-in `--constrained-json` mode uses the library processor
with native MLX streaming. It requires the decoder package pins before parent
launch and again in the child. Each stage receives fresh processor state; the
source prefill receives none. Plans and stage records bind the decoder and schema
hashes. Existing source-ID, exact-quote, and checker-row validation remains active.

Thirty-five checks passed without model inference. Four further decoder checks
passed against the exact Gemma tokenizer revision, and its chat-template/shared
prefix check passed separately. These checks demonstrate valid JSON completion
is allowed while malformed JSON, `conditions: null`, extra fields, and invalid
claim kinds are blocked. A grammatical but unknown source reference still fails
Yawn's validator. They do not measure what the model will generate or its accuracy.
The exact model weights were subsequently downloaded into the private research
directory. The model file set and installed runtime are pinned before inference.

The fetched Outlines streaming wrapper yields text strings. The integration must
preserve native MLX terminal events and timing, using its processor with native
streaming if needed. Do not infer a normal stop from a string that parses. If
this bridge cannot preserve provenance and stop reasons, stop preparation before
launching an experiment. The installed research bridge preserves native terminal
events; integration into the product remains outside this experiment.

### Authorized allowance and adoption criteria

The experiment allowed at most three native attempts, run sequentially. Each could make only the
existing draft and checker calls. Keep the 4,096-token limit per call, 300-second
hard deadline, 120-second end-to-end target, and 4 GiB disk reserve. Failures
consume attempts. No repair, correction round, automatic retry, or larger model
is included. Stop after the first format, resource, or quality failure.

The first source is the retained migration meeting. Two further sources must be
real, unseen transcripts selected and available before any inference starts.
The deleted architecture and presentation receipts cannot serve as current
inputs or evidence. Freeze source hashes, the original prompts and schema,
model revision, runtime, and library bridge before the first call. Change only
the decoder in this experiment; a simpler schema or larger cap would be another
experiment.

Write source-based required facts and prohibited inferences before seeing each
output. A passing note must preserve every material decision, condition, and
supported follow-up in that frozen rubric, with no unsupported owner, date, or
decision. Compare the retained meeting with the supplied frontier notes against
the transcript; those generated notes are comparison outputs, not ground truth.
Passing JSON and a model's own `supported` label do not establish accuracy.

All three notes must finish with valid draft and checker responses and meet the
quality and timing criteria. A single success can justify further research but
cannot establish readiness for the app. Preserve raw outputs, the frozen rubric,
source copies, and receipts outside Git until independent review is complete.
The investigation task record is saved outside Git at
`~/Library/Application Support/yawn-evaluation/constrained-json/task-record.json`.
It retains the closed experiment, failed approaches, and human authorization under
`gemma4-constrained-json-v1`. One attempt ran and failed; the stop rule closes
the experiment with two attempts unused. The migration
transcript is preserved with its source hash. Its accuracy ledger and the common
scoring rubric are frozen after a full reading of all 477 turns. The existing
ledger validator accepts the source quotes and rejects a deliberately corrupted
quote. This checks evidence locations; the parent's full-source reading supplies
the semantic judgments. The operator's subsequent "go" authorized proceeding
with public sources for the two missing inputs. The two shortest complete
academic transcripts in the [QMSum test set](https://github.com/Yale-LILY/QMSum)
are now preserved at dataset commit `83d7768c1f2b4dfeb091385d3dc7e239b8e5bb7e`:
724 turns / 8,844 words and 726 turns / 12,565 words. Their required-fact ledgers
were checked against exact source quotes. Parent review corrected a missing
explicit agenda owner and added omitted proposal, example, progress and access
topics before generation. All three sources, ledgers, the common rubric and
runner pins are frozen in the private `preregistration.json`.

These public academic meetings are new inputs to this evaluation. Exposure in
the model's training data is unknown. They test general meeting-note behavior;
they cannot replace acceptance on additional private business meetings. The
source transformation preserves every turn in order and does not use QMSum's
reference answers. The first native attempt used the retained migration meeting.
Its failure closed the evaluation without running the public sources.

## Current decision — local notes merit a redesign, not retirement (2026-09-29)

Keep local note generation under development. Gemma 4 is the best candidate
for the next implementation design on this 36 GiB M3 Max. It completed both
full-transcript trials quickly and produced substantially fuller notes than
the installed feature. It did not pass the accuracy requirement on either
meeting. A model swap alone is not an adequate fix.

The proposed retirement was withdrawn before commit, packaging, or installation.
No replacement has been adopted. The installed app and its existing draft-note
feature remain unchanged. This comparison closes the authorized experiment;
it does not authorize more model runs or a release.

### What the comparison established

The current feature is both slow and incomplete. On the retained migration
meeting it took **25:48** and produced six claims with no follow-ups. All seven
completed full-transcript local trials were much faster. The remaining problem
is reliable coverage, ownership, conditions, and decision status.

| Local model, same generic instructions | Migration meeting | Held-out meeting | Quality judgment |
|---|---:|---:|---|
| Installed Gemma 3 12B QAT 4-bit, research path | 117.1 s | 145.8 s | Broad topics, but unreliable assignments and substantial omissions on both |
| Gemma 4 26B A4B 4-bit | 41.7 s | 60.3 s | Fuller, readable drafts; both still need material correction |
| Qwen3.5 9B 4-bit | 66.6 s | 83.7 s | Added consequential claims and commitments; unsafe as meeting records |
| Qwen3.5 27B 4-bit | 224.5 s | No note | Strongest local migration draft, with material omissions; held-out quality unassessed |

Times are single observations of model loading plus inference, with other work
active on the Mac. They exclude downloads and the parent's model-file verification.
For Gemma 4, total research-run time including that verification was 55.7 and
74.1 seconds. For Qwen 27B it was 251.1 seconds on the migration meeting. Total
research-run time was not recorded for the earlier four runs; these are not
installed-app timings or averages.

Peak MLX allocator use was 10.30–11.47 GiB for Gemma 3, 15.89–16.71 GiB for
Gemma 4, 7.43–8.37 GiB for Qwen 9B, and 18.84 GiB for the completed Qwen 27B
run. These figures do not include all process or system memory. The Qwen 27B
held-out run stopped when free disk fell below the registered 4 GiB reserve;
it is an execution failure, not a poor-note result or proof that the model
cannot summarize that meeting. The receipts preserve separately sampled child
RSS and the resource failure.

Seven runs returned complete outputs with no input or output truncation. All
eight registered attempts are used. No failed arm was retried, and no prompt
was tuned after seeing the outputs.

### Why Gemma 4 is a candidate, not an accepted replacement

The migration draft covered the major topics, but reopened an already agreed
order treatment, missed conditions and an explicit follow-up owner, and changed
some document-work wording. The held-out draft missed a consequential finding
about context preparation, blurred the clean-data boundary, and failed to
preserve the limits on sharing evaluation details and operating an internal
tool. Shorter waiting time does not repair those errors.

Qwen 27B's migration draft preserved most major topics without the consequential
unsupported claims flagged in the other local outputs. It still omitted the
explicit tax-investigation owner and conditional legacy-order servicing. Its
longer latency and incomplete second arm prevent choosing it as the preferred
replacement from this experiment.

The next engineering direction is a full-transcript draft with a separate
source check for material decisions, conditions, and supported follow-ups.
Unknown owners must remain unknown. A source link proves a location; it does
not prove the linked text supports the claim. Any new design needs its own
bounded evaluation on additional meetings before adoption. This is a proposed
direction, not an implementation or permission to extend this experiment.

### How the supplied frontier notes compared

The supplied OpenAI note was the strongest record of the migration meeting.
It preserved the material decisions, caveats, explicit investigation owner,
and schedule. It passed the fixed draft criterion in this review. It still
needs ordinary reader review; a model-assisted assessment is not operator
acceptance or verification of the participants' platform and legal assertions.

Claude covered the material topics particularly well, but its action table
added owner assignments the transcript did not establish. Gemini captured many
of the main topics, but added owners and made conditional legacy servicing
sound settled. Their exact prompts, model versions, settings, and runtime
were not provided. They are useful reference outputs, not a controlled model
benchmark or ground truth. Frontier references were available only for the
migration meeting, so they have no held-out comparison here.

### What was checked, and what could change the decision

The locked protocol supplied two complete retained transcripts, containing
477 and 872 turns, to each local model once. Each run used the same generic
prose instructions, seed 1, temperature 0.2, thinking disabled, an 1800-token
output limit, and a 300-second deadline. Native GPU runs were serial and their
children were denied network access before MLX import. The research runtime
was separate: Python 3.13.14, MLX-LM 0.31.3, MLX/MLX Metal 0.32.3, and
Transformers 5.17.0. Synthetic compatibility checks passed for all four models.
They establish execution, not note quality.

| Pinned model | Revision |
|---|---|
| Installed Gemma 3 12B QAT 4-bit | `66fc51ef25778c03d33c4c8bc446973d062e73f4` |
| MLX Gemma 4 26B A4B 4-bit | `0d77464eeb233a2da68ebf9d7dc4edaac7db956d` |
| MLX Qwen3.5 27B 4-bit | `45797d2985a12c55e6473686e9ea91b95e959553` |
| MLX Qwen3.5 9B 4-bit | `8b2b98c00a6b4d291155e4890773ca8f769aee53` |

Source-derived criteria were frozen before generation. The first keyword-based
criteria were rejected because they missed later substantive topics and treated
screen sharing as future work. The replacement criteria require material
coverage, supported claims, preserved uncertainty, and readable separation of
outcomes from proposals. Unsupported consequential assignments, commitments,
or certainty prevent a usable-draft verdict.

A separate reviewer scored eleven anonymous drafts: seven fresh local outputs,
the installed Yawn note, and the three supplied frontier references. Producer
identities and timings were withheld until the scores were saved. The parent
personally read all drafts and checked primary source passages; it recognized
the reference notes and was not fully blind. Both reviews are model-assisted,
not human acceptance. The dispatcher receipt distinguishes the requested
reviewer route from whatever model metadata the host actually reports.
For this review, the host reported neither the child model nor reasoning effort;
those identities remain unverified.

The parent validated every review's input hashes, complete item identities,
summary arithmetic, note excerpts, and exact source quotes. A deliberately
corrupted quote failed validation. Semantic review also found a ledger error:
the source explicitly names Nino for customer-journey work although the ledger
left that owner blank. That attribution was restored in a separate parent audit.
The audit also removed excess coverage credit for the held-out clean-data,
context-quality, and internal-tool boundaries. Original scores were retained;
these corrections do not make either local candidate pass both meetings.
Mechanical checks establish evidence locations, not semantic truth.

The disk guard was added after storage pressure during staging, before the
larger inference arms. It preserves the original 4 GiB reserve and does not
change the generation prompt or score criteria. Eight runner checks passed,
including context-overflow refusal, child-and-descendant deadline termination,
and resource refusal before and during execution. With the operator's direct
permission, all newly downloaded candidate weights were removed between tests
or after their final run. Installed and preexisting models were preserved.

Private transcripts, notes, source snapshots, scores, and receipts were stored
outside Git under Application Support/yawn-research/
local-note-comparison-2026-09-29 during the comparison. They were removed
during the September 29 disk cleanup, so the aggregate findings and protocol
identity below are preserved here but the private evidence cannot be reopened.
The protocol identity was
`9b7b22176bf7cb6aa14504d3392672f3891cc43decebcb5949caa0b6921ab51c`.
No private meeting content belongs in this public decision record.

A future candidate would change the adoption decision if it produced a useful,
source-supported draft on both evaluation and additional unseen meetings within
an agreed time and memory budget. Two meetings and one trial per model cannot
establish general superiority or that no local model is suitable.

### Earlier evidence remains scoped to its experiment

| Same installed Gemma 3 model, earlier migration-meeting probes | Elapsed | Result |
|---|---:|---|
| Existing full-transcript JSON prompt | 82.697 s | Incorrect source references; seven accepted claims, no actions |
| Expanded JSON prompt | 286.871 s | Missing topics, uncertain owner, excessive/incorrect references |
| Plain prose without citations | 79.154 s | Better coverage, but unsupported owners and a concern presented as a proven defect |

These single-run probes did not justify retiring all local generation. The
prose trial bypassed the JSON decoder; its zero admitted claims is not a quality
measurement. The expanded trial included item types the decoder did not accept;
its rejection rests on the raw note, not decoder counts. Decoder changes between
these probes prevent a controlled comparison of admitted-item counts.

The installed path repeatedly prefills hundreds of candidate excerpts and then
synthesizes isolated anchors. Its format restricts detail, and its action filter
discarded spoken contractions. The narrow contraction regression fix remains;
it does not repair incomplete context or incorrect source references. Earlier
implementation evidence already recorded 20–55-minute single-candidate runs
(`notes/EVAL.md`, "product elapsed budget re-derived for batch size 1").

The earlier Qwen3-4B, Granite micro, and Qwen3.5-4B candidate-classification
experiment remains closed under its own rules. It did not test the present
full-transcript task or these larger models. Neither experiment's failure
justifies reopening the other without a separately authorized contract.

### Source correction

The retirement controls, download blocks, UI copy, and retirement-only tests
were removed. Their earlier passing tests verified the proposed blocking
behavior, not the decision to retire. The private probe harness and the
spoken-contraction regression fix remain. The installed app was never changed.

## Runtime choice

Date: 2026-08-14. Status: decided. Owner: operator (posture admitted in
`notes/EVAL.md`, "Product decision, 2026-08-14").

## Decision

Note generation runs on **MLX-LM inside the existing supervised Python
worker child** — the same bundled CPython 3.12 runtime, process supervision,
and network-denied posture that already executes mlx-whisper. Model weights
are **not bundled**; they ship through the existing model-catalog path:
sha256-pinned, revision-locked download into the private `models/` directory
with an install receipt and re-verification on every use, exactly as the
1.61 GB whisper-large-v3-turbo does today.

No new runtime class enters the product. Rust keeps owning safety, storage,
digests, and the process boundary; Python keeps owning model execution.

## Why not the alternatives

- **Ollama (the research runtime).** The measured 11/13 recall ran on
  ollama + gemma3:12b, but ollama is an external daemon reached over
  localhost HTTP. The shipped note path enforces
  `SECURITY_NO_NETWORK_ACCESS` on the child and a `connect-src ipc:`-only
  CSP, and the product promises no external dependency the operator must
  install. The research pin cannot ship as-is; ollama remains the
  research-harness transport only.
- **Apple Foundation Models.** A 4,096-token window against a measured
  ~14–19k-token prompt need, and a macOS 26 floor against the shipped
  minimum of 14.4. Revisit if either constraint moves; not a candidate now.
- **A Rust inference crate (candle, mistral.rs, llama.cpp binding).** Would
  introduce a new dependency class the codebase deliberately avoids — no
  Rust crate executes any model today — for no measured benefit over MLX-LM
  on the same Metal hardware.

## Consequence: re-measurement is the first build step

Ship-gate condition 1 (`notes/EVAL.md`) requires recall ≥ 11/13 re-measured
after **any** change to model, prompt, view, or decoding. Moving from
ollama/gemma3:12b to an MLX-LM quantization is such a change. Therefore no
app code lands until a preregistered MLX-LM arm of the capture classifier
(`notes/capture_classifier.py`) clears the gate on the operator-locked
ledger. If the MLX arm cannot clear 11/13, this decision reopens.

## Integration seams, in dependency order (mapped 2026-08-14)

1. MLX-LM arm of the research harness clears the ship gate (above).
2. `worker/note_bridge.py` — lift the validator-only refusals; the manifest
   already carries `generator` and `models` fields.
3. Implement `NoteProjector` (`crates/session-core/src/note_projection.rs`)
   backed by the hardened one-shot child in `note_projector_process.rs`,
   replacing `UnavailableProjector`.
4. Register `regenerate_note` (`apps/desktop/src-tauri/src/product_facade.rs`)
   in `generate_handler!` with its permission TOML and capability entry,
   mirroring the transcript-model-settings commit shape.
5. Add the `Operation` variant, progress state, and worker heartbeat in
   `crates/session-core/src/protocol.rs` + `worker/main.py`.
6. Extend `model-catalog.json` and `model_store.rs` with a note-model role;
   reuse `model_download.rs::install` unchanged.

Known risk to manage at step 5: a ~7–8 GB 4-bit model and whisper weights
contend for unified memory in one MLX process; note generation must run
after transcription completes and release the whisper runtime first (the
release hook already exists in `worker/transcription.py`).

## Correction, 2026-08-14 — network denial is structural, not enforced

Two independent implementation sessions verified the same fact: the
`SECURITY_NO_NETWORK_ACCESS` flag in `note_projector_process.rs` is
`kSecCSNoNetworkAccess`, a code-signature *validation* option (it stops the
signature assessment from fetching revocation data over the network). It does
not deny the child process a socket, and the bundled Python's entitlements
carry no sandbox. Today the no-network promise for the generator child is
structural — its bytes are digest-pinned, so what runs is known — but nothing
enforces it at runtime. Before the note generator ships, the launcher must add
real enforcement (a sandbox profile or equivalent) or this document's
"network-denied posture" language must be weakened to match reality. Tracked
as a pre-ship requirement, not folded silently into any slice.

**Resolved, 2026-08-15 — enforcement is now real.** The launcher's `pre_exec`
calls `sandbox_init` with the named `no-network` Seatbelt profile between fork
and exec, so the kernel denies the interpreter child every socket. The sandbox
survives exec and wraps no binary, so the pinned interpreter path and the
code-signing admission chain are unchanged; application is fail-closed (a
child the profile cannot be applied to does not launch). Verified before
landing: the profile denies a local TCP connect (EPERM) and leaves mlx GPU
compute working, both probed empirically; a characterization test in
`note_projector_process.rs` pins the behavior (the connect an unsandboxed
control child completes is denied in the sandboxed child). `sandbox_init` is
deprecated in the headers with no replacement for this shape — sandboxing a
child the parent is about to exec — which is the "why not canonical"
sentence: the supported alternatives (App Sandbox entitlements, a
NetworkExtension content filter) either sandbox the wrong process or are
system-wide machinery for a per-child guarantee.

## Merge checklist (recorded 2026-08-14, owner: the session that merges)

Four finished branches wait on the measurement gate, all runtime-agnostic:
bridge admission (`worktree-agent-a331fcc4b9544b303`, 15 commits), projector
(`worktree-agent-a4d535796e42db89a`, 10 commits), command/protocol
(`worktree-agent-a3028d9a18ca4af29`), catalog (`worktree-agent-a7cd23fd64579a7c6`).
Items no branch owns, which must not evaporate at merge:

1. **Shared cross-language fixture pass** (after all branches land): one
   non-ASCII result frame; `invalid_result_frames` rows for
   `strictly_sorted`, digest length, locator cap, empty claim text
   (genuine Rust-side gaps); control-character and surrogate rows as
   parity locks. Both closing agent reports state it identically.
   **Done, 2026-08-15**: `note-projection-v1.fixture` gained
   `valid_results[2]` (the fixture's one non-ASCII claim text, parsed on
   both sides) and six `invalid_result_frames` rows — unsorted locators,
   truncated claim digest, four locators, empty claim text,
   control-character claim text, lone-surrogate escape — each generated
   from the same template as the parsing valid row and mutated in exactly
   one field, so the rejection is attributable. Both consumers exercise
   them: the Rust generic loop plus a `results[2]` assertion, the Python
   structural loop.
2. **Network enforcement before ship** (see Correction above): sandbox
   profile or equivalent on the generator child, or weaken this doc's
   posture language.
3. **Seam-6 alignment**: the projector's one-id-per-role model mapping is
   pinned by a characterization test and must widen when the catalog's
   sharded-weights role merges; `worker/build_manifest.py` needs the
   `note-runtime-generate.json` sibling constant.
   **Done, 2026-08-15**: `note_runtime_models` now derives one identifier
   per file from `catalog.note_models` (shards carry their designator:
   `note-generator-weights-00001-of-00002`; duplicate identifiers collapse
   the derivation to empty — honest refusal, never an ambiguous match), and
   `build_manifest.py::note_runtime_model_id` mirrors it, each side pinned
   to the same expected list by its own test. The catalog carries the
   registration-pinned gemma-3-12b-it-qat-4bit entry (snapshot 66fc51ef…,
   six files), hosted on the same R2 bucket as the whisper weights. A
   six-file tree was measured to load and tokenize byte-identically to the
   full snapshot on the product rendering path before the entry landed —
   the fixed two-turn rendering never calls the tokenizer's chat template,
   so the files the catalog cannot express are behaviorally inert.
4. **Registration adoption**: the official gated run adopts the ±2 view,
   the ollama two-turn rendering, the MLX runtime identity, and — if the
   pruning arm passes its second-capture validation — the pruning stage,
   as one preregistered registration change with a fresh operator lock.
   **Satisfied, 2026-08-15**: the candidate-first program's registration
   (digest 98dcbbd9…, `notes/EVAL.md`) carries the ±2 view, the two-turn
   rendering, the MLX runtime identity, and the budget-fitted pruner;
   four corpora hold official passes under it, each with a recorded lock
   (operator-delegated, supersession chains in the packets).
5. **Wiring recipe** for `admit_note_projector` at `library_reader.rs:338`
   is in the projector agent's report (catalog from
   `verified_model_catalog`, resource root from `StorageContext`, cache
   the admission decision off the hot rebuild path).
   **Done, 2026-08-15**: `library_snapshot_with` (the one production
   rebuild site) now injects `admitted_note_projector(state)` — catalog
   from `verified_model_catalog`, manifest paths from the resource root
   via constants owned by `note_projector_process.rs`, successful
   admission cached for the process lifetime, refusal re-derived per
   rebuild so a model installed mid-session admits without a restart
   (`admit_note_projector` now returns `Option` to make that split
   possible). Today it resolves to `UnavailableProjector` — no generate
   manifest is bundled and the catalog carries no note-model role — and
   activates mechanically once the catalog entry ships.

## Merge record (2026-08-14, late night)

All four branches merged to local main under the operator's standing
delegation, after the measurement gate cleared via the batch-size-1
adoption (notes/EVAL.md). One cross-branch fix at merge: the projector's
test catalogs gained the catalog branch's `note_models` field. Full lanes
green post-merge: session-core, desktop, UI, worker, root. Worktrees
removed. Remaining from the checklist: the shared cross-language fixture
pass (open, owner still unassigned), network enforcement (pre-ship),
seam-6 widening for sharded weights (pinned by characterization test),
catalog entry + model hosting (needs the external-publish step the
delegation excludes), and app-side admission wiring (in progress:
bridge product alignment + real MLX generator child).

## Merge record addendum (2026-08-14, later)

The bridge product-alignment branch merged to local main under the same
delegation: the generate lane now runs the product registration (±2
window, batch size 1, prune-then-budget on the pruned set, 3600 s
deadline bound to the registration at startup), and the real
`worker/note_generator_mlx.py` child exists with its sync obligation
enforced as a byte-identity test against `notes/product_run.py`. The
generate manifest builder lands unwired by decision: Rust refuses an
empty-models manifest and the signed catalog carries no note-model role
yet, so wiring waits on the catalog entry. All lanes green post-merge.
(The id constraint this paragraph originally stated — exactly
`note-generator-config` and `note-generator-weights`, sorted — described
the pre-widening derivation; seam 3's 2026-08-15 closure in the checklist
above owns the current per-file scheme.)

Caveat carried from notes/EVAL.md: the product registration this lane
aligns to is under an open refusal — official run 1 missed the recall
gate at the registered ±2 window. The lane binds to the registration
mechanism, not the numbers; an amendment moves the digest and the
startup binding check refuses any half-updated bundle loudly.
(Superseded 2026-08-14/15: the amended registration 98dcbbd9… holds
official passes on four corpora; see EVAL.md.)

## Merge record addendum (2026-08-15) — the catalog entry shipped

The operator pointed out the hosting question answered itself: the whisper
weights already serve from the `yawn-releases` R2 bucket, so the note model
ships the same way. Landed as one thread: the six-file catalog entry
(checklist item 3 above), `main()` writing the generate manifest, and the
packaging flip —
the five note runtime resources moved from a forbidden list to a required
one in both Tauri configs, `prepare-preview-bundle.sh`,
`sign-notarize.sh`, and `verify-release-bundle.py`, each flip pinned by its
updated test. The Settings window gained a "Note model" section backed by
three new commands (`note_model_settings`, `install_note_model`,
`remove_note_model`); `model_download::install` was generified over
`DownloadableModel` exactly as its trait comment documented. Unlike a
speech model, the note model may be removed while active — "no note model"
is an ordinary state the library renders honestly — so removal deactivates
first (`deactivate_note_model`) and drops the cached projector admission.

Hosting status at the time of this addendum: four of the six objects
(config, weights index, tokenizer, tokenizer config) are uploaded and live
on R2. The two weight shards (~8 GB) are **not uploaded** — the transfer was
stopped mid-flight when the operator moved to a metered connection — and no
public byte-count or downloaded-hash verification has run for any object.
Until the shards land and all six objects verify against the catalog pins,
an `install_note_model` against the published catalog fails at download or
at digest verification; the catalog entry is code-complete but not yet
servable. Upload and verification resume on operator clearance.

**Resolved, 2026-08-15 evening**: on operator clearance the aborted
multipart was cleaned, both shards uploaded, and all six objects were
downloaded end-to-end from the public URLs — byte counts and sha256
digests match the catalog pins exactly. The catalog entry is servable;
the hosting requirement of the distribution runbook is met.

## Design — the generation invocation chain (2026-08-15)

The last unbuilt stretch: nothing invokes generation. `regenerate_note`
is registered but `accept_regeneration` refuses by design; the worker
refuses `note.create` (no admitted generator); the Rust child launcher
only speaks the `project` role. Three candidate structures were
developed and compared before committing to one.

**Candidate A — generation inside the worker.** Admit `note.create` into
the worker's operation set and have the worker itself run the model
(spawn `note-generator-mlx.py` or load MLX in-process). One transport
(the existing supervised worker), and the `note.create` heartbeat and
Rust progress plumbing already exist. Rejected on one decisive fact: the
kernel no-network sandbox and the code-signing admission chain are
applied by the Rust one-shot launcher's `pre_exec`; the standing worker
is not launched that way, so the model child would run without the
guarantee the bridge architecture was built to provide. Sandboxing the
long-lived worker instead is a far larger change with its own risks.

**Candidate C — the bridge generate lane publishes end-to-end.** Extend
`validator.generate` to assemble the note/2 pair and write it. One
launch, sandbox intact. Rejected because it breaks two deliberate
boundaries the code states in its own comments: the validator "writes no
note" by design, and durable publication (immutable pair, meeting
record, lifecycle) is the worker's storage discipline — duplicating it
into the one-shot bridge is drift in the most hardened component.

**Candidate B — split at the points boundary (chosen).** Two steps, each
component doing exactly what it was built for:

1. A generate-role launch of the hardened one-shot child
   (`note_projector_process.rs` grows a generate lane): same
   interpreter admission, same `pre_exec` seatbelt, generate manifest
   (`note-runtime-generate.json`), registered 3600 s deadline. The
   bridge runs the model child and returns
   `note-generation-result/1` — KEEP verdict points, locators only,
   bounded by the existing 64 KiB frame cap.
2. The worker's `note.create`, admitted at last, with its generator
   argument satisfied by a *deterministic assembler* built from those
   points: kept locators → `render_structured_note` → citation checks →
   `note_artifact` (all already shipped in the runtime's `summarize`)
   → the validated immutable pair. No model in the worker — the
   adapter's "no model default" stance is preserved literally; the
   model never runs outside the sandboxed one-shot child.

Consequences accepted with B: the `note.create` argument contract widens
to carry the generation points; the coordinator sequences two child
launches per regeneration; failure surfaces stay honest and distinct
(model-side failures arrive as the bridge's `transcript-only` outcome,
assembler failures are deterministic artifact bugs that must be loud).

Sub-decision within B, recorded here: the generate-role child launches
through an **extended descriptor bootstrap** (a fourth descriptor for
the generator bytes), not through the path-based `main()` entry. The
descriptor protocol exists so the child executes pinned bytes, never
pathnames; a path-based generate launch would quietly weaken that for
the one role that runs the model. Both structural refusals — the Rust
`BOOTSTRAP` role check and `verify_descriptor_runtime`'s
project-only gate — widen together, with the generator descriptor
required exactly when the role is `generate` (the same biconditional
the manifest already enforces).

Build order: (1) Rust generate transport + descriptor widening, (2)
worker `note.create` admission + assembler (operation sets move in
lockstep with `supervision::internal_alpha_operations()`), (3)
coordinator sequencing behind `accept_regeneration`, (4) the UI
regenerate control.

**Slice 1 landed 2026-08-15 (commit `ad2eeec`).** The descriptor set and
the role are one decision on both sides: three descriptors speak
`project` exactly as before, a fourth — the manifest-pinned generator
bytes — speaks `generate`. Rust's hardened child-drive is one
role-parameterized sequence shared by `ProcessNoteProjector` and the new
`ProcessNoteGenerator`; `admit_note_generator` applies the projector's
admission rules and binds the verified installed model directory.

**Slice 2 spec (recorded before building, 2026-08-15).** The generate
lane returns `note-generation/1` points: located anchor excerpts with no
claim type and no prose — deliberately, because the candidate-first
program classifies KEEP/ABSTAIN over transcript-anchored candidates and
writes no claims. The note/2 claim vocabulary today is four types
(decision, action, proposal, question) inherited from the cue-era
extraction pipeline. Assigning any of those four to a typeless kept
point would be mis-attestation by construction. The product brief
resolves the shape: generated content is "points — a draft from the
transcript", each with a clear path back to the source text. So slice 2
adds one honest vocabulary entry — a `point` claim type rendering under
a single "Points" section — rather than faking types or widening the
note into a summary. Claim text is the anchor excerpt verbatim (claim
and quote coincide; the pipeline writes no prose by design). The entry
widens together everywhere the vocabulary is pinned: `summarize`'s label
set, render titles and claim parser; `note_projection.rs::ClaimType`;
the shared cross-language fixture (one additive valid row); and the UI
claim presentation. `note.create`'s argument contract gains the
generation result (points), and the injected generator becomes a
deterministic assembler: points → excerpt items → the existing
memory-only note/2 build chain (`attach_evidence_items` →
`normalize_extraction_items` → `render_structured_note` →
`structured_citations` → `structured_artifact_citations` replay), the
same chain `mlx_note_admission.py` already drives for research
candidates. The worker's operation sets and
`supervision::internal_alpha_operations()` move in lockstep, exactly as
the `corpus.embed` drift note in `supervision.rs` warns.

**Slice 2a landed 2026-08-15: the vocabulary, with one compatibility
finding.** The `point` type is admitted end-to-end (summarize's label
set, `_TYPES` section mapping, render titles; the validator's claim-row
gate; `ClaimType::Point`; the library reader's serialization — the UI
humanizes types generically). The finding: the model-extraction request
schema embeds the label enum, and every retained note/2 replays that
schema digest-for-digest through `structured_citations` — widening the
enum in place refused eight fixture notes, which is exactly what it
would do to a shipped user's existing notes after an update. So the
extraction contract is now its own frozen four-label constant
(`_EXTRACTION_LABEL_VALUES`); POINT lives only in the validation
vocabulary and rendering, and a points-note must carry candidate-first
provenance rather than a synthesized extraction stage. That constrains
the slice-2 assembler design: it cannot reuse the extraction stage
receipts; the `checks` a product note stores come from `report()`,
whose gates assume a model-written note (`calls`, prompt-echo,
context), so the assembler needs either a candidate-first-native checks
path or a deliberate satisfaction of those gates — the next design
question, not yet decided.

**Slice 2b design (decided 2026-08-16): a second evidence contract,
not a synthesized model run.** `structured_artifact_citations`
hard-replays extraction stage receipts — per-slice prompts, request
schemas, model identity — none of which a deterministic points assembly
possesses, and synthesizing them would be the same mis-attestation the
fake claim types were rejected for. The note/2 artifact already carries
a discriminator (`claim_evidence_contract`, single-valued until now),
so points-notes declare `candidate-evidence/1` and the citation entry
point dispatches on it. Its replay is stronger than storage: the
candidate manifest is a pure function of the transcript and the
registered product contract, so the validator regenerates it
(`generate_manifest`, broad strategy, ±2 window), requires the stored
`manifest_sha256` to match, resolves each point's `candidate_id` back
to its anchor fragment, and re-derives every locator span and digest
from the transcript itself. Claim text must equal the anchor excerpt
verbatim. The stored `checks` object keeps the one shared verdict
formula (`verdict()` stays the only owner): `citations` is computed for
real by the candidate replay; gates that measure model pathologies
record their own truthful state — `context.ok: null` (not scored — the
established meaning for a stage that does not exist), `attribution`
and `extraction` `applies: false`, `numbers` computed genuinely against
the transcript text, `prompt_echo` vacuously true with its reason
recorded. The assembler and the candidate replay both live in
`summarize` — one owner, shipped to the worker and the validator bundle
alike. Memory contention is already handled: whisper's
runtime is released inside the worker before the `transcript.create`
terminal frame, so any regeneration accepted at `TranscriptReady` or
later starts past the release.

**Slice 2 landed (2026-08-16, commit 4a3404a).** The worker now admits
`note.create` with an optional exact-shape `generation` payload
(`note-generation/1`: transcript pin, `manifest_sha256`, candidate
count, points, run receipt) and assembles a published note/2 through
the deterministic candidate-point assembler in `summarize`
(`candidate_note_document`). Points-notes declare
`claim_evidence_contract: candidate-evidence/1` and their citations
replay by regenerating the manifest from the transcript, exactly as
designed in 2b — the end-to-end dispatch test drives manifest points
through the worker to a published note whose artifact replays
digest-for-digest. Bare `note.create` (no payload) still refuses, and
`ALPHA_OPERATIONS`/`internal_alpha_operations` moved in lockstep. All
four lanes green (session-core 412, worker 192+40, desktop 130, UI 10).
Remaining: slice 3 (coordinator sequencing behind `accept_regeneration`
— the desktop `NoteGenerationWorker` impl that runs the generate child,
parses `note-generation-result/1`, and issues this `note.create`), then
slice 4 (UI regenerate control).

**Model-size research (2026-08-16, operator question: newer open-weight
models with better compression).** Web research plus direct HF API
verification. The current note model (gemma-3-12b-it-qat-4bit, 8.06 GB)
now has credible smaller challengers; the two worth evaluating, both
license-verified Apache-2.0 via the HF API (redistribution on our R2
mirror is clean, no Gemma-terms passthrough):

- **Qwen3.5-4B** (mlx-community/Qwen3.5-4B-MLX-4bit): 3,061,132,920
  bytes (verified). Vendor benchmarks place it above the Gemma-3-12B
  class; hybrid linear-attention architecture should also cut per-call
  latency. Risk: newer architecture — confirm the pinned mlx-lm
  supports it before committing. Conservative fallback:
  Qwen3-4B-Instruct-2507-4bit (2.28 GB, standard transformer).
- **Granite-4.0-h-micro** (3B, 1.81 GB): best verified
  instruction-following for its size (IFEval 82.3 from IBM's card);
  hybrid Mamba2 built for exactly our latency profile. Knowledge below
  12B class — but the product task is constrained KEEP/ABSTAIN plus
  extractive points, where instruction adherence dominates.

Also noted: Gemma 4 reportedly moved to Apache-2.0 (single-source,
re-verify before mirroring), but its E4B is 5.25 GB at MLX 4-bit —
over target; LFM2.5-2.6B (1.54 GB) is capped by its license at <$10M
commercial revenue — rejected for a shipped product; MLX now supports
MXFP4/NVFP4 and DWQ conversions, a second compression lever beyond
uniform 4-bit.

Any adoption runs the full preregistered eval (11/13 recall bar on all
four corpora) as a new arm — sequenced after the offer-stride arm
lands, which halves the cost of exactly that evaluation.

**Family follow-up (operator question: GLM, Grok, DeepSeek).** All
three ruled out, verified against the HF API through Aug 2026. GLM:
licensing is fine (MIT across the family) but size is not — the
smallest MLX conversions are GLM-4-9B-0414 at 5.31 GB and
GLM-4.6V-Flash at 7.09 GB, the 2026 releases are all flagship-scale
MoE, and GLM-Edge (1.5B/4B, 2024) has no MLX conversion at all. Grok:
xAI has only ever open-weighted 100B+ models (Grok-1, Grok-2.5, the
latter under a restrictive community license); nothing small exists.
DeepSeek: every 2026 release is giant MoE (V4-Flash is 304B —
"Flash" is not small); the R1-Distill small models are reasoning-tuned
and emit long chain-of-thought before answering — the wrong shape for
a single-token constrained verdict, and slower, not faster, per call.
Shortlist unchanged: Qwen3.5-4B, then Granite-4.0-h-micro.

**Model-selection closure (2026-08-16).** All three shortlist
candidates were evaluated against the four locked corpora under the
preregistered gates (notes/EVAL.md): Qwen3-4B-2507 passed 1 of 4,
Granite-4.0-h-micro 0 of 4, Qwen3.5-4B 0 of 4 (run under a separate
research venv with mlx-lm 0.31.3; the product runtime pin is
untouched). Small models under-keep real meeting speech by 5–25x and
miss most locked events; a probe confirmed the abstention is model
judgment, not the harness. **The 12B stays; the question is closed**
until a materially new small-model release, and the arm harness in the
packet makes a future re-test a one-command affair per candidate.

**Slice 3 landed (2026-08-16, commit a5462e4).** `accept_regeneration`
now runs the real chain: the desktop admits the generate child per
call from live storage (verified catalog, generate manifest), runs it
sandboxed, parses the `note-generation-result/1` frame under a strict
envelope (session-core `parse_note_generation_result` — request-id and
transcript pins bound, outcome/payload coherence enforced), and hands
the generation object verbatim to the worker's `note.create`, whose
frozen contract and deterministic assembler own every deeper judgment.
A `transcript-only` child outcome becomes a durable Rejected receipt
carrying only recoverability; the published pair is re-inspected
through the worker's `note.inspect` before the meeting record
advances. Without an installed note model, admission refuses before
any process is launched. Remaining: slice 4 — the UI regenerate
control and the facade command registration, where the long-running
generate call moves off the command thread.

**Slice 4 landed (2026-08-16, commit 2d2db87) — the chain is complete.**
The meeting view renders a Generate-note control whenever the backend's
note response carries `regeneration_source_sha256` — the eligibility
signal and the source pin in one field, present exactly when the
facade would admit the operation. The button goes busy per meeting
while the minutes-long command runs (Tauri executes it off the main
thread; snapshot polling and the rest of the app stay live), and
completion re-opens the meeting to show either the published note or
the summary-failed answer. The command now finishes the facade's
single-operation slot at the terminal receipt and carries the same
setup-recording guard as restoration; `regenerate_note` joined the
shell contract's pinned product-command set. With slices 1–4 landed,
the full path exists: button → facade → coordinator → sandboxed
generate child (admitted model, strided offer, constrained verdicts,
pruner) → worker `note.create` (deterministic assembler,
candidate-evidence/1 replay) → fresh `note.inspect` → published
meeting note. What remains is operational, not structural: a real
end-to-end run on this machine against an installed model, and the
operator's read of a generated note.

**Operational close-out (2026-08-16) — three defects found running the
chain for real, one still blocking.** The note model was hand-installed
against the sealed catalog digests (`gemma-3-12b-it-qat-4bit`, six
files under `models/note.d/<id>/<revision>/`) and the packaged app was
driven end-to-end for the first time on this machine.

*Release-blocking catalog bug, fixed (commit 98102da).* The packaged
worker's exact-shape check on the runtime catalog predated `note_models`
joining that schema, so the shipped 0.5.7 worker refused the whole
catalog and exited silently at startup once a note model was present.
Widened the check plus a regression fixture; full worker suite green.

*Running a local build requires the release lane's signing stage, not
just `npm run build`.* The preview-signed dev bundle has an
ad-hoc/linker-signed Python and an outer app with no hardened-runtime
flag, so `SecurityCodeVerifier` correctly refuses child admission
before any model runs. A runnable local build needs the same signing
`scripts/sign-notarize.sh` performs minus the Apple submission: sign
every nested Mach-O (`--options runtime --timestamp`, Python gets
`--identifier com.ninochavez.local-meeting-notes.python-runtime` and
the Python entitlements), refresh `app-runtime.json` and the
`note-runtime-*.json` manifests from the signed bytes
(`worker/build_manifest.py`, which must run *before* the outer sign —
running it after breaks the outer seal), then sign and verify the
outer bundle. Never replace the binary on disk under a running
process — the kernel SIGKILLs it on the next `CODESIGNING`/"Invalid
Page" fault; always stop the app first.

*Admission defect, fixed (commit 13d1aff).* With a properly signed
bundle, admission reached child spawn and then failed at the dynamic
bind: `SecCodeCheckValidity` with `kSecCSMatchGuestRequirementInKernel`
returned `errSecCSReqInvalid` for the live child. Reproduced standalone
outside the app — same result against a fresh admission child,
`/bin/sleep`, and the process's own self-code, on macOS 26.5.2,
independent of which requirement was passed (a bare cdhash requirement
failed identically to the full designated requirement). The flag has
never worked in this app; the affected path (`drive_bridge_child` →
`bind_live_code`) is shared by both the `project` and `generate` roles,
so 46a3eef's admitted note projector never actually admitted a live
child on this OS either — the library-rebuild path degrades to no
projection, not a crash, so this was silent. Fix: dropped the flag,
keeping the plain designated-requirement match. The residual guarantee
against a between-checks path swap is unchanged — `bind_live` already
re-verifies the live executable's pinned fd identity and digest
(`verify_live_executable_file`) after the dynamic check returns.

*Packaging gap, unresolved — this is what blocks the run.* With
admission fixed, generation itself now runs the full sandboxed child
protocol (spawn, ready, result) cleanly, but the child immediately
refuses with `response-contract`. Traced to source: `worker/note_generator_mlx.py`
imports `mlx_lm` to load and decode against the model, but `mlx_lm` is
not installed anywhere in the bundled Python runtime
(`apps/desktop/vendor/python-runtime` and the staged
`apps/desktop/runtime/python-runtime` both carry `mlx` and
`mlx_whisper` for transcription only). `_Session.resolve()` raises
`ModuleNotFoundError` → the child writes `{"error": "model could not be
loaded"}` and exits → the bridge reads a well-formed JSON line whose
shape isn't the classifier contract → `response-contract`,
non-recoverable. This is a first-run discovery, not a regression: no
build has ever run this path to completion, so the dependency gap was
never exercised. `mlx-lm==0.31.3` is what the research venv used for
model selection (`notes/EVAL.md`), but naively `pip install`-ing it
into the product runtime pulls a newer `mlx`/`mlx-metal` (0.29.3 →
0.32.0) as a transitive dependency, upgrading the same package
`mlx_whisper`'s already-shipped transcription path depends on — an
untested, unrequested change to a working feature. There is no
lockfile for either vendored Python runtime tree (both are
git-ignored, machine-local); `spike/requirements.txt` pins
`mlx-whisper`, `numpy`, `speechbrain`, `torch` and has never named
`mlx-lm`. This is a packaging decision, not a one-line fix — see the
options below. The end-to-end run and the operator's read of a
generated note stay blocked until it lands.

*Known gap, not addressed here.* `recover_incomplete` exists in
session-core but the desktop never calls it, so a failed or
interrupted generation leaves a nonterminal operation record that
permanently refuses every later `regenerate_note` attempt for that
meeting (`Ambiguous("another nonterminal operation already exists")`).
Manual workaround used during this session: delete the stale directory
under `.../operations/<uuid>/` (safe when it holds only
`request.json`). Wiring the recovery call into the desktop coordinator
is its own slice.

**mlx-lm packaging options, priced.** Whichever is chosen, land it as
its own slice with the arm harness re-run to confirm `mlx_whisper`
still passes.

1. *Add `mlx-lm` + transitive deps to the shared product runtime,
   upgrading `mlx`/`mlx-metal` in the process.* Cheapest to implement —
   one `pip install --target`, one re-sign. Cost: ~15 new packages
   (`transformers`, `safetensors`, `sentencepiece`, `protobuf`, `typer`,
   `rich`, …) with new compiled `.so` files into a bundle whose
   admission model rests on signed, digest-pinned bytes — bundle-size
   and notarization-surface growth — and an unverified `mlx_whisper`
   run against a newer `mlx` than it shipped on.
2. *Pin an older `mlx-lm` release compatible with the already-installed
   `mlx==0.29.3`,* if one exists, avoiding the shared-dependency bump
   entirely. Needs a compatibility check against the release history
   before committing; not yet done.
3. *Isolate the generate child's site-packages* from the transcription
   runtime's, so `mlx-lm`'s newer `mlx` only ever loads inside the
   sandboxed generate role and `mlx_whisper` keeps running against its
   already-verified `mlx==0.29.3`. Avoids the cross-feature risk
   entirely; costs a second vendored Python tree (or a second
   site-packages root the generate child's `sys.path` is pointed at)
   and a corresponding change to the admission/signing recipe, which
   currently assumes one Python runtime per bundle.

**Operational close-out, continued (2026-08-16) — the chain ran end to
end.** Option 3 above was chosen: `worker/note_bridge.py`'s generator
bootstrap now inserts a private `generate-site-packages/` ahead of the
shared one on `sys.path`, derived from `sys.executable`, before the
child imports anything. `mlx_whisper` keeps resolving the shared,
already-verified `mlx==0.29.3`; only the generate role's own bootstrap
sees the isolated tree carrying `mlx-lm` and its newer `mlx` pin. With
that in place, the app was driven through capture → transcript →
candidate generation → note publication → re-inspection on this
machine, against a real meeting, for the first time. Four more defects
surfaced running it, three fixed here and the fourth already fixed
above; the fifth (below) is why the run didn't complete on the first
try even with all four landed.

*Unbounded memory growth, fixed.* The registered classifier batch size
is 1, so `decide()` runs once per offered candidate — up to a few
hundred per real meeting — and mlx's Metal allocator caches freed
scratch buffers for reuse rather than returning them to the OS.
Nothing in the loop reused a cache across calls, so it grew unbounded
across the run instead of staying near one batch's peak: measured at
27GB on this machine, which forced a hard restart mid-session (Force
Quit reported the app at 27.39GB, non-responding). `mx.clear_cache()`
after each candidate's `decide()`, mirrored identically between
`worker/note_generator_mlx.py` and `notes/product_run.py`'s
`MLXVerdictTransport` per that file's own sync obligation, brought RSS
back to ~1GB between batches — verified by live memory monitoring
during a real run.

*Staged bundle missing a dependency, fixed.* Generation failed with
`ModuleNotFoundError: No module named 'candidate_first'` inside the
injected note generator, even though `notes/candidate_first.py`
already existed in the repo. `apps/desktop/runtime/notes/` — the
staging directory `tauri.conf.json` bundles into the app — was a stale
subset (3 of 82 files) that predated `note.create`'s dependency on it.
This directory is gitignored and rebuilt at package time, so the fix
is staging discipline, not a source change: copy `notes/candidate_first.py`
into the staging tree (and the currently-built app's `Resources/`
copy) alongside the other `notes/` modules it already carried.

*Admission gap, fixed (commit 9baa602).* `note.inspect` stayed
boundary-lane-only under the belief that it ran through the sandboxed
bridge's own `inspect` role rather than the standing worker port — but
`WorkerProcessNoteInspectBridge` always calls the standing worker
regardless of admission level, so under `internal-alpha` the worker
refused the operation outright. A published note could reach
`note.create` and then get stuck one step short of the meeting record:
`apply_result` never ran, `meeting.json`'s `lifecycle` never advanced
past its pre-note state. Promoted `note.inspect` into
`ALPHA_OPERATIONS`, the same move made for `corpus.embed` once its
model was packaged — see `crates/session-core/src/supervision.rs`'s
`the_alpha_operation_set_is_read_from_the_worker_itself`, which parses
`worker/main.py`'s literal set from source so the two sides cannot
silently disagree (a mismatch here is what caused the 2026-08-08
`OperationMismatch` incident referenced in that test).

*Read-time claim-length cap, fixed (commit 6d1c84d).* With generation,
creation, and inspection all landing, the library rebuild's
`note.project` step then refused the very first real note with
`artifact-invalid`, non-recoverable — on a retry, not a fluke.
Root-caused with a standalone script replaying `note_validator.py`'s
`project()` re-derivation against the actual published note file:
`validate_claim_rows` was rejecting a 165-character claim over its
160-character cap. That cap traces to the older LLM-extraction
contract's `MAX_STRUCTURED_CLAIM_CHARS`, enforced at write time only
for that contract — candidate-first claims are verbatim transcript
excerpts (`summarize.py`'s `validate_candidate_evidence`), never capped
at write time, and `candidate_first.py`'s fragment/anchor spans carry
no length bound either. Not a rare edge case: any note whose kept
candidates include one longer spoken sentence would write successfully
and then permanently fail projection, every time. Operator decision
(asked, since this was a real contract question, not a mechanical
bug): remove the cap entirely rather than special-case it to
`"point"`-type claims or push it upstream into candidate generation.
Removed from both `note_validator.py` and `note_projection.rs`'s
`parse_claim` — the Rust side independently re-enforced the same 160
characters and would not have followed a Python-only fix. Consequence
worth flagging: the projection frame no longer has a knowable
worst-case size, since claim length is now unbounded on both sides. A
meeting whose kept candidates run long enough could in principle
overflow `MAX_PROJECTION_FRAME_BYTES` (64KB) through ordinary claim
length rather than corruption; it fails closed as `Unavailable`, the
same fallback already documented for that case, so the failure mode is
unchanged even though it is now reachable by a different, more
ordinary path.

*Outcome.* With all five defects fixed, the packaged app ran the full
chain against a real meeting on this machine: `note.generate` (165
candidates) → `note.create` (published) → `note.inspect` (re-verified)
→ `meeting.json` committed (`lifecycle: "ready"`, `current_note` set)
→ library rebuild's `note.project` step succeeded. Confirmed three
ways: the standalone re-derivation script passing against the real
published note and transcript files under the fixed validator; the
full Rust (`cargo test --workspace`, 439 tests) and Python
(`pytest worker/tests/test_note_bridge.py`, 102 tests) suites green;
and the running app's own UI showing no `Generate note` prompt and no
refusal toast for that meeting after a clean relaunch. The operator's
own read of the generated note is the next step, and stays the
operator's alone — no note or transcript content appears in this repo
or in any tooling output from this close-out.

## Full-transcript source-check prototype (2026-09-29)

This research-only draft is in `spike/full_transcript_note.py`. It has no model
loader, worker wiring, storage write, or product UI path.

It freezes the canonical transcript's visible turns into stable source-order
IDs. The canonical `gated_turns` representation stays withheld: it is counted
for provenance but is not copied into a prompt, a result, or an error. Channel
labels remain literal capture labels, and the `none` attribution removes them.
The snapshot identity covers the visible source, attribution limit, and
withheld-turn count. A changed or malformed identity refuses before a provider
call.

There are at most two calls. The first receives the complete source and returns
structured topics and material claims. Each claim has a closed kind, explicit
owner and date fields (or `null`), conditions, and visible source-turn IDs. The
second receives that same complete source plus the untrusted raw proposal. It
must assess every offered claim exactly once as `supported`, `unsupported`, or
`uncertain`, with exact offered-source quotes and a rationale. It may also name
source-quoted material omissions. Malformed JSON, missing, duplicate, or extra
checker rows, unknown source IDs, and quote mismatches refuse the run.

Unsupported claims do not enter the preview. Uncertain claims remain marked for
review. A supported claim remains marked for independent review. A source quote
only proves a location, so neither quote matching nor the model's own check can
make a result verified, accepted, complete, or production-ready. Checker
omissions prevent any completeness claim. Raw proposal and check text, plus the
immutable source identity, remain on the in-memory result for a future
provenance-owning caller.

Run the synthetic contract test from this repository root:

```bash
python3 -m unittest spike/test_full_transcript_note.py
```

The 15 synthetic cases measure orchestration and refusal behavior: full visible
source rendering, withheld exclusion, pre-draft and pre-check context overflow,
strict checker rows, quote and source bounds, source identity, invented owners,
uncertainty, and a later omitted decision. They include a decoy with a valid
quote but an unsupported claim. They do not show that any model finds semantic
errors in a real meeting or produces a useful note.

Production remains unproven. Integration needs a supervised offline child that
can enforce abort and deadline behavior; this injected provider only receives a
deadline and cannot be forcibly timed out here. It also needs the selected
model's chat template and tokenizer for exact full-prompt counting, a
capability-checked source projection, durable provenance storage outside Git,
and measured memory and disk admission. The next real-model evaluation needs a
separately registered protocol and allowance. This prototype does not create
one, run one, alter the installed app, or change the closed comparison budget.

### Supervised native adapter (2026-09-29, orchestration only)

`spike/full_transcript_note_mlx.py` is the research-only native adapter for
the prototype above. It accepts `--model`, `--transcript`, `--manifest`,
`--out`, and `--timeout`. Its output directory must be new and outside a source
checkout and product storage. A parent can run the following command only from
an already prepared, pinned local runtime; it does not download or install
anything:

```sh
python3 spike/full_transcript_note_mlx.py --model <pinned-model-dir> --transcript <private-transcript.json> --manifest <pins.json> --out <fresh-private-output-dir> --timeout 300
```

The parent verifies every manifest-pinned model file, Python executable, and
runtime package before launch and repeats the identity check after the child
ends. It refuses launch below a 4 GiB free-disk reserve, retains sampled RSS
separately from MLX peak allocation, and terminates the complete child process
group on a deadline or reserve loss. The child applies macOS Seatbelt `no-network` before
project or MLX imports, verifies the same pins again, loads the model once, and
uses the tokenizer's actual system/user chat template with thinking disabled.
The rendered prompt, not bare system-plus-user text, is what context admission
counts, with a full 4,096-token reserve for each of the draft and source-check
calls. No source trimming, repair call, or third call is permitted.

The child retains the raw text for every stage reached, each stage's timing and
terminal event, and a failure receipt even if JSON decoding or checking fails.
A `length` terminal event is refusal before any decoded preview. A successful
private output contains `candidate.md`, the original structured claims and
check rows, excluded and uncertain claim state, material omissions, source
identity, model/runtime hashes, per-stage load/prefill/generation timing, MLX
peak allocation, and the outer parent receipt. These are model-assessed
research drafts requiring independent review; a valid quote or `supported`
label is not semantic verification.

The adapter deliberately has no product command, storage write, model/runtime
installer, manifest edit, downloader, benchmark loop, or quality score. The
remaining production seams are an operator-approved source-quality protocol,
durable private provenance retention, and a separately reviewed app-side
admission and presentation path. The synthetic tests prove only orchestration;
no real-model quality or timing result has been measured by this change.


### Real-model source-check evaluation closed (2026-09-29)

The operator separately authorized planning and execution through a measured
adoption decision. This bounded evaluation did not reopen the completed
four-model comparison or authorize installation or release.

The evaluation used the migration meeting, the architecture meeting, and a fresh
captured presentation. Source-based criteria were frozen before inference. The
fresh source had repeated speech-recognition fragments and an unfinished last
turn. The evaluator was instructed not to turn the presentation into
organizational adoption, reconstruct its missing ending, or invent assigned work.

The allowance was six native attempts: three initial attempts and, only if a
general correction could change the outcome, one revision with three attempts.
Each attempt allowed at most two calls, 4096 output tokens per call, a 300-second
child deadline, and the unchanged 4 GiB disk reserve. The end-to-end target was
120 seconds. Failed executions consumed attempts; they did not establish note
quality. The negative result closed the experiment without widening its
allowance or lowering its criteria.

The parent fixed the declared-but-unapplied seed and restored conditions,
owners, and dates in the private preview before the first native attempt.
Thirty-four CPU-only checks now pass, including resource and deadline negative
controls. The later test run simulated available disk for synthetic children;
its dedicated reserve-loss controls still explicitly exercise refusal. A real
network-denied child also exited under the corrected termination path. These
checks establish runner behavior, not real-model quality.

The first native attempt stopped when free disk fell below the registered
reserve. It produced no complete draft and no source-check output. A
process-group termination race then raised PermissionError after SIGTERM.
Process enumeration confirmed the child had ended. The corrected supervisor
confirms exit and preserves the resource-failure receipt if that race recurs.
The generation instructions and quality criteria did not change.

The evaluation closed after one resource refusal and five inference attempts.
None produced an accepted draft and source check. Its private outputs and receipts
were removed during the later disk cleanup; the measured results and decision
remain summarized at the top of this document. The candidate is not integrated,
and the installed app is unchanged. Reopening requires a new protocol and a new
bounded allowance.
