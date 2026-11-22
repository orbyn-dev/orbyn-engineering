import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

// Real ad hoc signed copies of a system binary stand in for the app's binaries; macOS only.
const script = join(dirname(fileURLToPath(import.meta.url)), "check-macos-entitlements.sh");
const macos = process.platform === "darwin";
const JIT = "com.apple.security.cs.allow-jit";
const MEMORY = "com.apple.security.cs.allow-unsigned-executable-memory";
const LIBRARY = "com.apple.security.cs.disable-library-validation";
const DEBUG = "com.apple.security.get-task-allow";

function plist(keys) {
  const body = keys.map((key) => `<key>${key}</key><true/>`).join("");
  return `<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict>${body}</dict></plist>`;
}

/** An app with these binaries, each signed ad hoc with its own entitlements (and the hardened runtime when asked). */
function app(binaries, { runtime = false } = {}) {
  const directory = mkdtempSync(join(tmpdir(), "orbyn-entitlements-"));
  const macosDirectory = join(directory, "Orbyn Engineering.app", "Contents", "MacOS");
  mkdirSync(macosDirectory, { recursive: true });
  for (const [name, keys] of Object.entries(binaries)) {
    const file = join(macosDirectory, name);
    copyFileSync("/usr/bin/true", file);
    const args = ["--force", "--sign", "-"];
    if (runtime) args.push("--options", "runtime");
    if (keys.length > 0) {
      const entitlements = join(directory, `${name}.plist`);
      writeFileSync(entitlements, plist(keys));
      args.push("--entitlements", entitlements);
    }
    const signed = spawnSync("codesign", [...args, file], { encoding: "utf8" });
    assert.equal(signed.status, 0, signed.stderr);
  }
  return { directory, path: join(directory, "Orbyn Engineering.app") };
}

function check(path, ...flags) {
  return spawnSync("bash", [script, ...flags, path], { encoding: "utf8" });
}

function run(binaries, options, ...flags) {
  const built = app(binaries, options);
  try {
    return check(built.path, ...flags);
  } finally {
    rmSync(built.directory, { recursive: true, force: true });
  }
}

test("the host may carry its three entitlements and nothing else carries any", { skip: !macos }, () => {
  const result = run({ "orbyn-engine-host": [JIT, MEMORY, LIBRARY], orbynd: [], orbyn: [], "orbyn-engineering": [] });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Entitlements ok: 4 binaries/);
});

test("orbynd with disable-library-validation fails", { skip: !macos }, () => {
  const result = run({ "orbyn-engine-host": [JIT, MEMORY, LIBRARY], orbynd: [LIBRARY] });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /orbynd carries com\.apple\.security\.cs\.disable-library-validation/);
});

test("get-task-allow fails on any binary, the host included", { skip: !macos }, () => {
  const result = run({ "orbyn-engine-host": [JIT, DEBUG], orbynd: [] });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /orbyn-engine-host carries com\.apple\.security\.get-task-allow/);
});

test("the host carrying an entitlement outside its three fails", { skip: !macos }, () => {
  const result = run({ "orbyn-engine-host": [JIT, "com.apple.security.cs.allow-dyld-environment-variables"], orbynd: [] });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /allow-dyld-environment-variables, which is not allowed/);
});

test("an ad hoc build without the hardened runtime passes by default and fails --release", { skip: !macos }, () => {
  const binaries = { "orbyn-engine-host": [JIT, MEMORY, LIBRARY], orbynd: [] };
  const plain = run(binaries, {});
  assert.equal(plain.status, 0, plain.stderr);
  assert.match(plain.stdout, /Hardened runtime: not checked/);
  const strict = run(binaries, {}, "--release");
  assert.equal(strict.status, 1);
  assert.match(strict.stderr, /orbynd has no hardened runtime flag/);
});

test("--release passes when every binary has the hardened runtime and only its allowed entitlements", { skip: !macos }, () => {
  const result = run({ "orbyn-engine-host": [JIT, MEMORY, LIBRARY], orbynd: [] }, { runtime: true }, "--release");
  assert.equal(result.status, 0, result.stderr);
});

test("a path that is not an app bundle is a usage error", { skip: !macos }, () => {
  assert.equal(check("/nonexistent/app.app").status, 2);
});
