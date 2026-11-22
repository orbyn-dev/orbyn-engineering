// Builds dist/sdk/: a production node_modules holding only the Claude Agent SDK and the Cursor SDK and what they need at run
// time, at the versions engine-host/package-lock.json records, plus THIRD_PARTY_NOTICES.md. The host is one JS bundle in a Node
// single executable, so the SDKs ship as this folder beside it (docs/sdk-default-plan.md step 4).
//
// Left out on purpose: Anthropic's own `claude` binaries (@anthropic-ai/claude-agent-sdk-<platform>); the host runs the
// person's own `claude`. Included: the Cursor SDK's package for the build platform. Install scripts never run. The only
// network use is npm fetching the tarballs the lockfile names, checked against its integrity hashes.
import { execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { SDK_PACKAGES, isExcludedPackage, sdkPaths, selectPackages, stagingLock, stagingManifest } from "./sdk-packages.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const flag = (name, fallback) => {
  const at = process.argv.indexOf(`--${name}`);
  return at === -1 ? fallback : process.argv[at + 1];
};
const platform = flag("platform", process.platform);
const arch = flag("arch", process.arch);

const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));
const manifest = stagingManifest(readJson(join(root, "package.json")));
const lock = readJson(join(root, "package-lock.json"));
const keys = selectPackages(lock, SDK_PACKAGES, { platform, arch });

const staging = mkdtempSync(join(tmpdir(), "orbyn-sdks-"));
try {
  writeFileSync(join(staging, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  writeFileSync(join(staging, "package-lock.json"), `${JSON.stringify(stagingLock(lock, keys, manifest), null, 2)}\n`);
  const npm = process.platform === "win32" ? "npm.cmd" : "npm";
  execFileSync(
    npm,
    ["ci", "--omit=dev", "--ignore-scripts", "--no-audit", "--no-fund", `--os=${platform}`, `--cpu=${arch}`],
    { cwd: staging, stdio: "inherit", shell: process.platform === "win32" },
  );

  // Check what was installed against what was asked for, so a wrong folder fails here and not in the app.
  const installed = join(staging, "node_modules");
  for (const name of SDK_PACKAGES) {
    const version = readJson(join(installed, name, "package.json")).version;
    if (version !== manifest.dependencies[name]) throw new Error(`${name} installed ${version}, expected ${manifest.dependencies[name]}`);
  }
  for (const scope of readdirSync(installed).filter((entry) => entry.startsWith("@"))) {
    for (const entry of readdirSync(join(installed, scope))) {
      if (isExcludedPackage(`${scope}/${entry}`)) throw new Error(`${scope}/${entry} must not ship`);
    }
  }
  const platformPackage = keys.filter((key) => key.startsWith("node_modules/@cursor/sdk-"));
  if (platformPackage.length === 0) throw new Error(`no @cursor/sdk platform package matches ${platform}-${arch}`);
  for (const key of platformPackage) {
    if (!existsSync(join(staging, key, "package.json"))) throw new Error(`${key} was not installed`);
  }

  const out = sdkPaths(root);
  rmSync(out.dir, { recursive: true, force: true });
  mkdirSync(out.dir, { recursive: true });
  cpSync(installed, out.nodeModules, { recursive: true });
  // npm's bookkeeping and its `.bin` links are not needed to import the packages (and Tauri copies resources as plain files).
  rmSync(join(out.nodeModules, ".package-lock.json"), { force: true });
  rmSync(join(out.nodeModules, ".bin"), { recursive: true, force: true });
  cpSync(join(root, "THIRD_PARTY_NOTICES.md"), out.notices);
  console.log(`prepared ${out.dir} (${keys.length} packages for ${platform}-${arch})`);
} finally {
  rmSync(staging, { recursive: true, force: true });
}
