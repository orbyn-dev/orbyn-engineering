import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

// Real codesign runs against copies of a system binary, so these tests are macOS only.
const script = join(dirname(fileURLToPath(import.meta.url)), "sign-macos-release.sh");
const macos = process.platform === "darwin";
const JIT = "com.apple.security.cs.allow-jit";
const MEMORY = "com.apple.security.cs.allow-unsigned-executable-memory";
const LIBRARY = "com.apple.security.cs.disable-library-validation";
const HOST_KEYS = [JIT, MEMORY, LIBRARY].sort();

/** A fake app with these MacOS binaries and one native file under Resources/sdk, each a copy of /usr/bin/true. */
function app({ host = true } = {}) {
  const directory = mkdtempSync(join(tmpdir(), "orbyn-sign-release-"));
  const path = join(directory, "Orbyn Engineering.app");
  const contents = join(path, "Contents");
  mkdirSync(join(contents, "MacOS"), { recursive: true });
  mkdirSync(join(contents, "Resources", "sdk"), { recursive: true });
  const binaries = ["orbyn-engineering", "orbynd", "orbyn", ...(host ? ["orbyn-engine-host"] : [])];
  for (const name of binaries) {
    copyFileSync("/usr/bin/true", join(contents, "MacOS", name));
  }
  copyFileSync("/usr/bin/true", join(contents, "Resources", "sdk", "native.node"));
  writeFileSync(
    join(contents, "Info.plist"),
    `<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>CFBundleExecutable</key><string>orbyn-engineering</string><key>CFBundleIdentifier</key><string>test.orbyn.sign-release</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>`,
  );
  return { directory, path };
}

function sign(path, ...args) {
  // The release default is ad hoc ("-"); keep a developer's APPLE_SIGNING_IDENTITY out of the test.
  return spawnSync("bash", [script, ...args, path], {
    encoding: "utf8",
    env: { ...process.env, APPLE_SIGNING_IDENTITY: "" },
  });
}

/** The entitlement keys codesign reports for a binary; an empty list when it carries none. */
function entitlementKeys(file) {
  const signed = spawnSync("codesign", ["-d", "--entitlements", ":-", file], { encoding: "utf8" });
  assert.equal(signed.status, 0, signed.stderr);
  const start = signed.stdout.indexOf("<?xml");
  if (start < 0) return [];
  const parsed = spawnSync(
    "python3",
    ["-c", "import plistlib, sys\nprint('\\n'.join(plistlib.loads(sys.stdin.buffer.read())))"],
    { input: signed.stdout.slice(start), encoding: "utf8" },
  );
  assert.equal(parsed.status, 0, parsed.stderr);
  return parsed.stdout.split("\n").filter(Boolean).sort();
}

test("the ad hoc release signing puts the host's three entitlements on the host and none anywhere else", { skip: !macos }, () => {
  const built = app();
  try {
    const signed = sign(built.path);
    assert.equal(signed.status, 0, signed.stderr);
    assert.match(signed.stdout, /Entitlements ok/);
    assert.deepEqual(entitlementKeys(join(built.path, "Contents", "MacOS", "orbyn-engine-host")), HOST_KEYS);
    for (const file of [
      join(built.path, "Contents", "MacOS", "orbynd"),
      join(built.path, "Contents", "MacOS", "orbyn"),
      join(built.path, "Contents", "MacOS", "orbyn-engineering"),
      join(built.path, "Contents", "Resources", "sdk", "native.node"),
    ]) {
      assert.deepEqual(entitlementKeys(file), [], file);
    }
  } finally {
    rmSync(built.directory, { recursive: true, force: true });
  }
});

test("signing an already signed bundle again works", { skip: !macos }, () => {
  const built = app();
  try {
    const first = sign(built.path);
    assert.equal(first.status, 0, first.stderr);
    const again = sign(built.path);
    assert.equal(again.status, 0, again.stderr);
    assert.match(again.stdout, /Entitlements ok/);
    assert.deepEqual(entitlementKeys(join(built.path, "Contents", "MacOS", "orbyn-engine-host")), HOST_KEYS);
  } finally {
    rmSync(built.directory, { recursive: true, force: true });
  }
});

test("a path that is not a directory is a usage error", { skip: !macos }, () => {
  const directory = mkdtempSync(join(tmpdir(), "orbyn-sign-release-"));
  try {
    const file = join(directory, "not-an-app");
    writeFileSync(file, "");
    const result = sign(file);
    assert.equal(result.status, 2);
    assert.match(result.stderr, /Usage:/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test("without the engine host the rest is still signed and the release check passes", { skip: !macos }, () => {
  const built = app({ host: false });
  try {
    const signed = sign(built.path);
    assert.equal(signed.status, 0, signed.stderr);
    assert.match(signed.stdout, /Entitlements ok: 4 binaries checked\./);
  } finally {
    rmSync(built.directory, { recursive: true, force: true });
  }
});
