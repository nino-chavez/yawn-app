---
surface: installed saved meeting and Settings
build: 3a3d935
version: 0.6.3
device: installed app on this Mac, macOS 26.6.2
design_intent: preserve
design_direction: DIRECTION.md
reviewer: installed_cold_review, fresh context with images and user jobs only
kind: cold
cold: true
states: saved transcript; Apple selected; smaller speech model selected; optional note model and audio access
verdict: core choices are visible; wording follow-ups remain
not_reviewed: interactions, fresh setup, full restart persistence, permission prompts, downloads, real meeting quality
---
# Installed progressive setup screen review

The primary author and a fresh independent reviewer opened the captured frames.
The reviewer did not read implementation, prior reviews, or design rationale.
Private captures remain outside Git in the operator's research directory. This
record contains no meeting titles, transcript text, or other meeting content.

## Saved meeting

The selected meeting and available transcript are easy to find. The deleted-audio
state explains why retranscription is unavailable. No visible blocker was found
in the saved-meeting view.

## Speech and note choices

The current speech choice has an explicit In use badge. Alternatives distinguish
stored models from downloads and show storage costs. The smaller/full distinction
does not yet explain a measured quality or speed tradeoff. That copy must follow
evidence; the screen review does not establish that the larger model is better.

The note model reads as optional, local, selected, and removable. The reviewer
flagged that In use could imply active resource consumption. A future wording
pass should distinguish selection from current execution consistently across
speech and notes.

## Recording access

Settings distinguishes microphone access from system-audio access and exposes
an action for the latter. The main-window phrase about verifying a capture helper
is more technical and less actionable than the Settings explanation.

## Scope and follow-up

The visible current choices are understandable. Model tradeoff copy and the
recording-access explanation remain follow-ups. These frames do not prove that
a control works; the installed interaction receipt records those checks separately.

This fix preserves the existing layout. It removes the false finishing-message
for restored transcripts and the stale disabled state after an engine change.
It does not introduce another model tier or infer a quality ranking.
