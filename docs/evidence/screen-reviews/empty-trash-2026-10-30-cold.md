---
kind: cold
date: 2026-10-30
surface: Mac Trash and permanent-deletion confirmation
design_intent: preserve
device: native macOS WKWebView, real bundled frontend, 1080 by 760 CSS pixels, light and dark, invented meetings and stubbed Tauri bridge
reviewer: two independent Operator-dispatched Codex workers, neither given source or rationale
cold: true
verdict: pass
---

# Empty Trash is explicit about permanent loss

The operator requested an Empty Trash button to delete meetings without
waiting 30 days. The existing list and shared confirmation treatment stay.
The new sheet names the count and the loss of recordings, transcripts and
notes. It starts with focus on Cancel and performs no deletion until the
operator confirms.

## Cold review

The author opened the populated and confirmation frames before dispatching
pass 1, and the revised busy frame before pass 2. Each reviewer received only
neutral frame names, capture conditions and the questions: what is happening,
what can I do, what happens if I act, and can I understand what would be lost?

Pass 1: 12 frames passed. The two busy frames needed revision because the
heading still asked a question and Cancel looked available. The interaction
test proved it was disabled, but the rendered reading was ambiguous. The
heading now says "Deleting N meetings…", and Cancel is hidden only while
irreversible deletion runs. Pass 2: all 14 frames passed. The reviewer found
no material visual or reading defect; noticeable focus outlines did not
obscure the state or choices. No other existing action was removed, combined,
demoted or hidden.

Operator receipts (requested and runtime-reported route agree: gpt-5.6-terra,
high effort): pass 1 `f3f8402d-a472-4bd0-922c-493b822794a1`; pass 2
`8863b547-671f-499f-9560-412864ee6fad`. Full reports, receipts and frames are
retained at repository-root `.artifacts/empty-trash-2026-10-30/`.

| Neutral frame | Rendered state | SHA-256 |
|---|---|---|
| 01 | trash-busy-dark | `e4c4bb5d6310ec6f48fab7ec052d3b47034afef23519e85216fa5dc15af5f515` |
| 02 | trash-busy-light | `cdbeffeca46aeaa2641b7486fba1ac113c4b54c836f56c528652b1c9ecc3cf06` |
| 03 | trash-confirm-dark | `a97eb0e49ec39a76f0b5cc135960f6cc6288db3aed1e9d81b369a4c478fcdad4` |
| 04 | trash-confirm-light | `3c95233cab42f131a7b5f94a3deafdbca4015bf976688079bc90c7309bea1b13` |
| 05 | trash-empty-dark | `ef75be2bb2dc2491540082271598e5a1e38c24863d936b54fc80175c753d9921` |
| 06 | trash-empty-light | `43e89f99db8ed41982a07a5a450eb0d27a1e7a2e9ea44557719ea5cfda8c9936` |
| 07 | trash-failure-dark | `1991152c0cae2b75c0c8618283ac50ed0fce247945dd6d8281857961ca16085a` |
| 08 | trash-failure-light | `8279cd07dc0d7ce5c36e5316a54e749df6929bf66bf3dcdb1b9930d57f633cc7` |
| 09 | trash-partial-dark | `725fa670495f9ab84a1fe438ff8689606846230e0dbcf62900640605cfdca2ae` |
| 10 | trash-partial-light | `2fc1abdf0caa9c68ea9e2b14afa234f998e35a4b1cc3cb580ef8f2a99143462c` |
| 11 | trash-populated-dark | `ad288fde56a4941943a902d1c4bc779218f114db7bda6092b827479d0a8865ec` |
| 12 | trash-populated-light | `169d9d7287d4fdcf0560d0342caa39376a70fc4080fc0d7eecd5c35afa92ce1b` |
| 13 | trash-unavailable-dark | `1b4de7c4e65a4af3d26905b28f2bffa3100d37971b3e57748e44f6e37398f822` |
| 14 | trash-unavailable-light | `c9c1fe12f820f7388782052eabefe860003b6b84dee433a285547c9d2fcb40e2` |

## Behavior and deletion evidence

- UI unit suite: 215 passing. Native WebKit interaction scenario `run.sh trash`
  exercises Cancel, Escape, safe initial focus, Tab containment, one request
  for repeated clicks, the busy state, refreshed empty state and listing errors.
- The same scenario with `trash-negative` failed on the empty-list check when
  the bridge falsely reported success without deleting its synthetic entries.
- Session-core library suite: 564 passing, 2 ignored. Six new tests cover
  immediate deletion of existing-format receipts, preservation of live meetings,
  later additions, restored/re-trashed meetings within the same second, active
  entries, malformed or changed entries, collisions and symlink replacements.
- The same-second re-trash test failed (exit 101) when receipt-file identity
  checking was temporarily removed. The original source was restored in a
  `finally` block; all six deletion tests passed again.
- Desktop binary suite: 299 passing. Build-matrix suite: 6 passing. Shell
  capability suite: 6 passing after its expected permission list was updated.
  Final focused command tests passed: confirmation required, single-use review,
  cancellation, forged/stale handle and unavailable writer authority.

The backend keeps an in-process snapshot of open, private Trash receipts.
Holding the files pins their identities, so even a same-second restore and
re-trash cannot reuse an earlier confirmation. The same receipt and its bytes
are rechecked under the meeting lease and storage-sequence lock before the
existing durable deletion path runs. New entries are absent from the plan;
live-library paths are refused. Cancel releases the plan. No saved receipt
schema or automatic 30-day schedule changed.

This is source/test and native-WebKit fixture evidence. It does not establish
an installed release, VoiceOver behavior or physical use of the new command
against a real meeting. No user recording was deleted during validation.
