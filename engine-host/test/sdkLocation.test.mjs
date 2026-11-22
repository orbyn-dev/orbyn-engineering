import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { findSdkEntry, importSdk, packageEntry, sdkRoots } from "../src/sdkLocation.mjs";
import { SDK_PACKAGES, isExcludedPackage, listAllows, resolveKey, sdkPaths, selectPackages, stagingLock, stagingManifest } from "../scripts/sdk-packages.mjs";

const NAME = "@vendor/sdk";

/** A fake install: <base>/<NAME>/package.json (ESM, exports) and its entry file. */
function install(base, { entry = "dist/esm/index.js", exports } = {}) {
  const dir = join(base, NAME);
  mkdirSync(join(dir, "dist", "esm"), { recursive: true });
  writeFileSync(join(dir, "package.json"), JSON.stringify({ name: NAME, type: "module", exports: exports ?? { ".": { import: `./${entry}`, default: `./${entry}` } } }));
  writeFileSync(join(dir, entry), "export const where = 'here';\n");
  return join(dir, entry);
}

function layout() {
  const root = mkdtempSync(join(tmpdir(), "orbyn-sdkloc-"));
  const execPath = join(root, "Contents", "MacOS", "orbyn-engine-host");
  mkdirSync(join(root, "Contents", "MacOS"), { recursive: true });
  return {
    root,
    execPath,
    beside: join(root, "Contents", "MacOS", "sdk", "node_modules"),
    bundle: join(root, "Contents", "Resources", "sdk", "node_modules"),
    done: () => rmSync(root, { recursive: true, force: true }),
  };
}

test("candidate folders: beside the executable, then the macOS bundle's Resources", () => {
  const [first, second] = sdkRoots("/app/Contents/MacOS/orbyn-engine-host");
  assert.equal(first, "/app/Contents/MacOS/sdk/node_modules");
  assert.equal(second, "/app/Contents/Resources/sdk/node_modules");
});

test("the entry comes from exports, skipping types, require and bun", () => {
  const manifest = { exports: { ".": { bun: "./b.js", require: "./c.cjs", types: "./t.d.ts", import: "./esm.js", default: "./d.js" } }, main: "./m.js" };
  assert.match(packageEntry("/p", manifest), /\/p\/esm\.js$/);
  assert.match(packageEntry("/p", { exports: { ".": "./only.mjs" } }), /only\.mjs$/);
  assert.match(packageEntry("/p", { exports: "./sugar.mjs" }), /sugar\.mjs$/);
  assert.match(packageEntry("/p", { exports: { ".": { types: "./t.d.ts", default: "./sdk.mjs" } } }), /sdk\.mjs$/);
  assert.match(packageEntry("/p", { main: "./legacy.js" }), /legacy\.js$/);
  assert.match(packageEntry("/p", {}), /index\.js$/);
});

test("the folder beside the executable wins over the bundle's Resources", () => {
  const l = layout();
  try {
    const beside = install(l.beside);
    install(l.bundle);
    assert.equal(findSdkEntry(NAME, { execPath: l.execPath }), beside);
  } finally {
    l.done();
  }
});

test("without a folder beside the executable the bundle's Resources is used", () => {
  const l = layout();
  try {
    const bundled = install(l.bundle);
    assert.equal(findSdkEntry(NAME, { execPath: l.execPath }), bundled);
  } finally {
    l.done();
  }
});

test("a folder whose entry file is missing is skipped for the next one", () => {
  const l = layout();
  try {
    const beside = install(l.beside);
    rmSync(beside);
    const bundled = install(l.bundle);
    assert.equal(findSdkEntry(NAME, { execPath: l.execPath }), bundled);
  } finally {
    l.done();
  }
});

test("with no shipped folder the bare package name is imported (development)", async () => {
  const l = layout();
  try {
    const seen = [];
    await importSdk(undefined, NAME, { execPath: l.execPath, importer: async (specifier) => seen.push(specifier) });
    assert.deepEqual(seen, [NAME]);
  } finally {
    l.done();
  }
});

test("an override wins over a shipped folder: a path by file URL, a name as given", async () => {
  const l = layout();
  try {
    install(l.beside);
    const seen = [];
    const importer = async (specifier) => seen.push(specifier);
    await importSdk("/somewhere/else/sdk.mjs", NAME, { execPath: l.execPath, importer });
    await importSdk("other-sdk", NAME, { execPath: l.execPath, importer });
    assert.deepEqual(seen, [pathToFileURL("/somewhere/else/sdk.mjs").href, "other-sdk"]);
  } finally {
    l.done();
  }
});

test("a shipped SDK really loads by file URL from the folder beside the executable", async () => {
  const l = layout();
  try {
    install(l.beside);
    const mod = await importSdk(undefined, NAME, { execPath: l.execPath });
    assert.equal(mod.where, "here");
  } finally {
    l.done();
  }
});

test("a missing SDK still rejects, so main.mjs can give its plain message", async () => {
  const l = layout();
  try {
    await assert.rejects(importSdk(undefined, "@orbyn-test/definitely-not-installed", { execPath: l.execPath }));
    await assert.rejects(importSdk("/nonexistent/sdk.mjs", NAME, { execPath: l.execPath }));
  } finally {
    l.done();
  }
});

// --- prepare-sdks: package selection and paths (the pure parts) ---

const entry = (extra = {}) => ({ version: "1.0.0", resolved: "https://registry.npmjs.org/x.tgz", integrity: "sha512-x", ...extra });
const lock = {
  packages: {
    "": { name: "host" },
    "node_modules/@anthropic-ai/claude-agent-sdk": entry({
      optionalDependencies: { "@anthropic-ai/claude-agent-sdk-darwin-arm64": "1.0.0", "@anthropic-ai/claude-agent-sdk-linux-x64": "1.0.0" },
      peerDependencies: { zod: "^4", "@anthropic-ai/sdk": "*" },
      peerDependenciesMeta: { zod: { optional: true } },
    }),
    "node_modules/@anthropic-ai/claude-agent-sdk-darwin-arm64": entry({ optional: true, os: ["darwin"], cpu: ["arm64"] }),
    "node_modules/@anthropic-ai/claude-agent-sdk-linux-x64": entry({ optional: true, os: ["linux"], cpu: ["x64"] }),
    "node_modules/@anthropic-ai/sdk": entry({ dependencies: { json: "^1" } }),
    "node_modules/@cursor/sdk": entry({
      dependencies: { dep: "^1" },
      optionalDependencies: { "@cursor/sdk-darwin-arm64": "1.0.0", "@cursor/sdk-linux-x64": "1.0.0", "@cursor/sdk-win32-x64": "1.0.0" },
    }),
    "node_modules/@cursor/sdk-darwin-arm64": entry({ optional: true, os: ["darwin"], cpu: ["arm64"] }),
    "node_modules/@cursor/sdk-linux-x64": entry({ optional: true, os: ["linux"], cpu: ["x64"] }),
    "node_modules/@cursor/sdk-win32-x64": entry({ optional: true, os: ["win32"], cpu: ["x64"] }),
    "node_modules/dep": entry({ dependencies: { json: "^2" } }),
    "node_modules/dep/node_modules/json": entry({ version: "2.0.0" }),
    "node_modules/json": entry(),
    "node_modules/esbuild": entry({ dev: true }),
    "node_modules/@opencode/client": entry(),
  },
};

test("selection keeps the two SDKs and what they need, and the build platform's Cursor package", () => {
  const keys = selectPackages(lock, SDK_PACKAGES, { platform: "darwin", arch: "arm64" });
  assert.deepEqual(keys, [
    "node_modules/@anthropic-ai/claude-agent-sdk",
    "node_modules/@anthropic-ai/sdk",
    "node_modules/@cursor/sdk",
    "node_modules/@cursor/sdk-darwin-arm64",
    "node_modules/dep",
    "node_modules/dep/node_modules/json",
    "node_modules/json",
  ]);
});

test("selection never includes Anthropic's platform packages, whatever the platform", () => {
  for (const [platform, arch] of [["darwin", "arm64"], ["linux", "x64"], ["win32", "x64"]]) {
    const keys = selectPackages(lock, SDK_PACKAGES, { platform, arch });
    assert.equal(keys.some((key) => isExcludedPackage(key.replace("node_modules/", ""))), false, `${platform}-${arch}`);
  }
  assert.ok(selectPackages(lock, SDK_PACKAGES, { platform: "linux", arch: "x64" }).includes("node_modules/@cursor/sdk-linux-x64"));
  assert.equal(selectPackages(lock, SDK_PACKAGES, { platform: "linux", arch: "x64" }).includes("node_modules/@cursor/sdk-darwin-arm64"), false);
});

test("selection leaves out the host's other dependencies and dev tools", () => {
  const keys = selectPackages(lock, SDK_PACKAGES, { platform: "darwin", arch: "arm64" });
  assert.equal(keys.includes("node_modules/esbuild"), false);
  assert.equal(keys.includes("node_modules/@opencode/client"), false);
});

test("selection fails on a stale lockfile instead of shipping a broken folder", () => {
  assert.throws(() => selectPackages({ packages: {} }, SDK_PACKAGES, { platform: "darwin", arch: "arm64" }), /not in the lockfile/);
  const broken = structuredClone(lock);
  delete broken.packages["node_modules/dep"];
  assert.throws(() => selectPackages(broken, SDK_PACKAGES, { platform: "darwin", arch: "arm64" }), /dep .*not in the lockfile/);
});

test("nested resolution prefers the closest node_modules", () => {
  assert.equal(resolveKey(lock.packages, "node_modules/dep", "json"), "node_modules/dep/node_modules/json");
  assert.equal(resolveKey(lock.packages, "node_modules/@cursor/sdk", "json"), "node_modules/json");
  assert.equal(resolveKey(lock.packages, "node_modules/dep", "nothing"), undefined);
});

test("os and cpu lists follow npm's rule, including negation", () => {
  assert.equal(listAllows(undefined, "darwin"), true);
  assert.equal(listAllows(["darwin", "linux"], "win32"), false);
  assert.equal(listAllows(["!win32"], "win32"), false);
  assert.equal(listAllows(["!win32"], "linux"), true);
});

test("the staging manifest takes the exact pins and refuses a range", () => {
  const host = { dependencies: { "@anthropic-ai/claude-agent-sdk": "0.3.321", "@cursor/sdk": "1.4.2", other: "^1.0.0" } };
  assert.deepEqual(stagingManifest(host).dependencies, { "@anthropic-ai/claude-agent-sdk": "0.3.321", "@cursor/sdk": "1.4.2" });
  assert.throws(() => stagingManifest({ dependencies: { ...host.dependencies, "@cursor/sdk": "^1.4.2" } }), /exact version/);
});

test("the staging lock keeps entries unchanged but drops links to packages left out", () => {
  const keys = selectPackages(lock, SDK_PACKAGES, { platform: "darwin", arch: "arm64" });
  const manifest = stagingManifest({ dependencies: { "@anthropic-ai/claude-agent-sdk": "1.0.0", "@cursor/sdk": "1.0.0" } });
  const staged = stagingLock(lock, keys, manifest);
  assert.equal(staged.lockfileVersion, 3);
  assert.deepEqual(Object.keys(staged.packages), ["", ...keys]);
  assert.equal(staged.packages["node_modules/@anthropic-ai/claude-agent-sdk"].optionalDependencies, undefined);
  assert.deepEqual(Object.keys(staged.packages["node_modules/@cursor/sdk"].optionalDependencies), ["@cursor/sdk-darwin-arm64"]);
  assert.equal(staged.packages["node_modules/dep"].integrity, "sha512-x");
  assert.equal(lock.packages["node_modules/@cursor/sdk"].optionalDependencies["@cursor/sdk-win32-x64"], "1.0.0", "the source lock is not changed");
});

test("the SDK folder is dist/sdk with node_modules and the notices inside", () => {
  const paths = sdkPaths("/repo/engine-host");
  assert.equal(paths.dir, "/repo/engine-host/dist/sdk");
  assert.equal(paths.nodeModules, "/repo/engine-host/dist/sdk/node_modules");
  assert.equal(paths.notices, "/repo/engine-host/dist/sdk/THIRD_PARTY_NOTICES.md");
});
