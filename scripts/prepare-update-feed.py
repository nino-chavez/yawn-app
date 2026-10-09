#!/usr/bin/env python3
"""Write the in-app updater's feed, latest.json, for one release.

tauri-plugin-updater reads `version`, `notes`, `pub_date` and, for this Mac,
`platforms["darwin-aarch64"]` = {url, signature}. The signature field is the
text of the package's .sig file, produced by scripts/build-update-artifact.sh.

The package lives at an immutable key beside the feed:

    <base-url>/updates/Yawn-<version>-macos-arm64.app.tar.gz
    <base-url>/updates/latest.json

This only writes the file. Publishing it, last and with no-cache, after the
package is reachable, is DEPLOYMENT.md's job.
"""

from __future__ import annotations

import argparse
import base64
import datetime as dt
import json
import pathlib
import re
import sys

PACKAGE_NAME = re.compile(r"Yawn-(\d+\.\d+\.\d+)-macos-arm64\.app\.tar\.gz")


def fail(message: str) -> None:
    print(f"prepare-update-feed: {message}", file=sys.stderr)
    sys.exit(1)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--package", required=True, type=pathlib.Path,
                        help="the signed Yawn-<version>-macos-arm64.app.tar.gz")
    parser.add_argument("--base-url", required=True,
                        help="public bucket origin, e.g. https://pub-….r2.dev")
    parser.add_argument("--notes-file", type=pathlib.Path,
                        help="plain-text release notes shown before the install")
    parser.add_argument("--prefix", default="updates",
                        help="bucket folder for the package; updates-test only for the first end-to-end test")
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args()

    match = PACKAGE_NAME.fullmatch(args.package.name)
    if not match:
        fail("the package must be named Yawn-<x.y.z>-macos-arm64.app.tar.gz")
    if not args.package.is_file():
        fail(f"no package at {args.package}")
    signature_path = args.package.with_name(args.package.name + ".sig")
    if not signature_path.is_file():
        fail(f"no signature at {signature_path}; run scripts/build-update-artifact.sh")
    signature = signature_path.read_text(encoding="utf-8").strip()
    try:
        decoded = base64.b64decode(signature, validate=True).decode("utf-8")
    except Exception:  # noqa: BLE001 - any decode failure means the same thing
        fail("the signature file is not a base64 minisign signature")
    if "trusted comment:" not in decoded:
        fail("the signature file is not a minisign signature")

    base = args.base_url.rstrip("/")
    if not base.startswith("https://"):
        fail("the base URL must be HTTPS; the updater refuses anything else")
    notes = args.notes_file.read_text(encoding="utf-8").strip() if args.notes_file else None

    feed = {
        "version": match.group(1),
        "notes": notes,
        "pub_date": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "platforms": {
            "darwin-aarch64": {
                "url": f"{base}/{args.prefix.strip('/')}/{args.package.name}",
                "signature": signature,
            }
        },
    }
    if notes is None:
        del feed["notes"]
    args.output.write_text(json.dumps(feed, indent=2) + "\n", encoding="utf-8")
    print(f"prepare-update-feed: wrote {args.output} for {feed['version']}")


if __name__ == "__main__":
    main()
