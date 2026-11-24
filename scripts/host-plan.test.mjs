import assert from "node:assert/strict";
import test from "node:test";
import { hostPlan } from "./host-plan.mjs";

test("macOS arm64 ships the host and an arm64 SDK", () => {
  assert.deepEqual(hostPlan({ target: "aarch64-apple-darwin", platform: "darwin", arch: "arm64" }), {
    ship: true,
    sdk: { platform: "darwin", arch: "arm64" },
  });
});

test("Windows x64 ships the host and an x64 SDK", () => {
  assert.deepEqual(hostPlan({ target: "x86_64-pc-windows-msvc", platform: "win32", arch: "x64" }), {
    ship: true,
    sdk: { platform: "win32", arch: "x64" },
  });
});

// A build only ships on the machine class it was built and checked on, so every cross build and every
// other target stages the empty placeholder.
for (const { name, target, platform, arch } of [
  { name: "a macOS target cross-built on Windows", target: "aarch64-apple-darwin", platform: "win32", arch: "x64" },
  { name: "a Windows target cross-built on macOS", target: "x86_64-pc-windows-msvc", platform: "darwin", arch: "arm64" },
  { name: "a Windows x64 target built on an arm64 Windows host", target: "x86_64-pc-windows-msvc", platform: "win32", arch: "arm64" },
  { name: "an Intel Mac target", target: "x86_64-apple-darwin", platform: "darwin", arch: "x64" },
  { name: "a Windows arm64 target", target: "aarch64-pc-windows-msvc", platform: "win32", arch: "arm64" },
  { name: "a Linux target", target: "x86_64-unknown-linux-gnu", platform: "linux", arch: "x64" },
]) {
  test(`${name} does not ship the host`, () => {
    assert.deepEqual(hostPlan({ target, platform, arch }), { ship: false, sdk: null });
  });
}
