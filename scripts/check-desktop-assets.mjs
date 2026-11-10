// Tauri runs this from desktop before native compilation. --config overrides are
// supplied by the CLI as a merged JSON patch in TAURI_CONFIG.
import { readFileSync, statSync } from "node:fs";
import { isAbsolute, join, resolve, win32 } from "node:path";

function readConfig(path, optional = false) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    if (optional && error.code === "ENOENT") return {};
    throw error;
  }
}

try {
  const tauri = resolve("src-tauri");
  const platform = process.env.TAURI_ENV_PLATFORM ?? process.platform;
  const configPlatform = { darwin: "macos", win32: "windows" }[platform] ?? platform;
  const layers = [
    readConfig(join(tauri, "tauri.conf.json")),
    readConfig(join(tauri, `tauri.${configPlatform}.conf.json`), true),
    JSON.parse(process.env.TAURI_CONFIG ?? "{}"),
  ];
  // Only frontendDist is needed. Retain JSON merge-patch's removal semantics for
  // a null build/frontendDist instead of falling back to the base configuration.
  let frontendDist;
  for (const layer of layers) {
    if (Object.hasOwn(layer, "build")) {
      if (layer.build === null) frontendDist = undefined;
      else if (Object.hasOwn(layer.build, "frontendDist")) frontendDist = layer.build.frontendDist;
    }
  }
  if (typeof frontendDist !== "string" || !frontendDist.trim()
    || isAbsolute(frontendDist) || win32.isAbsolute(frontendDist)
    || /^[a-z][a-z\d+.-]*:/i.test(frontendDist)) {
    throw new Error("build.frontendDist must be a relative directory containing index.html. "
      + "Windows drive paths can be parsed as URLs by Tauri and embed no UI assets; "
      + "use a path relative to desktop/src-tauri (normally ../../build/ui).");
  }
  const index = resolve(tauri, frontendDist, "index.html");
  const entry = statSync(index);
  if (!entry.isFile() || entry.size === 0) throw new Error(`Missing nonempty UI entry point: ${index}`);
  console.log("Desktop UI assets verified: relative frontendDist contains index.html.");
} catch (error) {
  console.error(`Desktop UI asset preflight failed: ${error.message}`);
  process.exitCode = 1;
}
