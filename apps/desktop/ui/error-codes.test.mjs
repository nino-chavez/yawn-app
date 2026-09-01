// W7-B (2026-09-01 desktop audit). This is the JS half of the cross-language
// drift check described in error_codes.rs: both sides load
// ../src-tauri/error-codes.json rather than restating its contents, so a
// code dropped from either the JSON registry or this file's code lists
// fails a test instead of silently drifting apart.
//
// The Rust half (error_codes::tests::error_code_drift) asserts every code in
// error-codes.json has a matching Rust constant, and every Rust constant is
// listed in the JSON. This file asserts the JSON's code set is exactly the
// union of the three code lists errorRecoveryPresentation actually reads.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  LIBRARY_UNAVAILABLE_CODES,
  SELECTED_MEETING_RECOVERY_CODES,
  VIEW_STALE_CODE,
} from "./view-model.mjs";

async function registryCodes() {
  const raw = await readFile(new URL("../src-tauri/error-codes.json", import.meta.url), "utf8");
  const entries = JSON.parse(raw);
  assert.ok(Array.isArray(entries) && entries.length > 0, "error-codes.json must be a non-empty array");
  return new Set(entries.map((entry) => entry.code));
}

function jsCodes() {
  return new Set([VIEW_STALE_CODE, ...LIBRARY_UNAVAILABLE_CODES, ...SELECTED_MEETING_RECOVERY_CODES]);
}

test("every code in error-codes.json is recognized by the JS recovery map, and vice versa", async () => {
  const fromJson = await registryCodes();
  const fromJs = jsCodes();

  const missingFromJs = [...fromJson].filter((code) => !fromJs.has(code));
  assert.deepEqual(
    missingFromJs,
    [],
    `codes in error-codes.json but missing from view-model.mjs's code lists: ${missingFromJs.join(", ")}`,
  );

  const missingFromJson = [...fromJs].filter((code) => !fromJson.has(code));
  assert.deepEqual(
    missingFromJson,
    [],
    `codes in view-model.mjs's code lists but missing from error-codes.json: ${missingFromJson.join(", ")}`,
  );
});

test("the three JS code lists carry no duplicate or empty entries", () => {
  const combined = [VIEW_STALE_CODE, ...LIBRARY_UNAVAILABLE_CODES, ...SELECTED_MEETING_RECOVERY_CODES];
  assert.ok(combined.every((code) => typeof code === "string" && code.length > 0));
  assert.equal(combined.length, new Set(combined).size, "a code appears in more than one list");
});
