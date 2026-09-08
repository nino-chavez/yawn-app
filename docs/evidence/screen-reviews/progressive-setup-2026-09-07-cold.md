---
surface: progressive setup and Settings
build: source on feature/progressive-setup, before packaging
device: synthetic browser harness on this Mac
reviewer: bundle_visual_gate, fresh context with images and user job only
kind: cold
cold: true
states: Apple language files required; Settings with compact speech and note models active
verdict: no visible blocker in the supplied frames
not_reviewed: installed app, post-download state, full Settings scroll, older macOS, real model downloads
---
# Progressive setup cold screen review

The independent reviewer opened both images without reading the implementation,
previous reviews, or design rationale. The images contain synthetic state.

## Setup

[Reviewed setup frame](captures/progressive-setup-2026-09-07/apple-assets-required-reviewed.png)

The Apple download is the obvious next action. The smaller speech model is a
clear alternative, and optional notes are deferred to Settings. No visible
blocker was found. The frame does not establish the selected state after a
successful download; that behavior remains a packaged-app check.

## Settings

[Reviewed Settings frame](captures/progressive-setup-2026-09-07/settings-engines.png)

The compact speech model is marked in use. The larger alternative has a download
and use action. The note model is marked in use, with its optional role and
removal consequence explained. No visible blocker was found in the shown area.
This capture is scrolled and does not cover the complete Settings window.

## Scope

Setup demotes the larger speech model to Settings and keeps notes optional.
This record verifies only the captured browser presentation. It does not prove
installed-app behavior, actual downloads, or speech and note quality.
