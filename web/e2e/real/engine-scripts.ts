import { resolve } from "node:path";

// Windows follows these npm-style shims to Node; CreateProcess cannot run a shebang.
const extension = process.platform === "win32" ? "cmd" : "mjs";
export const SCRIPTED_ENGINE = resolve(import.meta.dirname, `scripted-engine.${extension}`);
export const USAGE_ENGINE = resolve(import.meta.dirname, `usage-engine.${extension}`);
