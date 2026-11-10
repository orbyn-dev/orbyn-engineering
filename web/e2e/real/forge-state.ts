import { readFileSync, renameSync, writeFileSync } from "node:fs";
import { acquireForgeLock } from "./forge-lock.mjs";

export function readForgeState(path: string): { pulls: { number: number; [field: string]: unknown }[] } & Record<string, unknown> {
  const release = acquireForgeLock(path);
  try { return JSON.parse(readFileSync(path, "utf8")); }
  finally { release(); }
}

/** The spec may edit state while a stand-in process is reading it. */
export function editForgeState(path: string, number: number, patch: Record<string, unknown>): void {
  const release = acquireForgeLock(path);
  try {
    const state = JSON.parse(readFileSync(path, "utf8")) as { pulls: { number: number }[] } & Record<string, unknown>;
    const target = number === 0 ? state : state.pulls.find((pull) => pull.number === number);
    if (target) Object.assign(target, patch);
    // Readers must see one complete revision, including when Reload starts a forge process.
    const pending = `${path}.${process.pid}.edit`;
    writeFileSync(pending, JSON.stringify(state));
    renameSync(pending, path);
  } finally { release(); }
}
