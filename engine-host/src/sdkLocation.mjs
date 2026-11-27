// Where the host loads the Claude and Cursor SDKs from. They are dependencies shipped as a `node_modules` folder beside the
// host (engine-host/scripts/prepare-sdks.mjs, docs/sdk-default-plan.md step 4), because the single executable cannot hold their
// native files. Order: an override env var, then `sdk/node_modules/<pkg>` beside the executable, then `../Resources/sdk/...`
// (the macOS app bundle: Contents/MacOS/<host> next to Contents/Resources), else the bare package name (development).
import { existsSync, readFileSync } from "node:fs";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

/** Candidate folders that may hold `node_modules/<package>`, first match wins. */
export const sdkRoots = (execPath) => {
  const beside = dirname(execPath);
  return [join(beside, "sdk", "node_modules"), join(beside, "..", "Resources", "sdk", "node_modules")];
};

// Conditions that apply to a Node ESM import; `types`, `require` and `bun` never do.
const CONDITIONS = ["node", "import", "module", "default"];

function fromExports(target) {
  if (typeof target === "string") return target;
  if (Array.isArray(target)) {
    for (const item of target) {
      const found = fromExports(item);
      if (found) return found;
    }
    return undefined;
  }
  if (target && typeof target === "object") {
    // Object order is the package's priority order; take the first key that is a usable condition.
    for (const key of Object.keys(target)) {
      if (!CONDITIONS.includes(key)) continue;
      const found = fromExports(target[key]);
      if (found) return found;
    }
  }
  return undefined;
}

/** The entry file of a package folder for an ESM import: `exports["."]`, else `main`, else `index.js`. */
export function packageEntry(packageDir, manifest) {
  const exports = manifest.exports;
  const root = exports && typeof exports === "object" && !Array.isArray(exports) && Object.keys(exports).some((key) => key.startsWith("."))
    ? exports["."]
    : exports;
  const relative = fromExports(root) ?? manifest.main ?? "index.js";
  return resolve(packageDir, relative);
}

/** The entry file of `name` in the first existing candidate folder, or undefined. `fs` is injectable for tests. */
export function findSdkEntry(name, { execPath = process.execPath, exists = existsSync, readText = (path) => readFileSync(path, "utf8") } = {}) {
  for (const base of sdkRoots(execPath)) {
    const packageDir = join(base, name);
    const manifestPath = join(packageDir, "package.json");
    if (!exists(manifestPath)) continue;
    const entry = packageEntry(packageDir, JSON.parse(readText(manifestPath)));
    if (exists(entry)) return entry;
  }
  return undefined;
}

/**
 * Imports an SDK. An override wins: an absolute path is imported by file URL, a plain name as usual. Without one, the shipped
 * folder is used when it exists, else the bare package name resolves from `node_modules` (development).
 */
export function importSdk(override, name, options = {}) {
  const load = options.importer ?? ((specifier) => import(specifier));
  if (override) return load(isAbsolute(override) ? pathToFileURL(override).href : override);
  const entry = findSdkEntry(name, options);
  return load(entry ? pathToFileURL(entry).href : name);
}
