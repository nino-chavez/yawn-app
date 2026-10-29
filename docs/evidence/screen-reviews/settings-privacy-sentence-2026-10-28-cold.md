---
surface: settings-privacy-sentence (the sentence that closes the Settings page, and the Updates disclosure when automatic checks are off)
kind: cold
build: da05c93 plus this change, uncommitted at capture
device: browser review harness (`ui/review/harness.html`, Settings opened from the sidebar gear) at 1080 by 900 CSS pixels, scaled to 800 wide by the capture; light and dark set from Settings › Appearance; not an installed app
reviewer: three fresh subagents (Claude), one per pass, each with the frames under neutral names, the capture settings and the five job questions only
implementer: the session that wrote the change and captured it (Claude), not any reviewer
cold: true
states: [settings-bottom-light, settings-bottom-light-automatic-off, settings-bottom-dark]
verdict: revise on every pass, each time for findings outside this change (below); the changed sentences were accepted by pass 3 with one change after it
---

# Settings privacy sentence — cold review, 2026-10-28

**Question:** From the bottom of Settings, can a person tell what Yawn sends or fetches over the internet, when, and whether any of it is automatic? Does anything contradict anything else, with the automatic update check on or off?

**Before:** Yawn 0.6.17's page ends: "Nothing leaves this Mac. Yawn has no account and makes no network calls with meeting data; the optional update check above is its only connection." That is untrue. A speech-model, speaker-model or Apple speech-file download also connects. Each of those starts only from a button: `install_transcript_model`, `install_nemotron_model` and `install_apple_speech_assets` are called only from click handlers in `main.js` and `settings.js`. The 2026-10-09 wording, "Its only automatic connection is the optional update check", was true. The Settings-page rewrite (b6faf52) dropped "automatic".

| Pass | Frames | Verdict | Findings on this change |
|---|---|---|---|
| 1 | 3: light, light with automatic checks off, dark | revise | With checks off, "Its only automatic connection is the optional update check above" describes a connection that isn't happening. The 2026-10-09 pass 2 noted the same staleness. |
| 2 | 2: light with checks off, dark | revise | The off disclosure "Yawn contacts the release server only when you choose Check now" contradicts a model download from the same server. That is true in fact: speech models come from the same R2 host as the update feed (`worker/build_manifest.py:46`, `tauri.conf.json` updater endpoint). |
| 3 | 2: dark (pass 2's frame; its content did not change), light with checks off | revise | The privacy claims are "consistent with every control shown". But "only for the update check above" leaves out the update download that Update and restart makes. |

**Changed:**
- Pass 1. The closing sentence became "Nothing from your meetings leaves this Mac, and Yawn has no account. It connects to the internet only for the update check above and to download the models or speech files you choose." It is true with the switch on or off.
- Pass 2. The off disclosure became "Automatic checks are off. Yawn checks for updates only when you choose Check now."
- After pass 3. "for the update check above" became "for the updates above", which covers both the check and the update download. This one-phrase change was not reviewed by a fourth pass. Frame: `settings-privacy-sentence-2026-10-28/final/settings-bottom-light.jpg`.

**Raised but not changed, with reasons:**
- **The sentence's place and size.** Two passes said it reads as part of Updates, is the smallest text, and has no heading; both suggested a Privacy group. DESIGN.md places it deliberately ("The one privacy sentence closes the column", operator amendment 2026-10-16). Moving it is a design decision for the operator, not part of a copy fix.
- **No "last checked" time, and "Up to date" repeats "This is the newest version of Yawn."** Raised on all three passes. This is the Updates row from 2026-10-09, not this change. It is a real gap, worth its own change.
- **The on disclosure's wording.** Reviewers questioned "account details" next to "no account". They asked for the trigger and the time of the daily check. They asked that the internet-address clause also show when checks are off. All of this is the 2026-10-09 disclosure. Its pass 1 found that a "sends only the version number" line was untrue, so no reviewer's suggestion to state what is sent was taken without a source.
- **"Checking speaker analysis … Checking" that never resolves.** This is the harness stub's state, not a shipped behavior.
- **Model rows and the model link.** The rows give no "Downloaded" label, no Compact size, and "Turbo weights" is unexplained. The NVIDIA link doesn't say it opens a web page. These rows were outside this change.
- **Dark-mode legibility.** Reviewers found secondary text low in contrast, the selected gear faint, and the checkbox far from its label. These are existing styles; this change did not touch them.

**What was removed, combined, demoted or hidden:** nothing. Two existing sentences were reworded. The review harness now loads `settings-window.css` as `index.html` does. Without it, the Settings page rendered unstyled, because the stylesheet was added to `index.html` when Settings became a page.

Frames: `settings-privacy-sentence-2026-10-28/pass-1/`, `pass-2/`, `pass-3/`, `final/`; neutral-name maps in `review-map-pass*.txt`.

Checks: `npm run test:ui` passes, 195 tests. They assert that the disclosure follows the switch, not its wording.
