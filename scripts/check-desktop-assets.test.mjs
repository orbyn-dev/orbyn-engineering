import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const repositoryConfig = JSON.parse(readFileSync(join(root, "desktop/src-tauri/tauri.conf.json")));

function fixture() {
  const directory = mkdtempSync(join(tmpdir(), "orbyn-desktop-assets-"));
  const desktop = join(directory, "desktop");
  const tauri = join(desktop, "src-tauri");
  mkdirSync(tauri, { recursive: true });
  mkdirSync(join(directory, "build/ui"), { recursive: true });
  writeFileSync(join(directory, "build/ui/index.html"), "<!doctype html><title>Fixture UI</title>");
  writeFileSync(join(tauri, "tauri.conf.json"), JSON.stringify(repositoryConfig));
  mkdirSync(join(directory, "scripts"));
  const script = join(root, "scripts/check-desktop-assets.mjs");
  if (existsSync(script)) cpSync(script, join(directory, "scripts/check-desktop-assets.mjs"));
  return { directory, desktop, tauri };
}

function runHook(f, override = {}, platform = "windows") {
  const hook = repositoryConfig.build.beforeBuildCommand;
  // Tauri proceeds to native compilation when no beforeBuildCommand is configured.
  if (!hook) return { status: 0, stdout: "", stderr: "" };
  return spawnSync(hook, {
    cwd: f.desktop,
    shell: true,
    encoding: "utf8",
    env: { ...process.env, TAURI_CONFIG: JSON.stringify(override), TAURI_ENV_PLATFORM: platform },
  });
}

for (const frontendDist of ["C:/work/orbyn/build/ui", "C:\\work\\orbyn\\build\\ui"]) {
  test(`native build hook rejects the Windows URL trap: ${frontendDist}`, () => {
    const result = runHook(fixture(), { build: { frontendDist } });
    assert.notEqual(result.status, 0, "must stop before native compilation");
    assert.match(result.stderr, /frontendDist.*relative directory/);
  });
}

test("native build hook accepts the repository relative path with index.html", () => {
  const result = runHook(fixture());
  assert.equal(result.status, 0, result.stderr);
});

test("relative override resolves from src-tauri, not the hook working directory", () => {
  const f = fixture();
  mkdirSync(join(f.directory, "local UI"));
  writeFileSync(join(f.directory, "local UI/index.html"), "<!doctype html>");
  const result = runHook(f, { build: { frontendDist: "../../local UI" } });
  assert.equal(result.status, 0, result.stderr);
});

for (const entry of ["missing", "empty", "directory"]) {
  test(`native build hook rejects ${entry} index.html`, () => {
    const f = fixture();
    const index = join(f.directory, "build/ui/index.html");
    renameSync(index, `${index}.saved`);
    if (entry === "empty") writeFileSync(index, "");
    if (entry === "directory") mkdirSync(index);
    const result = runHook(f);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Desktop UI asset preflight failed/);
  });
}

for (const [platform, configPlatform] of [["windows", "windows"], ["darwin", "macos"]]) {
  test(`${platform} validates platform config and then applies CLI override precedence`, () => {
    const f = fixture();
    writeFileSync(join(f.tauri, `tauri.${configPlatform}.conf.json`), JSON.stringify({
      build: { frontendDist: "C:/work/orbyn/build/ui" },
    }));
    assert.notEqual(runHook(f, {}, platform).status, 0);
    const result = runHook(f, { build: { frontendDist: "../../build/ui" } }, platform);
    assert.equal(result.status, 0, result.stderr);
  });
}

test("unrelated release overrides preserve the valid base frontendDist", () => {
  const result = runHook(fixture(), { bundle: { createUpdaterArtifacts: true } });
  assert.equal(result.status, 0, result.stderr);
});

for (const override of [
  { build: null }, { build: { frontendDist: null } },
  { build: { frontendDist: ["../../build/ui/index.html"] } },
  { build: { frontendDist: "https://example.invalid" } },
]) {
  test(`invalid embedding configuration fails closed: ${JSON.stringify(override)}`, () => {
    const result = runHook(fixture(), override);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /frontendDist.*relative directory/);
  });
}
