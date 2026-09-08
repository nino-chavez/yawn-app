# Yawn distribution runbook

## 0.6.4 release receipt

**0.6.4 was released on 2026-09-08 with Apple speech setup, clearer Settings,
and the Lavender Haze dark palette. Matching-passage search changes are included
behind the existing default-off probe; search is not generally available.** PR #74 merged
source commit `58e13a276a352b7ef3d283951d52d037d2fad6af`, which produced the
signed artifact. Annotated tag `v0.6.4` identifies that exact source. The
[GitHub prerelease](https://github.com/nino-chavez/yawn-app/releases/tag/v0.6.4)
and [release-notes page](https://apps.ninochavez.co/yawn/release-notes/#yawn-0.6.4)
use the same reviewed release data. The commit corpus covers 19 commits after
`v0.6.3`; it is input for review, not automatically published commit messages.

The Rust workspace, UI harness, Python worker suite, distribution checks,
release-corpus test, Swift capture self-test, and ordinary workspace Clippy
check passed. An additional Clippy run with `-D warnings` failed on existing
style warnings in unchanged code. The site renderer's five tests passed,
including prior-release retention and deploying only public output.

The signed artifact is `Yawn-0.6.4-macos-arm64.dmg`, 553,335,506 bytes, with
SHA-256 `e45c96687ffab25e031c7ac946f328cce6417759134c7d1beaf664d0ee439c2a`.
Apple accepted app submission `880c9061-f94d-49cf-bb61-f46dee603ad4` and DMG
submission `ee2fea31-a6d6-4aae-9153-4a395bc47dd0`. Both artifacts were stapled.
Signing and independent release verification passed with 200 arm64-compatible
Mach-O files under the `internal-alpha` admission. Gatekeeper accepted the
release. The ten hosted speech- and note-model objects matched their recorded
byte counts and SHA-256 values after full streamed verification.

The [public installer](https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/Yawn-0.6.4-macos-arm64.dmg)
returned 200 with its recorded byte count, disk-image content type, and
immutable cache control. The first upload omitted the cache header because
`rclone --metadata-set` also requires `--metadata`. An S3 metadata replacement
corrected the header; a subsequent full public download matched the frozen
DMG's byte count and SHA-256. No public installer was deleted.

Landing-site commit `d8c1a2cf10309f7cb90317b84af7c649b95e3acc` was pushed to
main and deployed as production deployment
`bcfd9e7d-e14f-4eb9-b821-6ab5637cb98e`. The homepage, app card, and release-notes
page on both `bcfd9e7d.yawn-site.pages.dev` and `yawn-site.pages.dev` matched
the generated files byte for byte. The deployment uploaded only those three
public files from `dist/`. Desktop and phone-width WebKit captures showed
readable wrapping and no horizontal overflow on the release page; the phone
download card also kept the button, model disclosure, and checksum in bounds.

`/Applications/Yawn.app` was installed from the signed DMG and reports version
0.6.4. Its signature and Gatekeeper checks passed. The main executable's
SHA-256 is `40a86ab18fdaa5f82d05eb12e186d4ffa86fe1aa32236e81d18d63d3cc19bb16`.
The replaced app remains recoverable at
`/Users/nino/.Trash/Yawn-0.6.3-before-0.6.4-20260908T125406.app`.
Installed-app interaction acceptance was not performed in this release pass.
The operator deferred the real-meeting quality review; packaging and rendered
page checks do not close that review or establish improved speech accuracy.

### 0.6.4 follow-up: corrected claims and installed checks — 2026-09-08

The initial public notes overstated transcript-content search availability.
The homepage, release notes, app card, and GitHub prerelease now omit that
feature claim and explicitly identify transcript-content search as experimental
and disabled by default. Site commit `b77f933` was pushed and deployed at
`https://84972cb2.yawn-site.pages.dev`. The corrected version and wording were
checked on all three public files at that deployment, the Pages production
address, and `https://apps.ninochavez.co/yawn/`. GitHub's published release body
matches the corrected notes. The app artifact and release tag did not change.

Fresh checks against `/Applications/Yawn.app` report version 0.6.4, a valid
strict/deep code signature, a valid notarization staple, and Gatekeeper acceptance
as Notarized Developer ID. Its executable still matches the SHA-256 recorded
above. The installed `apple-speech --capabilities --locale en-US` command returned
`state: ready` on macOS 26.6.2. This command checked availability without
recording audio or installing assets.

Installed interaction acceptance remains pending. This session had no
`node_repl` tool, which the Computer Use skill requires for Mac UI interactions.
No recording was started, meeting edited, model selection changed, or acceptance
box checked. Source or synthetic-harness tests cannot stand in for these actions.

| Next installed check | Passing observation | Status |
|---|---|---|
| Existing library and engine selection | Open a saved meeting, change between Apple and a previously downloaded speech model, restart, and confirm the choice and original transcript persist; restore the initial choice | Pending |
| Fresh setup and downloads | In a separate disposable account, inspect first setup, missing Apple assets, optional speech/note downloads, and interrupted-download recovery without resetting the operator's library | Pending |
| Record, pause, stop, and reopen | With a consenting operator and a synthetic spoken script, complete a two-source take, restart, and reopen its transcript; record interruptions as failures rather than completed takes | Pending |
| Speaker correction and generation | In a disposable meeting, change one source group, reopen, regenerate, and confirm only intended attribution changes and source links remain valid | Pending |
| Transcript retry and note preservation | In a disposable meeting with retained audio, compare a retry, keep the original, then promote a retry; observe note invalidation and regeneration without losing the source record | Pending |
| Source playback | Open a generated claim's source and seek its retained audio; verify the intended passage and the unavailable-audio explanation | Pending |

Use disposable meeting data for mutating checks. Do not attest that the operator
is alone or wearing headphones on their behalf, and do not reset their existing
library to manufacture fresh setup. Human speech/note quality judgment remains
explicitly deferred. Native quiet-system-audio cadence and device-interruption
checks follow this acceptance pass; system-audio stall detection stays disabled
until those observations support it.

## 0.6.3 release receipt

**0.6.3 was released on 2026-09-04 as a performance patch for switching
between meetings.** The artifact was built from `main` commit `b6e3008`. The
detail view now reuses the note-generation availability answer within one app
session and invalidates it when the model or storage context changes. The
actual generation path still performs its full runtime and model admission
immediately before spawning, so the navigation cache cannot authorize work.
Meeting-note and transcript opens also run off the macOS event loop. The 269
desktop tests, full Rust workspace, UI harness, Clippy, and release-bundle
checks passed before the release was cut.

With the same eight-meeting library, six steady-state switches in the installed
0.6.2 app took 1.30–1.84 seconds. A signed Preview bundle carrying the 0.6.3
fix took 195–217 milliseconds across six switches. The screenshot-based
measurement itself cost 116–142 milliseconds, so these are visible-response
upper bounds rather than pure command timings. The final installed 0.6.3 bundle
was not re-driven with synthetic UI input; the measured interaction receipt
belongs to the signed Preview comparison.

The signed artifact is `Yawn-0.6.3-macos-arm64.dmg`, 549,375,672 bytes, with
SHA-256 `9f7bbf04d95ce7300bea0b27d9066a01365d21619ad385cb48c4242fe5c91c38`.
Apple accepted app submission `ec8ce000-e1e1-46a0-856a-6baaf00b47f4` and DMG
submission `18ca2a7d-c716-4abb-8af9-b9515f06dde0`. Both artifacts were stapled,
Gatekeeper accepted them with `source=Notarized Developer ID`, and an
independent signed-release verification passed with 199 arm64-compatible
Mach-O files under the `internal-alpha` admission.

The four immutable transcript-model objects were downloaded from their public
catalog URLs and matched their sealed byte counts and SHA-256 values before
signing. The public installer returned 200 with the recorded byte count,
disk-image content type, byte-range support, and immutable cache control.

Landing-site commit `72ffbb3` was manually deployed to Cloudflare Pages as
production deployment `91832d6e-df42-467c-b93a-d7f8e069ac34`. Both that
deployment and `yawn-site.pages.dev` showed version 0.6.3, the exact installer
URL, the recorded checksum, and the measured switching release note. The
public app card also rendered version 0.6.3.

`/Applications/Yawn.app` reports version 0.6.3, passes strict code-signature,
staple, and Gatekeeper verification, and its main executable matches the frozen
release hash `eee00e581832a8431787f09b7ab898bffd4f164d02c2ad53382597ed96ee291a`.
The replaced 0.6.2 app remains recoverable at
`/Users/nino/.Trash/Yawn-0.6.2-replaced-2026-09-04.app`. No public installer was
deleted.

## 0.6.2 release receipt

**0.6.2 was released on 2026-09-03 as a performance patch for meeting-library
startup and refresh.** The artifact was built from `main` commit `e0a4ac9`. The
change reuses an accepted in-memory library projection when its source files are
unchanged, avoids hashing the 8.06 GB note model for read-only projection, and
runs snapshot work off the macOS event loop. The full Rust workspace and UI
harness passed before the release was cut.

With the same eight-meeting library and installed note model, the previous
0.6.1 app remained busy for about 39 seconds after a cold launch. Two
release-shaped 0.6.2 launches settled near idle in about five seconds. The
installed notarized build repeated that result: its main process used 5.6% CPU
at four seconds, 1.1% at five seconds, and 0.4–1.1% from six through nine
seconds. These are same-machine CPU timings, not a direct user-task latency or
acceptance measurement.

The signed artifact is `Yawn-0.6.2-macos-arm64.dmg`, 555,660,774 bytes, with
SHA-256 `49cf46d440a5e2a8cc34562877ea00500fa395e8d00db8f7abb6623736d06ac7`.
Apple accepted app submission `c6ee6c2d-9c53-4202-aa88-d34a2f8f2a15` and DMG
submission `f57b7bc1-4feb-45e0-9bd2-4a89b7d265cf`. Both artifacts were stapled,
Gatekeeper accepted them with `source=Notarized Developer ID`, and the signed
release verifier passed with 199 arm64-compatible Mach-O files under the
`internal-alpha` admission.

The public installer URL returned 200 with the recorded byte count,
disk-image content type, byte-range support, and immutable cache control.
Landing-site commit `df769f1` was manually deployed to Cloudflare Pages as
production deployment `94e86015-262e-4a4a-854c-53753540518d`. Both that
deployment and `yawn-site.pages.dev` showed version 0.6.2, the exact installer
URL, the recorded checksum, and the performance release note; the public app
card matched.

`/Applications/Yawn.app` reports version 0.6.2, passes strict code-signature,
staple, and Gatekeeper verification, and its main executable matches the frozen
release hash. The replaced 0.6.1 app remains recoverable at
`/Users/nino/.Trash/Yawn-0.6.1-replaced-2026-09-03.app`. No public installer was
deleted.

## 0.6.1 release receipt

**0.6.1 was released on 2026-09-03 as a visual-fidelity patch for the Tonal
Ledger meeting window.** The artifact was built from `main` commit `3e2be45`.
The production UI passed the 25-check WKWebView geometry contract, the full UI
harness, and a cold screenshot review against the selected reference before
the release version was cut.

The signed artifact is `Yawn-0.6.1-macos-arm64.dmg`, 550,743,801 bytes, with
SHA-256 `68548aaca254d39798b032d960c447a7deb6886fa4bc33b6979ea71dba93cb90`.
Apple accepted app submission `5c64ff27-8d91-4064-b558-4c4d404ac4f0` and DMG
submission `c4a841ba-1612-4767-9695-39750db83a4a`. Both artifacts were stapled,
Gatekeeper accepted them with `source=Notarized Developer ID`, and the signed
release verifier passed with 199 arm64-compatible Mach-O files under the
`internal-alpha` admission.

The public installer URL returned 200 with the recorded byte count, disk-image
content type, and immutable cache control. Landing-site commit `7f446a6` was
manually deployed to Cloudflare Pages as production deployment
`f8045d3e-288a-4ac0-a4ed-e2fd50f3488a`. Both that deployment and
`yawn-site.pages.dev` showed version 0.6.1, the exact installer URL, the recorded
checksum, and the `Internal alpha` label.

`/Applications/Yawn.app` reports version 0.6.1, passes strict code-signature,
staple, and Gatekeeper verification, and its main executable matches the frozen
release hash. The installed sparse-meeting state is recorded in
`docs/evidence/screen-reviews/captures/tonal-ledger-fidelity-2026-09-03/installed-0.6.1-dark.png`.
The replaced 0.6.0 app remains recoverable at
`/Users/nino/.Trash/Yawn-0.6.0-replaced-2026-09-03.app`; no public installer was
deleted.

## 0.6.0 superseded release receipt

**0.6.0 was released on 2026-09-03 with the first Tonal Ledger shell, then
superseded by 0.6.1 after installed-window review found that its text-button
padding and document spacing did not match the approved design.** It was built
from `main` commit `f4a11ff`.

The signed artifact is `Yawn-0.6.0-macos-arm64.dmg`, 550,312,865 bytes, with
SHA-256 `8fca220963ce80da05f9aa50dba283b7582cc2da0ddddcdf65d4f0de38cb59ee`.
Apple accepted app submission `fab52b73-872c-4759-ae1b-957ca8c98dfc` and DMG
submission `0dd23d64-5df7-4c7a-b97c-5c16da37b088`. Landing-site commit
`b4be6ed` was deployed to Cloudflare Pages as production deployment
`4840ba9a-a7d4-4676-8901-4627c983dbc2`. The immutable 0.6.0 installer remains
public for release archaeology, but the landing page no longer offers it.

## 0.5.9 release receipt

**0.5.9 was released on 2026-08-19 with queued local transcription between
meetings.** The artifact was built from
`d74b6ba599eaab23aa8502b9e2d0e8fdc40a2c7f`; PR #72 merged that source into
`main` as `16e2650a96d1a18eff04efa21a7d5d17e4461c02`.

The signed artifact is `Yawn-0.5.9-macos-arm64.dmg`, 348,709,396 bytes, with
SHA-256 `e148ea3bcbee0764a340699fdd560f8f75c556c84ae7648210e5e0fde2be23e8`.
Apple accepted app submission `4c24d974-51d3-43ce-903c-825a284670fd` and DMG
submission `4e5b95b7-0f72-4f27-b6b3-f26c432484e2`. Both artifacts were stapled,
Gatekeeper accepted them with `source=Notarized Developer ID`, and the release
lane's signed verifier completed before writing the checksum sidecar.

The public installer URL returned 200 with the recorded byte count, disk-image
content type, byte-range support, and immutable cache control. Landing-site
commit `fb4a28f` was merged through PR #2 as `58edb284a3ef3931add74462e68b741b46af535f`
and manually deployed to Cloudflare Pages as production deployment
`02e5123f-3e0a-413c-b5a9-501c11d0581b`. That deployment and
`yawn-site.pages.dev`, fetched with `Cache-Control: no-cache`, show version
0.5.9, the exact installer URL, and its checksum. No superseded installer was
deleted during this release.

## 0.5.8 release receipt

**0.5.8 was released on 2026-08-16 with summary-first local meeting notes.**
The artifact was built from release-branch commit
`245fd16bf8aa51a262857440b004f6731fd69cff`; draft PR #72 carries that exact
source for review.

The signed artifact is `Yawn-0.5.8-macos-arm64.dmg`, 345,630,762 bytes, with
SHA-256 `207b430d7499b19e44dcc5e68d997e9e9e046eeb5964e48ccc9d40f70d19216d`.
Apple accepted app submission `cd1407ba-e36a-4f89-b4db-970c44535361` and DMG
submission `f4855522-06c5-4d24-ba81-379700e33d8f`. Both artifacts were stapled,
Gatekeeper accepted them with `source=Notarized Developer ID`, and the
independent signed-release verifier passed with 169 arm64-compatible Mach-O
files under the `internal-alpha` admission.

The public installer URL returned 200 with the recorded byte count, disk-image
content type, immutable cache control, and a full streamed SHA-256 match. The
six immutable note-model objects also returned 200 with the exact byte counts
in the signed catalog. The 8.06 GB note model remains a separate, verified
download; it is not embedded in the DMG.

Landing-site commit `310dd93` was manually deployed to Cloudflare Pages as
production deployment `4b062c2f-a3db-4d24-ba52-61ceb809e88f`. Both that
immutable deployment and `yawn-site.pages.dev` displayed version 0.5.8, the
exact public installer URL and checksum, the summary-first note description,
and the separate note-model disclosure. Draft PR #2 carries the site source
for review. No superseded installer was deleted during this release.

## 0.5.7 release receipt

**0.5.7 was released on 2026-08-12 with speech-model management in Settings.**
The artifact was built from `main` commit `da14c3726d721001d9372fcf0a4dd3da961674c0`.
PR #69 squash-merged the model-management feature at `120b3b2`, and PR #70
squash-merged the 0.5.7 version bump at the release commit above.

The signed artifact is `Yawn-0.5.7-macos-arm64.dmg`, 345,222,032 bytes, with
SHA-256 `eea3973de85260e93afa4f1d25fdca9c7b2fc94b623a3eb6d86ac48c0d59875d`.
Apple accepted app submission `4f1bf227-c7c4-4baf-9f3e-f601050a379e` and DMG
submission `80f07c2c-ee96-4ce1-a039-cf84ee3549b6`. Both artifacts were stapled,
Gatekeeper accepted them with `source=Notarized Developer ID`, and the independent
signed-release verifier passed with 169 arm64-compatible Mach-O files under the
`internal-alpha` admission.

The public installer URL returned 200 with the recorded byte count, disk-image
content type, immutable cache control, and a full streamed SHA-256 match. Landing
site commit `4c54b3b` was manually deployed to Cloudflare Pages as deployment
`79a02781`; both that immutable deployment and `yawn-site.pages.dev` displayed
version 0.5.7, the exact public URL, and the matching checksum. The installed
`/Applications/Yawn.app` reports 0.5.7, passes strict code-signature and staple
validation, and completed a fresh local startup. This is packaging and startup
evidence, not a transcript-accuracy or automatic-note judgment.

Only after the replacement and landing page were live, the exact superseded R2
keys `Yawn-0.5.6-macos-arm64.dmg` and
`Yawn-0.5.6-macos-arm64.dmg.sha256` were deleted and returned 404. The 0.5.7
installer remained public, and both immutable transcript-model weight objects
remained reachable with their catalog byte counts.

## Downloadable transcript models (0.5.6 release lane)

Release 0.5.6 uses `app-runtime/2`. The signed and notarized app contains
the Python runtime, capture helpers, MiniLM embedding model, and
`model-catalog.json`. It does not contain Whisper weights. First launch is the
installer step: the user chooses either the 464 MB Q4 model or the 1.61 GB full
Turbo model, and Yawn downloads only that choice into its private Application
Support directory.

This is intentionally not a second installer executable. A bootstrap installer
would add another signed, notarized, updated, and supported program while still
needing the same download and verification code. Keeping setup in Yawn also
lets an interrupted download fail before activation and retry through the same
visible first-run surface.

After setup, Settings shows which model is active and which downloads remain on
the Mac. A user can download or switch models between meetings and remove an
inactive download to reclaim space. Yawn never offers to remove the active
model, and the backend refuses that operation even if a caller bypasses the
window.

`model-catalog.json` is sealed into the app by its code signature and then bound
again by `app-runtime.json`. Each catalog entry pins an immutable upstream
revision, exact object URL, filename, byte count, and SHA-256 digest. The Rust
installer writes to a private staging directory, verifies the exact inventory,
renames it atomically, and only then writes the active-model receipt. The Python
worker independently rechecks that receipt and every file before reporting
ready. Meeting audio never participates in this network request.

Build the smaller app runtime with:

```sh
worker/build_runtime.sh build-alpha-external
```

Prepare the R2 upload tree with:

```sh
scripts/prepare-model-hosting.py \
  --q4-dir <pinned-q4-snapshot> \
  --full-dir <pinned-full-turbo-snapshot> \
  --note-dir <pinned-gemma-note-snapshot> \
  --output <new-empty-staging-directory>
```

The resulting `hosting-manifest.json` is an upload receipt, not runtime
authority. Upload its object keys to `yawn-releases` with immutable cache
control (`--note-dir` adds the six note-model keys; omit it to stage only the
four transcript-model keys). Objects over wrangler's 300 MiB `r2 object put`
ceiling — both note-model weight shards — go through the S3 API instead: the
scoped key in 1Password ("Cloudflare yawn-app", object read/write on
`yawn-releases`) with any S3 client, e.g. rclone with `--s3-no-check-bucket`
(the key cannot create buckets, and the client must not try). Verify public
byte counts and full downloaded hashes before building the app that
advertises those URLs. Do not overwrite an existing revision key; a changed
model gets a new revision path and a newly signed app catalog.

The operator reported that the completed 0.5.6 checks passed. That report is not
a recorded accuracy measurement: no transcript content or comparison score is
stored in this runbook. The packaging evidence proves that MLX loads the
`weights.npz` format and that the model boundary is exact.

**0.5.6 was released, 2026-08-12, with downloadable transcript models.** The
artifact was built from release head `0327ad8`; PR #67 squash-merged the identical
source tree to `main` as `732e075`. The DMG is
`Yawn-0.5.6-macos-arm64.dmg`, 345,913,055 bytes, with SHA-256
`a33c7ac6603ebf43243f58e46be5b0f69dd818a20f58ee4270fecdbc0e890859`.
Apple accepted app submission `3aa442be-8a18-40a7-87e3-c51a7aed49c0` and DMG
submission `6db1de15-e3d2-4d3b-863a-4b9574493c54`; both artifacts were stapled
and Gatekeeper accepted them with `source=Notarized Developer ID`.

The app copied from the DMG passed `verify-signed-release.sh` for the
`internal-alpha` admission with 169 arm64-compatible Mach-O files. The installed
`/Applications/Yawn.app` reports version 0.5.6, passes strict code-signature
verification, and has a valid staple. The public DMG URL returned 200 with the
recorded byte count, disk-image content type, immutable cache control, and a
full streamed SHA-256 match. The live `yawn-site.pages.dev` page showed the same
version, URL, and digest.

After those checks, the superseded R2 key
`Yawn-0.5.5-macos-arm64.dmg` was deleted and returned 404. No matching 0.5.5
checksum sidecar existed. The 0.5.6 DMG remained public with a 200 response, and
the four immutable model objects remained untouched.

**0.5.1 was cut, 2026-08-08, and it is the only image on the operator's desktop.**
It exists because 0.5.0 was cut at `e39f576` hours before `corpus-scan-bench` was
pointed at exact search, and carries the defect that hour found: a word appearing
more than a hundred times across a library returns **no results at all**, with
"That search has too many matches." Measured at 5, 20, 200 and 800 meetings — it
refused at every one, so it is a first-week failure rather than a scale limit.
Fixed in `ca07ca6` (US-13.14); exact search now returns the hundred most recent
matches and says how many it cut from.

**0.5.1's record.** Built at commit `d3468ea` on `main`; DMG SHA-256
`c409ce7fa1d215ae82b38d84408ef19662f5df99cac070528ebbd12420ab1e0e`,
1,838,398,703 bytes, 169 arm64-compatible Mach-O files. Notarization submissions
`7b2190e2-05c4-49c3-9313-536d94546a9e` (app) and
`ed7fc15c-0b7a-48ec-ba6b-652dd2dd5cfb` (image), both **Accepted**; both stapled,
both Gatekeeper-accepted with `source=Notarized Developer ID`, checked twice each.
`verify-release-bundle.py` PASS (signed, 0.5.1, internal-alpha) and the lane exited
0. Re-verified afterwards under `script`, the pty-backed form this file recommends:
`verify-signed-release.sh … internal-alpha` PASS at exit 0, from a run written
down, and that run recomputed the same DMG digest independently of the lane. Its
interactive operator run is open, and it inherits the chain from 0.2.2, 0.3.0,
0.3.1, 0.4.0 and 0.5.0 rather than clearing it.

**The 0.5.0 image was deleted, 2026-08-08, after 0.5.1's copy verified.** Two
images in one folder, one of them broken, is how the wrong one gets installed —
and this is the folder the operator is being pointed at. The order was: copy in,
re-digest the copy at its destination, compare to the source, and only then
delete — so a truncated copy could never be the survivor. The `.sha256` sidecar
went with it; an orphaned checksum naming a file that is gone is the same
confusion in a smaller font. Nothing is lost by the deletion: 0.5.0 is
reproducible from `e39f576`, and its digest stays recorded below.

**Deleting the image does not uninstall the app.** If 0.5.0 was already dragged to
Applications, the installed build still refuses common words until 0.5.1 is
installed over it. The image being gone means nobody installs it *again*.

**The image is 1.6 MB smaller than 0.5.0's** — 1,838,398,703 bytes against
1,840,005,044 — carrying the same 169 Mach-O files and the same model set. Recorded
because a smaller image after a fix invites the guess that something is missing,
and the guess is checkable: `verify-release-bundle.py` digests every model against
the manifest rather than taking the manifest's word, and it passed. What was not
checked is the two images byte for byte, and this file does not claim it.

**0.5.0 was cut, 2026-08-08 — the first build in which a person can ask the
corpus a question — and recalled the same day.** Its image is deleted and it must
not be installed; everything below is why it was cut and what its lane recorded,
kept because 0.5.1 is the same build plus one fix and inherits every fact in it.
Everything the feature needs landed across #45–#53 and none
of it is in any image: the embedding model entered the runtime on 2026-08-08 and
the search surface the day after. 0.4.0 cannot do any of this, so the standing
question — is 7 of 10 useful — has had no build to be asked in.

That is what this cut is for, and it is worth being exact about what it does not
settle. **The retrieval figure is unchanged and unjudged: 7 of 10, and 3 of 5 on
the questions exact search cannot answer** (`notes/SEMANTIC_RETRIEVAL.md`). A
build cannot move it. What a build allows is the operator answering it against
their own meetings, which is the one gate this repository has never been able to
close for itself.

It also inherits, and does not clear, the open interactive operator runs from
0.2.2, 0.3.0, 0.3.1 and 0.4.0 — and 0.4.0's speaker gate, whose threshold has
still never been measured on live meeting audio.

**0.5.0's record.** Built at commit `e39f576` on `main`; DMG SHA-256
`5dc8b760d0f3cb17b37eaffc766123e722762ef82964205b4767492dbf53fae3`,
1,840,005,044 bytes; signed, notarized, stapled, and Gatekeeper-accepted for
both the app and the image — `source=Notarized Developer ID` on each, checked
twice — with `verify-signed-release.sh … internal-alpha` PASS and the lane
exiting 0. Re-verified afterwards under `script`, the pty-backed form this file
recommends: PASS at exit 0 again, from a run written down. Its interactive operator run is open, and it inherits the chain from
0.2.2, 0.3.0, 0.3.1 and 0.4.0 rather than clearing it.

**It is the first build cut from `main` rather than a feature branch**, which is
what the 2026-08-07 trunk consolidation was for. Every prior record names a
commit on `codex/guided-voice-enrollment`.

**The image is smaller than 0.4.0's** — 1.840 GB against 1.858 — while carrying
87 MB of embedding model it did not have. Recorded because the arithmetic looks
wrong and is not: DMG compression, not a missing component. The four MiniLM
files are physically present at `Contents/Resources/models/all-MiniLM-L6-v2`,
and `verify-release-bundle.py` digests each one against the manifest rather than
taking the manifest's word.

**The release verifier refused this build once, correctly.** Its `expected_models`
allowlist is a second, independent statement of what a bundle may carry, and the
four MiniLM entries were added to `worker/build_manifest.py` on 2026-08-08 and
not to it. Fixed in `e39f576` along with a test that compares the two files, so
the next drift of that kind fails in an ordinary test run rather than at the
signing lane.

**Two things a first user of this build should be told.** Meaning search finds
nothing until passages are prepared, which is a press on the Find screen and
runs entirely on this Mac; the surface says so rather than reporting an empty
result. And preparing is bounded per press, so a large library takes more than
one.

**0.4.0 is being cut, 2026-08-05 — the operator made the call.** The two
user-visible changes that sat in the repo and in no image ship in it: the
copy-transcript control (`f0302aa`) and the whole speaker gate, including the
two warnings the transcript screen now carries — that a recurring voice is being
removed, and that the gate's threshold was measured on enrolment recordings
rather than meeting audio.

The decision was made against a stated cost, which does not disappear because
the call went the other way. 0.2.2, 0.3.0 and 0.3.1 each still have an open
interactive operator run, and 0.4.0 inherits that chain rather than clearing it.
It adds a gate whose threshold has never been measured on live meeting audio and
which can mark a colleague's speech as non-operator. What bounds the risk is the
gate's entry condition, not its accuracy: `_installed_voiceprint_gate` in
`worker/adapters.py` runs no gate at all when no profile is installed, so a
cohort tester who never opens voice setup gets a transcript identical to
0.3.1's, and `voiceprint: null` keeps meaning exactly what it has always meant.
The gate reaches a transcript only after that tester deliberately enrols.

**0.4.0's record.** Built at commit `331c9e9` on `codex/guided-voice-enrollment`;
DMG SHA-256
`3634049ed5f8eb80f773db6b8a970a515a091640102ede628b9c6f1bf459b22d`,
1,857,662,331 bytes; signed, notarized, stapled, and Gatekeeper-accepted for both
the app and the image, with `verify-signed-release.sh … internal-alpha` PASS at
exit 0. Its interactive operator run is open, and it inherits the chain from
0.2.2, 0.3.0 and 0.3.1 rather than clearing it.

Status, 2026-08-05: the current cohort DMG is **0.3.1**. Built at commit
`9f0246e` on `codex/guided-voice-enrollment`; DMG SHA-256
`0797ea8df1b5a4fa9ca119463b36ed6d2b406c3e79d29cccc7a76e7df9058549`, signed,
notarized, stapled, Gatekeeper-accepted for both the app and the image, and
`verify-signed-release.sh … internal-alpha` PASS on a traced run. Uploaded to
R2, page deployed and confirmed past the edge cache; the 0.3.0 object was
deleted afterwards, not before — see the sequencing note in the site repo.
0.3.1 carries one change over 0.3.0: the record control stays reachable from a
finished transcript, the first defect a cohort operator reported. Its
interactive operator run is still open, and it inherits that gate from 0.3.0.

0.3.0 was the first build whose installed name is **Yawn**. The display rename
was the deliberate 0.3.0-class
change recorded in `docs/brand.md`: `productName`, the window title, the tray
entry, and the two macOS permission prompts now read Yawn, while the bundle
identifier stays `com.ninochavez.local-meeting-notes` so signing identity,
verifier constants, TCC grants, and the app-data root are untouched. The
`.app` is `Yawn.app` and the image is `Yawn-<version>-macos-arm64.dmg`; the
preview and library-dev lanes deliberately keep their old product names, since
neither is distributed. The Cargo package — and therefore the signed main
executable, `Contents/MacOS/local-meeting-notes-desktop` — is also unchanged.

Its own record stands: built at commit `bad28f1` on
`codex/guided-voice-enrollment`; DMG SHA-256
`38e5aa7d5bc8e1a86577f29cec660dd7e71e70912aad50d796c278be5eb83289`, signed,
notarized, stapled, Gatekeeper-accepted for both the app and the image, and
`verify-signed-release.sh … internal-alpha` PASS. Read the exit-16 note under
"Two traps" before trusting any redirected run of that verifier.

**0.2.2's evidence does not transfer to 0.3.0.** 0.2.2 was the first build
carrying the admitted ONNX speaker encoder with guided voice setup registered
end to end, and its own record stands: commit `f63d38a`, DMG SHA-256
`eec3e611aceef12e932e870197fba26d612b02a9a8f94eb41f070a8f838c89f4`, signed,
notarized, stapled, `verify-signed-release.sh … internal-alpha` PASS. It never
received its interactive operator run, so that gate is still open and is now
0.3.0's to close, not a box 0.2.2 already ticked. Its R2 object was deleted
when 0.3.0 replaced it, because a differently-named image would otherwise have
kept serving an app called "Local Meeting Notes" from a live URL nothing linked.
The first cohort download still supplies the waived transferred-build
Gatekeeper field receipt — record it in `spike/encoder-packaging/RESULTS.md`
when it arrives.

**Publishing the page is a manual step, not a push.** The `yawn-site` Cloudflare
Pages project has no git integration — verified against the Cloudflare API on
2026-08-05, its `source` is null — so committing and pushing the delivery page
publishes nothing. The 0.2.1 and 0.2.2 page updates were both pushed and never
deployed for exactly this reason; the live page served 0.2.0-era copy until
0.3.0. Run `node scripts/render-release.mjs`, then
`npx wrangler pages deploy dist --project-name=yawn-site --branch=main`
from the site repo, then re-fetch with `Cache-Control: no-cache`, because the
edge keeps serving the previous HTML for a while after a deploy. The project also
has no custom domain: it is `yawn-site.pages.dev`, and `yawn.ninochavez.com`
never resolved.

Earlier record: a signed and notarized internal transcript-alpha DMG exists for
commit `5fe9aecd4f53204dc6e82573fd4b4dde37efd6d1`. Its SHA-256 is
`f5f091811d337acae4c7dc25db5638e2675e30552ed1793af1dc82c7c734385a`.
The frozen app and DMG pass independent signature, Gatekeeper, runtime, and DMG
layout verification. The main app and capture helper carry Apple's required
audio-input entitlement. A hardware attempt reached local transcription and
then failed closed because human-readable library output entered the worker's
JSON-only protocol channel. The corrected worker isolates protocol output, and
the packaged-runtime regression now invokes the real transcript model. The
corrected installed build then completed two-leg capture and local transcription
on real hardware, and the completed transcript screen returned after a true quit
and fresh launch. The unchanged installed alpha is byte-bound to the original capture
attempt. Its one-day deadline, `2026-08-02T10:20:01-0500`, passed; a launch after that
time completed automatic retention and produced an `audio-deletion/1` `removed`
receipt with SHA-256 `59a500cb4f6c5e05e22425ba2f90c38629d300249b41628f5efb613bb029f4d5`,
observed at approximately `2026-08-02T20:24:47-0500`. Both bound microphone and
system WAVs are absent. The exact retained transcript artifact remains present, JSON-readable, and
digest-matched; its text was not inspected or emitted. This closes automatic deletion
only. This artifact is not cleared for team distribution: clean transfer, Gatekeeper,
permissions, capture, and recovery on another Mac or genuinely clean account remain
open; PR #2 and the release remain draft, and no release verdict is recorded.

This is a how-to for the release operator. It assumes a clean release commit,
Apple-silicon macOS, Xcode command-line tools, and access to the existing
Developer ID and notarization credentials. It does not assume knowledge of the
project's implementation history.

## Two release lanes

Both lanes produce a versioned, signed, notarized DMG for Apple-silicon Macs
running macOS 14.4 or later. Installation is drag-to-Applications. Updates are
manual for the first release.

The first lane is an **internal alpha**. It is for capture, permission,
recovery, retention, transcript feedback, and review of the local generated
meeting note. It carries runtime admission `internal-alpha` and shows that label
in the application. Its generated note is explicitly reviewable output: it
links outcomes to source evidence and labels transcript excerpts as a fallback
when no summary is available. It requires manual Start and Stop, headphones,
one operator at the microphone, and nobody else in the room. It is not a beta
and does not satisfy the product admission gate.

The alpha lane's main window carries the reviewed internal-alpha surface
command set — capture, Library and exact search, voice status, guided
enrollment through building and publishing a profile, preserve-first migration,
confirmed reset, the one-meeting audio-deletion boundary, and restoring a
withheld turn — the same list the Preview window grants. Only `profile.adopt`
stays outside both; enrollment mutation was inside the boundary as of the
2026-08-05 gate work, which is what makes a profile installable on a shipped
DMG and therefore makes the gate reachable at all. This was decided 2026-08-04 after the
0.2.0 coworker-cohort DMG shipped with the shell's record entry and search
gated on the dev-only preview lane flag, so neither was reachable on any
machine while the mechanical release suite stayed green. Two pins now hold the
boundary: `main_window_has_only_named_commands_and_no_generic_capability`
freezes the exact list, and `shipped_shell_is_permitted_every_command_it_invokes`
fails any build whose shell invokes a command its window capability does not
grant. The first fixed cohort version is 0.2.1.

The second lane is the **internal beta** described by the accepted vertical
slice. It carries `product` runtime admission and remains blocked until the
private automatic-note admission receipt exists for the exact build and model
digests.

No signed package expands its stated operating envelope.

## What must be true before signing

Start from an exact commit with no untracked executable code. Build the fixed
runtime and the `.app`, then run:

```bash
scripts/verify-release-bundle.py \
  "target/release/bundle/macos/Yawn.app"
```

That command must report `PASS`. It checks the built permission text, bundle
identity, macOS version, arm64-only executables, arm64-compatible native libraries,
packaged Python, NumPy native code,
worker import, runtime inventory, and `admission: product`.

The current `worker/build_runtime.sh` intentionally produces
`admission: boundary-test`. Renaming that value is not a release step. Live
capture, transcription, note creation, a closed executable inventory, and the
fixed model set must work before the product-runtime builder may issue product
admission.

For the transcript-only lane, build the fixed offline runtime and verify the
explicit alpha admission:

```bash
worker/build_runtime.sh build-alpha
scripts/verify-release-bundle.py \
  --admission internal-alpha \
  "target/release/bundle/macos/Yawn.app"
```

`internal-alpha` is not an alias for `product`. The default verifier still
requires `product`, so an alpha can enter the signing path only through the
explicit alpha command.

## Preview-bundle lane (local, not a release)

**Written 2026-08-07, after building it three times to find out what it wanted.**
`scripts/prepare-preview-bundle.sh` and `apps/desktop`'s `preview-build` script are
real code that appeared in no document, so which runtime feeds them had to be
recovered from the script's own header.

This lane produces a locally signed `Yawn Preview.app`. It is **not
notarized, not stapled, and not a release.** Its purpose is narrow: §H's two
first-run permission paths cannot execute at all outside a signed bundle, because
running the probe's request modes from an unsigned binary mutates the calling
application's TCC state and answers about the wrong binary.

```bash
worker/build_runtime.sh build-alpha-external
cd apps/desktop && npm run preview-build     # tauri build + prepare-preview-bundle.sh sign
npm run preview-verify
```

**Stage `build-alpha-external`, the same runtime the release lane ships (§ "Build
and notarize", `DEPLOYMENT.md`).** Until 2026-09-02 this section said `build-alpha`,
which bundles Whisper and writes an `app-runtime/1` manifest with no model catalog.
A Preview built on it answers Settings with "this build does not use downloadable
speech models", so the model chooser the experience brief lists as a reviewed
surface never renders, and the screen review judges a state the shipped app never
shows (roadmap, D-MODEL). `prepare-preview-bundle.sh` already handles the
`app-runtime/2` manifest. Any `build-alpha*` mode satisfies the boundary below;
`build` does not.

**It needs a `build-alpha*` mode, not `build`, and the reason is a real boundary rather than
an oversight.** `prepare-preview-bundle.sh` hard-requires
`Contents/Resources/bin/meeting-capture`, and only `build-alpha*` stages it. The
default `build` lane is deliberately a *boundary* runtime: it stages
`permission-probe` in every mode — "a boundary build that cannot answer 'is the
microphone allowed' has the same lying surface the internal-alpha one would" — but
no recorder and no Whisper model. A Preview built on it fails the sign step with
`Preview meeting-capture helper is missing`.

### Two tracked files the build rewrites, and neither may be committed

**`apps/desktop/src-tauri/gen/schemas/capabilities.json` flips wholesale.** Building
the preview lane replaces the `main-window` capability with `preview-window`. It is a
one-line file, so the diff looks trivial and is not: committing it would put the
Preview capability on trunk in place of the shipped main window's. Revert it.

`npm install` also rewrites `apps/desktop/package-lock.json`. Incidental; revert it too.

    git checkout -- apps/desktop/src-tauri/gen/schemas/capabilities.json \
                    apps/desktop/package-lock.json

`git status` must read clean before committing anything from a session that built
this lane.

### Verify independently; the script's success is silent

`prepare-preview-bundle.sh verify` exits 0 and prints nothing, so a passing run and
a run that did nothing look identical. Check the properties directly:

```bash
APP="target/release/bundle/macos/Yawn Preview.app"
codesign -dvv "$APP" 2>&1 | grep -E '^Authority=Developer ID|^TeamIdentifier='
codesign -d --entitlements :- "$APP/Contents/Resources/bin/permission-probe" \
  | plutil -extract 'com\.apple\.security\.device\.audio-input' raw -o - -
```

Tauri ad-hoc signs during bundling — `Signing with identity "-"` plus a notarization
skip warning — and `prepare-preview-bundle.sh sign` re-signs afterwards with the real
identity and the capture entitlements. **The bundle Tauri leaves behind is not the
signed bundle**, and the two are indistinguishable without asking `codesign`.

### The 2026-08-07 build

First signed Preview bundle. Verified independently rather than by the script's exit
code:

| target | authority | audio-input |
|---|---|---|
| `Yawn Preview.app` | Developer ID Application: Abelino Chavez (34VZ63G58M) | — |
| `local-meeting-notes-desktop` | same | true |
| `meeting-capture` | same | true |
| `permission-probe` | same | true |

`admission: internal-alpha`, 2.1 GB. The capability grants
`allow-first-run-request-microphone` and `allow-first-run-request-system-audio`.

**Those two paths remain unrun.** They are macOS permission prompts; a person has to
be at the machine to grant or deny them, and no build advances that. Producing the
bundle was the builder-owned half and it is done.

### The alpha runtime carries the embedding model from 2026-08-08

`build-alpha` now stages `models/all-MiniLM-L6-v2` — 87 MB, four files at the
pinned revision `1110a243fdf4706b3f48f1d95db1a4f5529b4d41`, digest-checked on
download and again in `verify` — and installs `tokenizers==0.22.2` from
`worker/requirements-embedder.lock`, `--no-deps`, like `mlx-whisper` and
`onnxruntime`. The manifest's `models[]` grows from two entries to six and
`corpus.embed` joins `ALPHA_OPERATIONS`.

Against a 2.1 GB stage the model is inside the rounding, so the bundle size in the
2026-08-07 record above still reads the same to one decimal.

**Not a mode of its own, unlike the encoder below.** That lane exists because the
ECAPA encoder was a candidate under an admission check with alternatives. This
model is chosen and measured; what is unjudged about it is whether its retrieval
is useful, which no build mode settles. A second optional component would have
taken three modes to four and then to eight.

**Packaging is not admission.** `notes/SEMANTIC_RETRIEVAL.md` records 7 of 10 on a
realistic corpus, and 3 of 5 on the questions exact search cannot answer.

Verified on the 2026-08-08 build: `verify` ran 117 tests, `OK`, with **all 16
`test_embedding` tests executing rather than skipping** — `verify` names
`LMN_EMBEDDING_MODEL_DIR` for that reason, because a suite that skipped the only
tests touching the model would report the same green as one that ran them.

## Fixture-bundle lane (local rendered review, not Preview or a release)

This lane makes a disposable, privacy-safe data set visible in a signed local
app. It is for checking rendered library, meeting-detail, retry, quality, device,
decision, and playback states. It is not Preview: Preview tests the signed
permission surface. It is not production, notarization, installation, or
shipment.

The bundle and its data root are deliberately separate from every live app:

```text
app:  /Users/nino/Workspace/dev/apps/yawn-app/target/release/bundle/macos/Yawn Fixture.app
id:   com.ninochavez.local-meeting-notes.fixture
root: /Users/nino/Library/Application Support/com.ninochavez.local-meeting-notes.fixture
```

Build and verify from `apps/desktop`:

```bash
cd apps/desktop
npm run fixture-build
npm run fixture-verify
```

The build script uses `tauri.fixture.conf.json`, signs the resulting app with
the local Developer ID identity through `prepare-preview-bundle.sh`, and does
not notarize or package it. As with Preview, restore the generated capability
schema and lockfile if the build rewrites them before committing:

```bash
git checkout -- apps/desktop/src-tauri/gen/schemas/capabilities.json \
                apps/desktop/package-lock.json
```

### Seed the synthetic root

Build the `rendered-review-fixture` binary, then pass exactly one absolute path.
The final path component must be the fixture bundle identifier. The command
creates deterministic invented transcript and retry data, two eight-second
silent WAVs, and `SYNTHETIC_FIXTURE.json`; it does not create a generated note:

```bash
cargo run -p local-meeting-notes-session-core --bin rendered-review-fixture -- \
  "/Users/nino/Library/Application Support/com.ninochavez.local-meeting-notes.fixture"
```

The seed command refuses a relative path, the wrong final component, symlinks,
a broad parent, an existing root, a root inside the repository, and a directory
that is not private. The marker must remain the exact synthetic marker. Do not
copy real meeting material into this root.

### Archive one fixture before seeding the next state

There is no delete or in-place reset command. The bounded rotation command moves
an exact fixture to a recoverable sibling archive, then leaves the canonical root
absent for a fresh seed:

```bash
cargo run -p local-meeting-notes-session-core --bin rendered-review-fixture -- \
  archive \
  "/Users/nino/Library/Application Support/com.ninochavez.local-meeting-notes.fixture" \
  "/Users/nino/Library/Application Support/com.ninochavez.local-meeting-notes.fixture.archive-keep-current"
```

The archive destination must be absent, share the source root's canonical
parent, and end in `.archive-<label>`; the label is 1–64 ASCII letters, digits,
or hyphens. The command validates the exact synthetic markers, private modes,
and absence of symlinks, acquires the canonical app-data writer lock, repeats
the checks under that lock, and publishes the archive with an exclusive atomic
rename. It refuses a running writer and never deletes, copies, overwrites, or
automatically reseeds data. After it succeeds, run the seed and verified public
model-import steps again for the next isolated journey.

### Import one verified public transcript model

The source exposes the only model-import form as:

```bash
cargo run -p local-meeting-notes-session-core --bin rendered-review-fixture -- \
  install-model ROOT BUNDLE_RESOURCES SOURCE_MODEL_DIR MODEL_ID
```

For a real run, use the exact fixture root above, the signed app's Resources
directory, and a public model directory whose leaf is exactly
`<model-id>/<catalog-revision>`:

```bash
cargo run -p local-meeting-notes-session-core --bin rendered-review-fixture -- \
  install-model \
  "/Users/nino/Library/Application Support/com.ninochavez.local-meeting-notes.fixture" \
  "/Users/nino/Workspace/dev/apps/yawn-app/target/release/bundle/macos/Yawn Fixture.app/Contents/Resources" \
  "/absolute/path/<model-id>/<catalog-revision>" \
  "<model-id>"
```

Use only a public model that is already present in the verified bundle catalog.
The importer re-verifies the catalog and source bytes, copies only the catalog's
config and weights, then activates the model and writes `MODEL_FIXTURE.json`.
It refuses symlinked or non-directory inputs, repository-contained sources,
wrong model/revision leaves, existing model state, and source changes during the
bounded copy. The angle-bracket values above are placeholders, not model names
or revisions to invent; read the bundle's verified `model-catalog.json` first.

### Rendered receipt — 2026-08-17

Direct observation used the exact signed app at the path above, with bundle ID
`com.ninochavez.local-meeting-notes.fixture` and root
`/Users/nino/Library/Application Support/com.ninochavez.local-meeting-notes.fixture`.
The root held only the deterministic invented transcript/retry fixture, two
eight-second silent WAVs, and a verified public model. The rendered walk opened
the library and meeting detail, compared the pending retry, inspected recording
quality and device context, selected **Decide later**, and exercised active
playback with **Stop** before returning with **Back**. No settings, permissions,
recording, or private data were involved.

That first receipt did not show **Keep current**, **Use retry**, correction save
or regeneration, a generated note, a source link or post-promotion state, a
recovery toast, or an installed production app. It is not evidence of
notarization, installation, publication, shipment, or production behavior. Do
not describe it as a release artifact.

### Stateful retry receipt — 2026-08-17

The recoverable archive command preserved the first review root, then two fresh
deterministic roots exercised the explicit decisions. **Keep current** closed
the comparison, left the retained transcript selected, replaced **Review retry**
with **Retry transcript**, and survived Back and reopen. That root is preserved
as `.archive-keep-current-20260817`.

On the next fresh root, **Use retry** promoted the candidate and returned to
transcript-only detail with **Generate note** and **Retry transcript**. The
canonical fixture root preserves that promoted state. The app is closed.

This did not prove invalidation of an existing generated note because the
fixture had no note. The promotion toast also overclaimed that a previous note
was cleared. Fix and re-render that message before accepting the promotion
journey as complete. Back-and-reopen continuity after promotion remains open.
No private content, real meeting audio, Settings change, permission change, or
recording was involved.

## Encoder-candidate lane (admission evidence, not a release)

`worker/build_runtime.sh build-alpha-encoder` builds the alpha runtime plus
the preferred ONNX speaker-encoder candidate: onnxruntime from the hash-pinned
`worker/requirements-encoder.lock`, and the converted ECAPA model at
`models/speaker-encoder/ecapa-tdnn.onnx`, accepted only if it matches the
pinned digest of the deterministic export
(`spike/encoder-packaging/export_onnx.py` from the pinned checkpoint; two
independent exports reproduce the digest byte-identically). The manifest's
`encoder` entry then names that artifact and binds its digest; the
`encoder-unavailable.identity` file remains in the bundle as a fixed resource,
but the manifest — not that file — is what every consumer reads.

This lane exists to produce the evidence admission check 2 requires
(`spike/encoder-packaging/RESULTS.md`), on the real signing path:

1. Build and sign exactly as for the alpha lane; `verify-release-bundle.py`
   additionally re-derives the packaged encoder's digest against its own
   pinned constant and exercises onnxruntime inside the packaged Python —
   after signing, that exercise is the same empty-entitlement control that
   caught `llvmlite`, now covering onnxruntime's dylibs.
2. `scripts/verify-offline-coldload.sh "<app>"` — cold-loads the encoder
   under a deny-network sandbox profile, after first proving the profile
   actually refuses a socket connection.
3. `<Resources>/python-runtime/bin/python3.12 -E -s -B
   scripts/measure-encoder-beside-mlx.py <Resources>` — peak RSS with the
   encoder session co-resident with MLX transcription of synthetic audio.

Until 2026-08-04 a `build-alpha-encoder` bundle was admission evidence only,
never a distribution candidate. The operator's admission verdict
(`spike/encoder-packaging/RESULTS.md` § "Admission verdict") changed that:
**from 0.2.2 the cohort DMG is built with `build-alpha-encoder`**, shipping
the admitted ONNX encoder in the runtime. The transferred-build Gatekeeper
check was waived in that ruling; the first cohort download of an
encoder-carrying DMG supplies the field receipt and should be noted in
RESULTS.md when it arrives.

**Correction, 2026-08-30: the released DMG does not carry the encoder.** The
public `Yawn-0.5.9-macos-arm64.dmg` was downloaded from the landing site,
byte-count and SHA-256 matched the 0.5.9 receipt above, and its mounted
`app-runtime.json` names `encoder-unavailable.identity` with no
`models/speaker-encoder/` in the bundle. The installed 0.5.7 shows the same.
So the release lane has been building plain `build-alpha`, and the bolded
ruling above stopped describing the shipped artifact somewhere between 0.2.2
and 0.5.7 — the superseded DMGs were deleted per the receipts, so the exact
release where it decayed is not recoverable. What bounds the damage today:
no enrollment command is registered in the shipped app's `generate_handler!`
list, so a cohort operator cannot create a non-zero profile and the
placeholder gate cannot fire for them. The exposure is the developer path —
a profile adopted through dev tooling makes `transcript.create` refuse in
the installed placeholder app against the same data directory, which is
exactly the cost `docs/speaker-gate-slice.md` § "Fork 1" priced as landing
on nobody. Either the release lane returns to `build-alpha-encoder` before
enrollment registers, or the two claims stay corrected. Admission of the encoder is not admission of
enrolment: the recorder and profile operations stay unregistered until their
own operator decisions.

## Local Developer ID gate — signed, not released

Use this lane when the app needs the real Developer ID and hardened-runtime
shape for local verification, but no release has been authorized:

```bash
scripts/sign-notarize.sh local --admission internal-alpha \
  "target/release/bundle/macos/Yawn.app"
```

Use `--admission product` only for a product-admission runtime. The admission
is mandatory so the command cannot silently choose a wider runtime contract.

The lane runs the same nested signing, manifest refresh, outer signing, deep
signature check, and signed runtime verifier used before a release. It then
exits. It does not check the notary profile, submit to Apple for notarization,
staple, build or sign a DMG, run Gatekeeper, install, or replace an app. The
default target is the generated app under `target/release/bundle/macos`; do not
pass an installed app path unless modifying that exact installation is the
explicit job.

This lane is local in scope, not offline. `codesign --timestamp` contacts
Apple's timestamp authority. The completion line is deliberately bounded:

```text
DONE: locally signed and verified app (not notarized or packaged)
```

On 2026-08-17, the `internal-alpha` lane at commit `644aaaf` signed and verified
the current unreleased source build, version 0.5.8, at
`target/release/bundle/macos/Yawn.app`. The app source was built at commit
`97ff8c9`; this is not the released 0.5.8 artifact recorded at the top of this
runbook. The verifier confirmed 169 arm64-compatible Mach-O files, identifier
`com.ninochavez.local-meeting-notes`, Team `34VZ63G58M`, Developer ID authority,
and the hardened-runtime flag. The signed outer bundle's CDHash is
`5b878972e00a7a42657fda2abf06076f00b388eb`.

The recorded before/after comparison found the installed
`/Applications/Yawn.app` binary and the existing DMG unchanged by inode, size,
and modification time. This receipt is not evidence of notarization,
installation, publication, or shipment.

## Check Apple release access

Run the host-level preflight:

```bash
scripts/sign-notarize.sh preflight
```

It must find both:

- the Developer ID Application identity for Team `34VZ63G58M`;
- the Apple-accepted `filmroom-notary` keychain profile.

A restricted terminal process may see zero identities even when the release
keychain is present. Treat the host-level command as the release check. The
preflight reads credential state; it does not print or copy credential
material.

## Sign and notarize the exact app

Run:

```bash
scripts/sign-notarize.sh run \
  "target/release/bundle/macos/Yawn.app"
```

For the transcript-only lane, use the explicit command:

```bash
scripts/sign-notarize.sh run-alpha \
  "target/release/bundle/macos/Yawn.app"
```

The script follows the same two-submission sequence used by Film Room:

1. Refuse a runtime whose admission does not match the selected lane before
   changing the app.
2. Sign every nested Mach-O with Developer ID, hardened runtime, and a secure
   timestamp.
3. Rebuild the alpha runtime manifest from those exact signed bytes.
4. Sign and strictly verify the outer app.
5. Exercise the signed packaged runtime with the closed entitlement allowlist.
6. Submit the app to Apple, staple it, and pass Gatekeeper.
7. Build and verify the drag-to-Applications DMG.
8. Sign, submit, staple, and Gatekeeper-check the DMG.
9. Recheck the frozen app and DMG without relying on release credentials.

The preserved empty-entitlement control failed when `llvmlite` changed an
allocated page to executable memory. Re-signing only the packaged `python3.12`
with `com.apple.security.cs.allow-unsigned-executable-memory` made the identical
offline-runtime import pass. That one key is the closed allowlist. The Tauri
executable, Swift audio tap, libraries, and every other packaged executable
remain entitlement-free; this app does not carry Film Room's library-validation
exception.

The output name is derived from the built app version:

```text
target/release/bundle/macos/Yawn-<version>-macos-arm64.dmg
```

### Traps that have cost a lane run

**Never pipe this script into `tail`, `head`, or anything else.** A pipeline
reports the *last* command's exit status, so `sign-notarize.sh … | tail -40`
returns 0 even when the lane aborts. On 2026-08-05 the lane died at
`verify-dmg-layout.sh` and reported success; the app was notarized and stapled
but the DMG was never signed, notarized, or stapled, and only reading the log
text revealed it. Redirect to a file and check `$?` instead.

**`hdiutil: attach failed - Resource temporarily unavailable` means the image is
already claimed, not that it is corrupt.** A previous failed or interrupted
attach can leave the DMG half-attached with no mountpoint, and every later
attach fails against it. It is invisible in `mount`; look in `hdiutil info` for
the image path, then `hdiutil detach <device> -force`. Do not rebuild the DMG or
re-sign anything to "fix" this — nothing is wrong with the artifact.

**A bare exit 16 from `verify-signed-release.sh` is not a verdict on the
artifact — reproduce it interactively before believing it.** On the 0.3.0 run
the script exited 16 with no diagnostic every time its output was redirected to
a file, and passed completely — DMG layout, signed bundle, every component
hash, `signed release verification: PASS` — when run so its output reached a
terminal or a pipe, including under `bash -x`. The cause of the redirect-only
failure is still unexplained and is recorded here as unexplained rather than
guessed at; two hypotheses (a stale mount, then a race with `spctl`'s own
Gatekeeper mount) were tested and both were wrong. What settles the artifact is
a full PASS from a traced run plus Apple's own `Accepted` for the app and the
DMG, not the exit status of a redirected one. Never publish on a bare non-zero
exit either — find out which it is.

**Give the verifier a pty and keep the log — the two are not in conflict.** The
trap above reads as a choice between a record and a pass. It is not. `script`
allocates a real terminal for the child and writes everything to a file:

```bash
script -q verify.log scripts/verify-signed-release.sh \
  "target/release/bundle/macos/Yawn.app" \
  "target/release/bundle/macos/Yawn-<version>-macos-arm64.dmg" \
  internal-alpha
```

On 0.4.0 that returned exit 0 with the full `signed release verification: PASS`.
Prefer this to a bare interactive run; there is no reason to publish off a
verdict nobody wrote down.

**A zero-byte `$DMG.sha256` mid-lane is `tee` still writing, not the kill
below.** Read on 2026-08-08 seconds before the lane finished, and reported as a
timeout kill on the strength of the paragraph that follows. It was not: the lane
exited 0 and the file held the digest a moment later. Check `LANE_EXIT` — or the
absence of it — before concluding anything from an empty checksum file.

**A lane killed after stapling leaves complete artifacts and no checksum.** The
0.4.0 run was killed by an agent harness's own ten-minute background timeout
while `verify-signed-release.sh` was running as step 9 — `Terminated: 15`, not a
failure of anything the lane did. Both notarizations were Accepted, both staples
applied, both Gatekeeper checks passed, and the DMG layout verified before the
kill. What was missing was `$DMG.sha256`, because `shasum -a 256 "$DMG" | tee
"$DMG.sha256"` is the script's *last* line and sits after the verifier.

So: a lane that dies at step 9 needs the verifier and that one `shasum` line
re-run, and nothing else. Do not rebuild, do not re-sign, and do not re-notarize
— that would replace an Apple-accepted artifact with an unverified one to fix a
missing text file. Set the timeout past the lane's real length instead; this one
took roughly eighteen minutes.

## Recheck a frozen artifact

Anyone with the artifact and Apple command-line tools can run:

```bash
scripts/verify-signed-release.sh \
  "target/release/bundle/macos/Yawn.app" \
  "target/release/bundle/macos/Yawn-<version>-macos-arm64.dmg" \
  internal-alpha
```

This checks strict signatures, staples, Gatekeeper, the mounted DMG layout, the
runtime, every Mach-O signing authority, the closed entitlement allowlist, and
the release hashes. Omit the final `internal-alpha` argument only for a product
release.

## Prove installation on another Mac

Use an Apple-silicon Mac or genuinely clean macOS account with no source
checkout, Homebrew, prior app data, prior permission grants, or external Python
or model runtime.

Record only release evidence:

- the exact commit, app version, and DMG SHA-256;
- transfer method and whether quarantine remained intact;
- Gatekeeper opening without a manual override;
- drag-to-Applications success;
- microphone and system-audio prompts attributed to Local Meeting Notes;
- clean refusal after denying either permission;
- successful two-leg Start and Stop after granting permission;
- quit, reopen, and recovery results;
- retention and deletion results;
- absence of an external runtime or unexpected network dependency.

Do not put meeting audio, transcript text, notes, voice profiles, Apple
credentials, or private review packets in a Git receipt.

### Content-free closure receipt

Keep the completed receipt outside Git beside the frozen artifact. It records only
release identity and outcomes; it must not contain a meeting identifier, app-data
path, audio, transcript text, note text, profile material, or credential material.
An agent may verify this shape and the public hashes. It may not invent the observed
outcomes or the release verdict.

```json
{
  "schema": "transcript-alpha-release-receipt/1",
  "recorded_at": "<RFC 3339>",
  "release": {
    "commit": "5fe9aecd4f53204dc6e82573fd4b4dde37efd6d1",
    "app_version": "0.1.0",
    "admission": "internal-alpha",
    "dmg_sha256": "f5f091811d337acae4c7dc25db5638e2675e30552ed1793af1dc82c7c734385a"
  },
  "automatic_deletion": {
    "observed_at": "<RFC 3339>",
    "installed_build_unchanged": true,
    "receipt_schema": "audio-deletion/1",
    "receipt_sha256": "<sha256>",
    "microphone_audio_absent": true,
    "system_audio_absent": true,
    "transcript_retained": true
  },
  "consented_hardware_run": {
    "participant_attestation_recorded": true,
    "headphones_attested": true,
    "operator_alone_attested": true,
    "two_leg_start_stop_succeeded": true,
    "post_meeting_transcript_created": true,
    "true_quit_fresh_launch_recovered_transcript": true,
    "transcript_text_inspected_for_receipt": false
  },
  "clean_transfer": {
    "target": "another-mac-or-clean-account",
    "quarantine_intact": true,
    "gatekeeper_opened_without_override": true,
    "drag_to_applications_succeeded": true,
    "prompts_attributed_to_app": true,
    "permission_denials_failed_closed": true,
    "two_leg_start_stop_succeeded": true,
    "post_meeting_transcript_created": true,
    "true_quit_fresh_launch_recovered_transcript": true,
    "retention_deletion_succeeded": true,
    "external_runtime_or_network_required": false
  },
  "operator_release_verdict": "accept-or-decline"
}
```

The deletion observation belongs to the original unchanged installed run. The clean
transfer may use the same exact DMG on the separate target. Rebuilding, reinstalling,
changing retention, manually deleting audio, or removing quarantine does not close
either gate; it starts a new evidence chain or records a failure.

## Human release gates

Signing and notarization prove package identity and Apple trust. They do not
prove that the notes are useful.

Before an internal alpha is shared, the unchanged installed build needs a
consented hardware receipt proving two-leg Start and Stop, post-meeting
transcript creation, quit/reopen recovery, and the configured audio deletion.
The current build has passed capture, transcript creation, and fresh-process
recovery. Its automatic deletion is mechanically closed: the unchanged executable is
byte-bound to the original capture attempt; after the `2026-08-02T10:20:01-0500`
deadline, a post-deadline launch produced the completed `audio-deletion/1` `removed`
receipt SHA-256 `59a500cb4f6c5e05e22425ba2f90c38629d300249b41628f5efb613bb029f4d5`,
observed at approximately `2026-08-02T20:24:47-0500`. Both bound microphone and
system WAVs are absent, and the exact retained transcript artifact is present, JSON-readable, and
digest-matched; its text was not inspected or emitted. Clean-machine transfer remains
open. This receipt is mechanical evidence only and is not a release verdict.

Before an internal beta meeting, the unchanged installed build must also have
the private automatic-note admission receipt required by the release policy. The operator records that receipt against the exact
build and model digests. The private canary and its content stay outside Git.

The first release may use manual replacement. Before distributing a second
version, prove upgrade from the immediately previous app-data schema, injected
migration failure, and rollback without destroying the only copy of a meeting.
