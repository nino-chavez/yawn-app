# Deploying Yawn

This is the short, safe release path for an agent or release operator. Use it
when changing the macOS app, installing a new build, publishing its installer,
or updating the Yawn landing page.

The detailed record lives in [docs/distribution-runbook.md](./docs/distribution-runbook.md).
That document explains the native packaging choices and keeps historical
receipts. This guide is the how-to. It deliberately does not name a current
version: inspect the built artifact and public URL instead.

## What counts as released

There are four separate states:

1. The source commit is on the intended Git branch.
2. The exact app and DMG are signed, notarized, stapled, and locally verified.
3. The DMG is reachable from the versioned public R2 URL.
4. The live landing page links to that verified URL and checksum.

Do not call a build released until all four are true. A successful local build,
an Apple `Accepted` result, or a pushed landing-page commit is not enough on its
own.

## Scope and stop conditions

Yawn's distributable lane is `internal-alpha`. It includes the reviewable local
meeting note, but that generated note remains a draft with source links and an
explicit incomplete-output fallback. Do not change its runtime admission to
`product` as a packaging shortcut.

Stop and report the blocker instead of improvising when any of these is true:

- the app source is dirty or the version values disagree;
- the packaging lane refuses the staged runtime as stale or unstamped — the
  source-freshness gate (`worker/source_digest.py`) means the staging predates
  the sources; re-run `worker/build_runtime.sh`, never bypass the check;
- the runtime build cannot find the pinned local embedding-model assets;
- either hosted transcript model is missing, changed, or unreachable at its
  immutable catalog URL;
- the host cannot see the Developer ID identity or `filmroom-notary` profile;
- the signed verifier or Gatekeeper rejects the app or DMG;
- a suitable R2 S3 credential is unavailable for the large DMG upload; or
- GitHub authentication is invalid, so a branch cannot be pushed or merged.

Never put Apple credentials, Cloudflare credentials, meeting audio, transcript
text, notes, or user data in Git or deployment logs.

## Build and notarize the internal-alpha app

Start at the repository root:

```sh
git status --short
worker/build_runtime.sh build-alpha-external
(cd apps/desktop && npm run build)
scripts/verify-release-bundle.py \
  --admission internal-alpha \
  target/release/bundle/macos/Yawn.app
scripts/sign-notarize.sh preflight
scripts/sign-notarize.sh run-alpha target/release/bundle/macos/Yawn.app
```

Run the signed-bundle check and signing lane in a native macOS terminal context.
MLX needs this Mac's Metal device; a restricted shell can falsely fail its
import check. Keep the signing lane attached as one process. In Codex, do not
wrap it with `/usr/bin/script`: that wrapper can detach the child, which makes a
second signing attempt dangerous.

The lane creates these exact artifacts:

```text
target/release/bundle/macos/Yawn.app
target/release/bundle/macos/Yawn-<version>-macos-arm64.dmg
target/release/bundle/macos/Yawn-<version>-macos-arm64.dmg.sha256
```

Before delivery, independently run:

```sh
scripts/verify-signed-release.sh \
  target/release/bundle/macos/Yawn.app \
  target/release/bundle/macos/Yawn-<version>-macos-arm64.dmg \
  internal-alpha
shasum -a 256 target/release/bundle/macos/Yawn-<version>-macos-arm64.dmg
```

The checksum used on the landing page must come from the completed `.sha256`
sidecar or this final command. Never retype or reuse a previous release digest.

## Stage and publish the two transcript models

The DMG does not carry Whisper weights. Fresh setup uses Apple speech when its
language files are ready on a supported Mac. Otherwise, setup offers an explicit
Apple language-file download or the 464 MB Q4 speech model. The 1.61 GB full Turbo
model remains available in Settings. Existing downloaded-model choices survive
startup. Downloaded models go to Yawn's private Application Support directory
and are refused unless every byte matches the catalog signed inside the app.
The app still supports macOS 14.4; the Apple speech path requires macOS 26.

Stage the exact immutable object keys before building the release:

```sh
scripts/prepare-model-hosting.py \
  --q4-dir <pinned-q4-snapshot> \
  --full-dir <pinned-full-turbo-snapshot> \
  --output <new-empty-staging-directory>
```

Upload the four files listed in `hosting-manifest.json` to `yawn-releases` under
their exact `models/<model-id>/<revision>/<filename>` keys. Use
`application/octet-stream` and immutable cache control. Do not upload the
manifest itself as an app authority; the catalog sealed in `Yawn.app` is the
authority.

Before cutting the DMG, verify all four public URLs return the recorded byte
counts and perform one full downloaded SHA-256 check of each object. A HEAD or
range request proves reachability, not content identity. Do not release a build
whose model URLs have not passed both checks.

## Install locally without mixing bundles

Install from the signed DMG, never from the build directory. Mount the DMG,
copy `Yawn.app` to a new staging name in `/Applications`, verify its version and
signature, then replace `/Applications/Yawn.app`. Keep the previous bundle only
until the replacement reports the expected version and passes a strict code
signature check.

Do not merge a new app into an existing app bundle. A staged replacement avoids
leaving stale signed files behind. Quit Yawn before the swap. Verify the final
bundle with:

```sh
plutil -extract CFBundleShortVersionString raw /Applications/Yawn.app/Contents/Info.plist
codesign --verify --deep --strict --verbose=2 /Applications/Yawn.app
```

## Publish the installer

The public bucket is `yawn-releases`. Versioned public objects use this URL:

```text
https://pub-91cec3695eaf486bbfaaa114df6f2268.r2.dev/Yawn-<version>-macos-arm64.dmg
```

Older DMGs containing the full model were about 1.7 GiB. `npx wrangler r2 object
put --remote` has a 300 MiB remote-upload limit, so it must not be used to
publish a release DMG or either model weight file.
Use a multipart-capable S3 client such as `rclone` or the AWS CLI with an
existing R2 **Object Read & Write** access-key pair scoped to `yawn-releases`.
An ordinary Cloudflare API token is not an S3 access-key pair and must not be
passed to an S3 client.

Keep credentials in 1Password. Reuse an existing scoped R2 credential when it
exists. If it does not exist, stop and obtain explicit authority to create one;
do not silently mint a broad, long-lived credential during a release.

Upload the exact completed DMG under its versioned filename with content type
`application/x-apple-diskimage` and immutable cache control. With `rclone`,
include `--metadata` as well as `--metadata-set` so those headers are applied.
Then verify the public URL returns `200` and the expected `Content-Length` before editing the
landing page. A byte range request is enough to prove public reachability; do
not download the full image merely to test the link.

## Update and deploy the landing page

The site source is `/Users/nino/Workspace/dev/sites/ventures/yawn-site`. Its
`release.json` owns the version, download URL, SHA-256, and release notes shown
to users. Update them together only after the new R2 object is public. Run
`node scripts/render-release.mjs` to generate the homepage, app card, and
release-notes page. It also builds `dist/` with only those public artifacts.

Commit the site source, then deploy it manually:

```sh
cd /Users/nino/Workspace/dev/sites/ventures/yawn-site
node scripts/render-release.mjs
node --test scripts/render-release.test.mjs
npx wrangler pages deploy dist --project-name=yawn-site --branch=main
```

This Pages project has no Git integration. A site commit or push does not deploy
the public page. Re-fetch the exact live page with `Cache-Control: no-cache` and
confirm it names the new version, URL, and checksum. Check the release-notes
page and app card on the returned deployment URL and the production URL.
The public entry point is `https://apps.ninochavez.co/yawn/`; verify its
`release-notes/` link as well. The router serves the Pages source under that
prefix, so internal links should remain root-relative.

Only after both the new R2 URL and live landing page are verified may the
previous installer and its matching checksum sidecar be deleted. Confirm the
exact old key first. Never delete an old installer before its replacement has a
working public link.

## Push and hand off source

Check the configured Git remote before pushing. Check `gh auth status` before
creating or merging a pull request with the GitHub CLI. If either authentication
path is invalid, do not claim the branch was pushed, the pull request exists, or
`main` contains the fix. Restore the needed authentication, push the reviewed
branch, and merge the exact source commit that produced the artifact. Keep
release-document updates in a separate, clearly labeled commit when they are
recorded after the artifact is made.

## Prepare and publish release notes

Use an annotated `vX.Y.Z` tag for a publicly released app. The tag points to
the exact source commit used to build the signed artifact. An optional
`build/vX.Y.Z-rc.N` tag identifies a candidate; it does not mean the app is
public. Never move an existing release tag to different source.

Before building, collect the changes since the previous published tag:

```sh
python3 scripts/prepare-release-notes.py \
  --from v0.6.3 --to HEAD --version 0.6.4 \
  --output .artifacts/release-0.6.4/commits.json
```

Use that complete commit record and the relevant diffs to draft release notes
for someone deciding whether to update. State the main change first. Group
user-visible additions, improvements, and fixes; include upgrade requirements
and known limitations where needed. Keep internal work out of the reader's
notes. Commit messages and generated drafts require review before publication.

The landing site's release data owns the published copy. Its
`scripts/render-release.mjs` renders the dedicated release-notes page and the
latest summary. Preserve earlier release entries. Publish the version, date,
source tag, and comparison to the previous release with the reviewed notes.

After the signed installer is public and verified, create and push the release
tag at the recorded build commit. Publish the reviewed notes with that tag as
a GitHub release, then deploy the site. Include the installer URL and checksum;
do not attach a different locally rebuilt artifact. Historical tags may be
backfilled only when a recorded release receipt identifies the build commit.

## Final receipt

Record only these delivery facts: source commit, app version, DMG filename and
SHA-256, Apple app/DMG acceptance, installed-app version, public download URL,
landing-page deployment result, and retired object keys. The human hardware
test remains separate from packaging evidence.
