import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

// The per-binary entitlement contract for macOS: only orbyn-engine-host is signed with
// engine-host/entitlements.plist, and the app and the other sidecars keep the default hardened runtime with
// no entitlements. The signing scripts themselves need a Mac and real codesign, so sign-macos-release.test.mjs
// and check-macos-entitlements.test.mjs skip everywhere else; these read the shipped files, so a plist, a
// config or a release check that drifts away from the contract fails on Linux and Windows too, where the
// release job never runs.
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
// A Windows checkout may write these files with CRLF; the assertions below are about their contents.
const read = (...parts) => readFileSync(join(root, ...parts), "utf8").replaceAll("\r\n", "\n");

// The engine host's set, from engine-host/README.md: V8 needs the first two to compile and run its JIT, and
// the third lets the Node runtime it injects load code that was not signed with the same team.
const JIT = "com.apple.security.cs.allow-jit";
const MEMORY = "com.apple.security.cs.allow-unsigned-executable-memory";
const LIBRARY = "com.apple.security.cs.disable-library-validation";
const HOST_KEYS = [JIT, MEMORY, LIBRARY].sort();

/** The plist's keys and boolean values, read from its text: enough for this file, with no parser dependency. */
function plistBooleans(...parts) {
  const text = read(...parts);
  const keys = [...text.matchAll(/<key>([^<]+)<\/key>/g)].map((match) => match[1]);
  const entries = Object.fromEntries(
    [...text.matchAll(/<key>([^<]+)<\/key>\s*<(true|false)\s*\/>/g)].map((match) => [match[1], match[2] === "true"]),
  );
  assert.deepEqual(Object.keys(entries), keys, "every entitlement in the plist must be a boolean");
  return entries;
}

test("the host's entitlements are exactly its three keys, all true", () => {
  const entries = plistBooleans("engine-host", "entitlements.plist");
  assert.deepEqual(Object.keys(entries).sort(), HOST_KEYS);
  for (const [key, value] of Object.entries(entries)) assert.equal(value, true, key);
});

test("the app declares no bundle-wide entitlements file and keeps the hardened runtime", () => {
  const config = JSON.parse(read("desktop", "src-tauri", "tauri.conf.json"));
  // Tauri applies bundle.macOS.entitlements to the app and to every sidecar, so a file there would give the
  // host's three keys to orbynd, orbyn and the shell as well. The release signer adds them per binary instead.
  assert.equal(
    config.bundle.macOS.entitlements,
    undefined,
    "bundle.macOS.entitlements is set again, so the app and every sidecar would inherit the host's entitlements",
  );
  assert.equal(config.bundle.macOS.hardenedRuntime, true, "the app keeps the default hardened runtime");
});

test("the release signer gives that plist to the host and to no other binary", () => {
  const signer = read("scripts", "sign-macos-release.sh");
  const entitled = signer.split("\n").filter((line) => line.includes("--entitlements"));
  assert.equal(entitled.length, 1, "exactly one binary is signed with an entitlements file");
  // The whole command, so a line that names the plist without signing with it does not pass.
  const [, reference] =
    entitled[0].match(/^\s*sign_file --entitlements "\$\(dirname "\$0"\)\/([^"]+)" "\$host"$/) ?? [];
  assert.ok(reference, `the plist goes to the host in a sign_file call, not: ${entitled[0]}`);
  assert.equal(reference, "../engine-host/entitlements.plist", "the signer names the host's plist");
  assert.doesNotThrow(() => read("scripts", reference), "the plist the signer names exists");
  assert.match(
    signer,
    /^host="\$app\/Contents\/MacOS\/orbyn-engine-host"$/m,
    "the binary the signer entitles is the one the bundle carries",
  );
});

test("the release check allows the host exactly the keys its plist carries", () => {
  const check = read("scripts", "check-macos-entitlements.sh");
  const [, allowed] = check.match(/^host_allowed="([^"]*)"$/m) ?? [];
  assert.ok(allowed, "the check lists the host's allowed entitlements");
  assert.deepEqual(allowed.split(/\s+/).sort(), Object.keys(plistBooleans("engine-host", "entitlements.plist")).sort());
  // The guarded assignment, so moving the allowed set to another binary's branch fails here.
  assert.equal(
    (check.match(/\[ "\$name" = "orbyn-engine-host" \] && allowed="\$host_allowed"/g) ?? []).length,
    1,
    "only the host's branch is allowed an entitlements set",
  );
});
