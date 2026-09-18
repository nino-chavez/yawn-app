---
surface: Record toolbar and system-audio allow (meeting selected + empty)
build: 2efbf5a
device: synthetic layout harness on this Mac (native WebKit)
reviewer: cold agent, did not build these surfaces and did not read their rationale
kind: cold
cold: true
states: mic authorized, system audio unmeasured; meeting selected; empty library
verdict: Empty library makes the system-audio gate the job; with a meeting open, past-meeting work competes with that gate.
not_reviewed: installed app, live TCC prompt, post-allow Record-enabled state, real meeting quality
---

# Cold screen review — Record / system-audio allow (2efbf5a)

Synthetic 900×600 harness frames only. No installed-app behavior inferred.

## Frames

| Frame | Path | Mark |
| --- | --- | --- |
| Meeting selected, light | [captures/record-system-audio-unlock/system-audio-unmeasured-light-900x600.png](captures/record-system-audio-unlock/system-audio-unmeasured-light-900x600.png) | **revise** |
| Meeting selected, dark | [captures/record-system-audio-unlock/system-audio-unmeasured-dark-900x600.png](captures/record-system-audio-unlock/system-audio-unmeasured-dark-900x600.png) | **revise** |
| Empty library, light | [captures/record-system-audio-unlock/system-audio-unmeasured-empty-light-900x600.png](captures/record-system-audio-unlock/system-audio-unmeasured-empty-light-900x600.png) | **accept** |
| Empty library, dark | [captures/record-system-audio-unlock/system-audio-unmeasured-empty-dark-900x600.png](captures/record-system-audio-unlock/system-audio-unmeasured-empty-dark-900x600.png) | **accept** |

Light and dark read the same for each scene; marks apply to both appearances of that scene.

## Job questions

### 1. What is happening now?

Microphone access is ready. Recording is not available yet: **Record** is dimmed, and the chrome states that system audio must be allowed first.

- **Meeting selected:** A past meeting is open (“Operations review…”), dated Sep 18, 2026, with transcript available and no meeting note yet. Another earlier meeting sits in the sidebar.
- **Empty library:** No meetings listed. The sidebar explains that private meetings land here after Record; a “Needs attention” card in the main pane restates the same system-audio requirement.

### 2. What is next?

Allow system audio (toolbar control; also the center card when empty). That is the stated precondition before recording.

On the selected meeting, equally loud next steps compete: **Generate note**, plus **Retry transcript**, **Rename**, and **Manage**. Those read as work on the past meeting, not as the unlock path.

### 3. Who is involved?

A single operator on this Mac. Copy frames private, on-device notes and transcript. No other people, accounts, or invitees appear. Meeting titles imply a workplace context but do not name participants.

### 4. When and where?

On this Mac, in the Yawn window. Selected meeting: today / Sep 18, 2026 (sidebar times 8:59 AM and 9:59 AM). Empty library: no meeting time yet—only the unlock before a first Record.

### 5. What can I do?

- **Always visible:** Allow system audio; search meetings; open Trash; toggle the sidebar.
- **Meeting selected:** Rename, Manage, Generate note, Retry transcript; Record remains unavailable until system audio is allowed.
- **Empty:** Same toolbar unlock; center **Allow system audio**; sidebar suggests trying a short note-to-self once Record works. Record itself is still unavailable.

## Blockers and hierarchy

- **Clear blocker (all frames):** System audio not allowed; Record disabled; mic ready. Message is consistent.
- **Empty library:** Hierarchy supports the job—attention card + purple allow control. Mild tension: sidebar says “Press Record…” while Record is still off; the card resolves it if you look at the main pane.
- **Meeting selected:** Unlock lives only in the toolbar while the body leads with note generation and transcript retry. Easy to miss that recording is gated, or to treat **Generate note** as the primary next step instead of the allow control. Sidebar titles truncate.
- Duplicate allow affordances on empty (toolbar + card) are redundant but not contradictory.

## Verdict

**Empty library: accept.** **Meeting selected: revise** so the system-audio gate is not overshadowed by past-meeting actions when Record is still blocked.
