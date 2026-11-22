// Pure parts of prepare-sdks.mjs: which packages of the lockfile ship beside the host, and where they go.
// No file or network access here, so node --test can cover them.
import { dirname, join } from "node:path";

/** The two engine SDKs the host loads from disk. Everything else the host needs is bundled by esbuild. */
export const SDK_PACKAGES = ["@anthropic-ai/claude-agent-sdk", "@cursor/sdk"];

/** Anthropic's own `claude` binaries. The host runs the person's own `claude` (pathToClaudeCodeExecutable), so these never ship. */
export const isExcludedPackage = (name) => name.startsWith("@anthropic-ai/claude-agent-sdk-");

/** Where a build puts the SDK folder: `<engine-host>/dist/sdk`, with the packages in `node_modules` and the notices beside it. */
export const sdkPaths = (engineHostRoot) => {
  const dir = join(engineHostRoot, "dist", "sdk");
  return { dir, nodeModules: join(dir, "node_modules"), notices: join(dir, "THIRD_PARTY_NOTICES.md") };
};

/** npm's own os/cpu rule: a list may hold names to allow, or `!name` to deny. An absent or empty list allows everything. */
export function listAllows(list, value) {
  if (!Array.isArray(list) || list.length === 0) return true;
  const denied = list.filter((item) => item.startsWith("!")).map((item) => item.slice(1));
  if (denied.includes(value)) return false;
  const allowed = list.filter((item) => !item.startsWith("!"));
  return allowed.length === 0 || allowed.includes(value);
}

export const platformMatches = (entry, platform, arch) => listAllows(entry.os, platform) && listAllows(entry.cpu, arch);

/** The lock key a dependency resolves to from a package at `from` (npm's nested node_modules lookup), or undefined. */
export function resolveKey(packages, from, name) {
  let base = from;
  for (;;) {
    const key = base === "" ? `node_modules/${name}` : `${base}/node_modules/${name}`;
    if (packages[key]) return key;
    if (base === "") return undefined;
    const cut = base.lastIndexOf("/node_modules/");
    base = cut === -1 ? "" : base.slice(0, cut);
  }
}

/**
 * The lock keys to install: the roots and everything they need at run time (dependencies, peers, and optional packages that
 * match the build platform), minus Anthropic's platform packages. A missing required dependency is an error, so a stale
 * lockfile fails the build instead of shipping a broken folder.
 */
export function selectPackages(lock, roots, { platform, arch }) {
  const packages = lock.packages ?? {};
  const keep = new Set();
  const queue = [];
  const add = (key) => {
    if (!keep.has(key)) {
      keep.add(key);
      queue.push(key);
    }
  };
  for (const root of roots) {
    const key = `node_modules/${root}`;
    if (!packages[key]) throw new Error(`${root} is not in the lockfile`);
    add(key);
  }
  while (queue.length > 0) {
    const key = queue.shift();
    const entry = packages[key];
    const peerMeta = entry.peerDependenciesMeta ?? {};
    const wanted = [
      ...Object.keys(entry.dependencies ?? {}).map((name) => ({ name, required: true })),
      ...Object.keys(entry.peerDependencies ?? {}).map((name) => ({ name, required: !peerMeta[name]?.optional })),
      ...Object.keys(entry.optionalDependencies ?? {}).map((name) => ({ name, required: false })),
    ];
    for (const { name, required } of wanted) {
      if (isExcludedPackage(name)) continue;
      const found = resolveKey(packages, key, name);
      if (!found) {
        if (required) throw new Error(`${name} (needed by ${key.replace(/^.*node_modules\//, "")}) is not in the lockfile`);
        continue;
      }
      if (!required && !platformMatches(packages[found], platform, arch)) continue;
      add(found);
    }
  }
  return [...keep].sort();
}

/** The staging manifest: only the two SDKs, at the exact versions the host's own package.json pins. */
export function stagingManifest(hostManifest, roots = SDK_PACKAGES) {
  const dependencies = {};
  for (const name of roots) {
    const version = hostManifest.dependencies?.[name];
    if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) throw new Error(`${name} must be an exact version in package.json, found ${version}`);
    dependencies[name] = version;
  }
  return { name: "orbyn-engine-host-sdks", version: "0.0.0", private: true, dependencies };
}

/** A lockfile for the staging manifest: the selected entries unchanged (versions, URLs, hashes), with links to dropped packages removed. */
export function stagingLock(lock, keys, manifest) {
  const keep = new Set(keys);
  const packages = { "": { name: manifest.name, version: manifest.version, dependencies: manifest.dependencies } };
  for (const key of keys) {
    const entry = structuredClone(lock.packages[key]);
    for (const field of ["optionalDependencies", "peerDependencies"]) {
      if (!entry[field]) continue;
      for (const name of Object.keys(entry[field])) {
        const found = resolveKey(lock.packages, key, name);
        if (isExcludedPackage(name) || !found || !keep.has(found)) {
          if (field === "optionalDependencies") delete entry[field][name];
        }
      }
      if (Object.keys(entry[field]).length === 0) delete entry[field];
    }
    // Development-only markers from the full tree do not apply to a production install of these packages.
    delete entry.dev;
    delete entry.devOptional;
    packages[key] = entry;
  }
  return { name: manifest.name, version: manifest.version, lockfileVersion: 3, requires: true, packages };
}
