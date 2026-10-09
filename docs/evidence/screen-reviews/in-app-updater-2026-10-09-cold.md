# In-app updates (Settings › Updates, the main-window notice) — cold review, 2026-10-09

**Kind:** cold screen review, two passes, each by a fresh subagent. Each reviewer saw only unlabeled frames and five neutral job questions. Neither had source code, rationale, or the other pass. Each was told only that the frames show a Mac app for recording meetings and keeping private notes, which Settings situations the frames cover, and that all data is synthetic. The frames come from the browser review harness (`ui/review/settings-harness.html?update=<state>` and `ui/review/harness.html?update-notice=<version>`), which renders the shipped Settings and main-window frontend against a stubbed backend. They are not captures of an installed app, and no update was downloaded or installed to make them.

**Question:** Can a person tell what is happening with new versions of the app, what they can do and why not when they can't, and what the update check means for their privacy? Does anything contradict anything else?

**Before:** Yawn had no update path. A new version meant a manual download from the landing page, and the app never contacted a server unless the operator asked for a speech-model download.

| Pass | Frames | Verdict | Main findings |
|---|---|---|---|
| 1 | 8: Settings available (dark, light), blocked, downloading, failed check, automatic off; main-window notice (dark, light) | revise | (a) After a failed check, the "Not updated" pill reads as "the update failed". (b) The once-a-day sentence still showed under the switch with checks turned off. (c) While downloading, the controls are disabled with no reason given, and nothing says whether Yawn restarts. (d) The privacy row says "Nothing", then names an exception. (e) "The request carries only Yawn's version number" is an absolute claim the reader cannot check |
| 2 | 6: Settings available, blocked, downloading, failed check, automatic off; main-window notice | pass | The update and privacy story is understandable and honest; the internet-address disclosure is stated plainly. Polish: say what happens if a recording starts during the download; with checks off, "its only automatic connection" reads slightly stale; the notice's "Update…" differs from Settings' "Update and restart" |

**Pass 1, changed:**
- (a) The pill reads "Check failed", or "Not installed" when a found update failed to download or install. The detail adds "You’re still on Yawn <current>."
- (b) The disclosure follows the switch. Off reads: "Automatic checks are off. Yawn contacts the release server only when you choose Check now." The row's idle detail no longer repeats it.
- (c) The download detail says "When it finishes, Yawn closes and reopens."
- (d) The privacy row opens "Nothing from your meetings."
- (e) Checking the claim against the plugin source found it was untrue. The feed URL carries no version placeholder. The check is a plain GET with an `Accept` header and the plugin's own user agent (`tauri-plugin-updater/2.12.0`), so no Yawn version is sent at all. The disclosure now says what is true: "Once a day, Yawn downloads a small file from the release server that names the newest version. It sends no meeting data or account details; like any download, the server can see this Mac’s internet address."

**Pass 2, changed after the pass:**
- The download detail adds "If a recording is running by then, Yawn keeps recording and doesn’t install." The install step holds the command lock, re-checks for a recording, and refuses with "The update was downloaded but not installed. Finish the current recording first." A recording started mid-download is therefore never interrupted.
- The privacy row says "the optional update check".

Frame: `../in-app-updater-2026-10-09/final-settings-downloading-dark.png`.

**Raised but not changed, with reasons:**
- **The notice's "Update…".** The button opens Settings; it does not restart anything. "Update and restart…" would promise an immediate restart. The ellipsis already signals a further step, and pass 1's reviewer expected Settings, which is where it goes.
- **No way to cancel a download.** A real gap, but a feature of its own. A download now ends either in an install or in a refusal that leaves the recording alone.
- **The toast covers content at the bottom right.** Every Yawn notice behaves this way.
- **Where the saved AI drafts were made.** A question about the retired note-generation feature, outside this change.

**What was removed, combined, demoted or hidden:** nothing was removed. One sentence of existing copy changed: the privacy row's "Nothing." became "Nothing from your meetings.", with one sentence added about the optional update check. A new Settings group, "Updates", sits between Speakers and Storage, with a matching navigation link. The main window gains a notice that shows only when no other notice or error is up, so it never stacks. The app menu gains "Check for Updates…" below About.

Frames: `../in-app-updater-2026-10-09/pass-1/` and `../in-app-updater-2026-10-09/pass-2/`.

Checks:
- `npm run test:ui`: 164 pass. These include the update presentation and notice tests, the Settings interaction test rendering the Updates row from `update_status`, and the pinned navigation list.
- `cargo test -p local-meeting-notes-desktop`: passes. That includes 10 updater tests: the install gate and its check-before-claim order (a defect the post-commit review found: checking after the claim refused every install), the automatic-check flag, a relaunch that runs only after the old process exits, the shipped `plugins.updater` config parsing as the plugin's own `Config`, and `scripts/prepare-update-feed.py` output parsing as the plugin's own `RemoteRelease`. It also includes the shell-contract tests, which now pin the four new Settings commands and permissions.
- `examples/verify_update_signature.rs` passes a package signed with the 1Password key and fails the same package with one byte appended.

- `examples/unpack_like_updater.rs` unpacks with the updater's own archive crates and path handling. On a tarball of the notarized 0.6.12 bundle (21,943 entries), strict code-signature verification, the stapled ticket and Gatekeeper all pass on the result, and its root comes out at mode 755. With one bundled file altered, code-signature verification fails.

Known limit, not on screen: standalone `tauri signer sign` writes no version into the signature, so `requireSignedVersion` stays off. Someone who controls the feed (or a test override of it) could offer an older genuinely signed Yawn under a higher version number. They could not install anything unsigned.

This review does not establish the install path on a real Mac. That needs two updater-capable signed builds and a test feed. `DEPLOYMENT.md`, "Prove the first update end to end", makes that a gate before the first real feed is published.
