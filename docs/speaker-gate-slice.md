# The speaker gate, wired into transcription

**Status: built, 2026-08-05.** This file was a proposal until that date. It is
now the record of what was built, and of three things the proposal got wrong.

---

## What changed

A transcript now runs the operator's voice profile over the microphone leg,
marks the turns the profile says are not the operator, and records in the
artifact what the gate did. Before this, the app promised operator voice
isolation and performed none: `create_transcript_revision` ran two filters —
`drop_unvoiced` for voicing, `drop_bled` for bleed — and no gate. Every
transcript contained exactly the words it would have contained with no profile
at all, and its `voiceprint` field said `null`, which means *no profile was
supplied*.

That last part is why this was a defect rather than an absence. A profile can
be installed on the shipped build — Rust's `enroll_profile_candidate` publishes
it — so `voiceprint: null` was an active misstatement on any transcript made
after an operator finished enrolment.

## The three corrections

**The machinery already existed.** The proposal said the slice would "map
segments to `spike/speaker_gate.py`'s inputs." It does not. `spike/dual_capture.py`
already holds the complete filter (`drop_offprint`), the provenance builder
(`voiceprint_provenance`), and the `Voiceprint` type — written, measured on the
75-minute capture, and used by the research CLI. The packaged path was the only
consumer that never called them. The slice is wiring, not implementation.

**Fork 2 was already answered in code, and more thoroughly than the proposal
knew.** `drop_offprint` marks rather than removes, and says why at length: the
gate's own failure mode is deleting a colleague from a record of a meeting that
cannot be re-run, and only the operator can say whether a voice near the
microphone was a participant. It also *keeps* the unscorable segments — 28% of
the long capture's mic segments, carrying the "yes", "agreed", "I'll do that"
that the tool exists to record.

**Option B was cheaper than the proposal claimed.** The proposal said an
explicitly-ungated transcript would need "a new transcript-level field, which is
a storage contract change." False. `write_transcript` has taken a `gating`
argument all along and the schema has carried a `voiceprint` field all along,
documented as "computed by the caller, which is the only place that knows
whether the gate actually executed." Option A was still chosen, but on its
merits, not on a cost that did not exist.

## Fork 1, settled: refuse

*A profile is installed and the runtime cannot apply it — what then?*

**Refuse the transcript.** `_installed_voiceprint_gate` raises when a profile is
installed and no admitted encoder can score it, and `transcript.create` fails
with it.

The reason is the misstatement above. A transcript written without the gate
records `voiceprint: null`, and null already means "no profile was supplied."
There is no way for the artifact to say "a profile was supplied and this build
could not run it," so writing one hands the operator words they believe were
checked, in a file that says nothing was ever asked.

What it costs, stated exactly: on a **placeholder-encoder** build, installing a
profile stops transcription until the operator resets the profile. That is a
real cost and it lands on nobody in the cohort — the distributed DMG is built
with `worker/build_runtime.sh build-alpha-encoder` and carries the admitted ONNX
encoder (runbook, "Encoder-candidate lane"; true since 0.2.2). It lands on
developer builds of the transcript-only lane, which is the right place for it.

> **Stale as of 2026-08-30.** The released 0.5.9 DMG was pulled and checked
> against its receipt: it is a placeholder-encoder build
> (`encoder-unavailable.identity`, no packaged encoder). The paragraph above
> was true when written and is not true of the current release; the cost
> analysis is bounded only because enrollment is not registered in the shipped
> app. Details and evidence: distribution runbook, "Encoder-candidate lane",
> correction of the same date.

### "Installed" is a size, and the first version got that wrong

`<root>/profile/voiceprint.json` **always exists**. Rust owns its lifecycle and
never unlinks it: `initialize_or_open` creates it at zero bytes on every macOS
startup before any capture, and a reset swaps the live profile for a zero-length
file rather than removing it. `ProfileLifecycleBaseline::profile_present` is
`profile_size != 0`.

The first version of `_installed_voiceprint_gate` tested existence. That made
every fresh install look enrolled, so `transcript.create` refused for every
operator who had never recorded a sitting — on the placeholder lane at the
encoder check, on the cohort lane when `load_profile` hit an empty file. The
whole product, unavailable, on both lanes.

Two things let it through, and both are worth naming. The tests used `{}\n` and
an absent directory — plausible states, neither of them the one the app actually
produces. And the proposal's provenance list named `retention.rs` and
`profile_adopt` but not `profile_lifecycle.rs`, which is where "installed" is
defined. Reading the code that *writes* a file is not the same as reading the
code that decides whether it counts.

It was caught in commit review before any build. The regression test asserts the
zero-byte state directly and was confirmed to fail against the broken version.

## Where it runs

One filter, in the seam the module already had. `create_transcript_revision`
takes `gate_filter` alongside the injectable `voicing_filter` and `bleed_filter`,
and applies it to the **microphone leg only** — the system leg is by definition
not the operator, and a voiceprint has nothing to say about it. Unlike the other
two it returns a report as well as segments, and that report becomes the
transcript's `voiceprint` field.

`_installed_voiceprint_gate` in `worker/adapters.py` decides which of three
states the runtime is in:

| State | Behaviour | `voiceprint` field |
|---|---|---|
| No profile installed — the file absent **or zero bytes** | No gate. Unchanged. | `null` — and that is what null means |
| Profile installed, encoder admitted | Gate runs | The full report, including what it rejected |
| Profile installed, no admitted encoder | **Refuse** | No transcript is written |

Two further refusals are honest rather than defensive: the packaged encoder must
match its manifest digest, and `load_profile` must accept the installed profile
against that exact digest. The spike's separate dimension probe is not repeated,
because here the fingerprint argument *is* the verified packaged artifact's
digest — the two checks are the same check.

## What the gate itself decides, and does not

Three behaviours come from `drop_offprint` unchanged, and each is a measured
decision rather than a default:

- **It skips itself above the bleed cut.** Where the far end is coming back
  through the room the transcript has already dropped every speaker label, and
  that is the same audio where the gate is measured to reject the operator — 1
  of 7 voiced windows admitted. It records `applied: false` with the reason.
- **It keeps what it cannot score.** Short turns are commitments.
- **It reports the co-located speaker.** When the dropped speech keeps coming
  back as one voice, someone beside the operator is being removed from the
  record, and that goes into the artifact rather than a terminal.

## What this lit up downstream

Nothing else needed building, which is the sign the seam was right. Everything
below was already written against `gated` and had never had an input:

- `notes/transcript.py` splits gated turns away from what the notes model may
  see, and renders the co-located-speaker alert into the note's caveats.
- `library_read.rs` projects gated turns as withheld, excludes them from search,
  and addresses restoration by their index in the base document.
- `transcript.restore` — the whole of J4 — overrules a gate decision. Until now
  it was a correct implementation of an operation that could never have an input.
- The transcript screen renders a withheld turn, and the copy formatter writes
  it as `[mm:ss] (withheld — a voice check set this turn aside)`.

`worker/tests/test_transcription.py` now writes a real gated transcript and
asserts the on-disk shape all four of those read.

## What is actually exercised, and what is not

`RealProfileGateTests` runs the gate for real: a profile built by `enroll`,
written by the canonical `save_profile`, read back by `load_profile` with the
fingerprint binding live, then scored over fixture audio in which one turn is
the operator and one is somebody at the next desk. The tests assert that the
other voice is marked and scores below the threshold, that the operator's turn
was **scored and admitted** rather than merely left alone — `kept == 1`,
`unscorable_kept == 0`, because an unjudged segment is also kept and also
unmarked — and, as a control, that two operator turns produce no rejection at
all. The bleed skip, both refusal branches, and the whole chain to a loaded
document are covered too.

**The numbers are fixture values, not measurements.** The threshold comes from
the synthetic score arrays handed to `save_profile`, and the scores come from a
fixture encoder that recovers a speaker from a clip's amplitude. They prove the
wiring discriminates; they say nothing about how this gate performs on a voice.
The only measured figures in this area are in `spike/encoder-packaging/RESULTS.md`
and RESULTS' own leave-one-sitting-out evidence.

Two invariants are pinned by mutation rather than by reading. Changing the mic
leg to the system leg at the call site fails two tests — before that, it failed
none, and in production it would have scored every operator turn against the far
end, rejected all of them, and written them into `gated_turns` with no error
anywhere. A stub that names the audio argument and discards it cannot see that;
the stubs here assert identity on it first.

Only the ONNX session is substituted, through an `embedder` argument that
replaces nothing else — the encoder file, its digest against the manifest, and
the profile's fingerprint against that digest all stay live. Before it existed
the closure had no coverage whatever, and its first execution would have been on
a cohort machine after a real meeting.

Still unexercised, and worth stating rather than discovering: **the real
onnxruntime path**. `_onnx_sitting_embedder` and `fbank_features` are proven by
`sitting.derive`'s own admission evidence, not by anything here, and the two
consumers pass segments of different provenance. The first meeting transcribed
on an encoder-carrying build with a profile installed is the first time this
exact composition runs.

## What would change this

- **If the measured operating points do not transfer** from leave-one-sitting-out
  enrolment evidence to live meeting audio, the threshold enrolment produced is
  not the threshold that should gate a meeting. The gate does not invent one —
  `drop_offprint` takes it from the profile file and there is no constant to fall
  back to — but the profile's threshold carries an enrolment provenance, not a
  live-audio one. The artifact records `threshold`, `target_frr`, `measured_frr`
  and `n_sittings` so a later reader can see what it was calibrated on.
- **If gating should apply to the system leg too** — to mark a second person on
  the operator's own microphone — the mic-leg-only rule is wrong. Nothing in the
  current inventory asks for that.
- **If a placeholder build must keep transcribing with a profile present**,
  Fork 1 flips to B: pass a `gating` dict recording that a profile was installed
  and not applied. Cheap, now that the field is known to exist. It was not chosen
  because a build that cannot check should not produce artifacts implying it did.

## Provenance

Read in the tree at commit `9c448d4`, 2026-08-05: `worker/transcription.py`
`create_transcript_revision`; `worker/adapters.py` `transcript_create`,
`_onnx_sitting_embedder`, `profile_adopt`, `dispatch`; `worker/main.py`
`ALPHA_OPERATIONS` and `dispatch_without_protocol_output`;
`spike/dual_capture.py` `drop_offprint`, `voiceprint_provenance`,
`load_voiceprint`, `write_transcript`, `Voiceprint`; `spike/speaker_gate.py`
`load_profile`; `notes/transcript.py`; `crates/session-core/src/retention.rs`
`enroll_profile_candidate`; `crates/session-core/src/profile_lifecycle.rs`
`profile_present`, `initialize_or_open` and the reset swap (added 2026-08-05
after the existence-versus-size defect above — it was the one file this list
should have named from the start); `crates/session-core/src/library_read.rs`;
`docs/distribution-runbook.md` "Encoder-candidate lane". The lane question —
does the distributed DMG carry the encoder — was answered from the runbook's own
build-command record, not from a memory note that said the opposite.
