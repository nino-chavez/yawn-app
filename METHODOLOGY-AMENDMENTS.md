# METHODOLOGY-AMENDMENTS.md — Yawn

Append-only, reverse-chronological. Convention:
`tools/blueprint/template/docs/methodology/methodology-amendments-convention.md`.

## 2026-09-02 — A cold review asks the category question when the brief names a platform

**Trigger**: The e7e96f9 cold review passed a Home screen composed as a marketing hero over a list, because the five job questions test legibility and a hero answers them. A blind reviewer asked one more question, "does this window read as an installed Mac app of its kind or as a web page", and named every composition fault in one pass.
**Scope**: Candidate for methodology promotion
**Bucket**: methodology
**Status**: Active

When the experience brief's platform strategy names a platform, the cold-review protocol (judged-screen pattern § 3a) adds a sixth question: does each surface read as an installed application of its category on that platform, and which elements make it read that way. The reviewer stays blind: no comparables are named to it. Applied first in `docs/evidence/screen-reviews/all-surfaces-bfa0a80-installed-cold.md`, section 11.

**References**:
- docs/design-rethink-2026-09-02.md
- docs/evidence/screen-reviews/all-surfaces-bfa0a80-installed-cold.md

## 2026-09-02 — The conformance review fails, not goes quiet, when the brief has no platform strategy

**Trigger**: `DIRECTION.md` declared `design_intent: refit` on 2026-09-01 against a direction whose platform strategy (required by judged-screen pattern § 2b) had never been written. The conformance gate had nothing to read and went quiet. The rendered app disagreed with the brief's "Mac-native, light chrome" from the first commit of the reset (`2d2b681`) and no review could see it.
**Scope**: Candidate for methodology promotion
**Bucket**: reviewer
**Status**: Active

For any app screen, the conformance review (§ 3b) reports a failure, not silence, when the direction record it reads lacks a platform strategy section. Declaring `refit` or `preserve` on a direction without one is itself a finding. For this initiative the strategy now lives in `docs/experience-brief-2026-09-02.md` and the selected concept's ADR.

**References**:
- docs/design-rethink-2026-09-02.md
- docs/experience-brief-2026-09-02.md
- docs/design-direction-decision.md
