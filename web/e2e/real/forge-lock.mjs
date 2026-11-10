import { mkdirSync, readdirSync, renameSync, rmdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import process from "node:process";
import { randomUUID } from "node:crypto";

const contended = (error) => ["EEXIST", "ENOTEMPTY", "ENOENT", "EPERM", "EACCES"].includes(error.code);
const removeEmpty = (path) => {
  try { rmdirSync(path); } catch (error) { if (!contended(error)) throw error; }
};

/** One populated claim directory is published atomically; live ownership is never expired by age. */
export function acquireForgeLock(statePath) {
  const lock = `${statePath}.lock`;
  const owner = `${process.pid}_${randomUUID()}`;
  const candidate = `${lock}-${owner}`;
  const pause = new Int32Array(new SharedArrayBuffer(4));
  const deadline = Date.now() + 5_000;
  const alive = (pid) => {
    try { process.kill(pid, 0); } catch (error) { return error.code !== "ESRCH"; }
    if (process.platform === "win32") return true;
    // An unreaped, killed process still answers signal 0 on Unix.
    const state = spawnSync("ps", ["-o", "stat=", "-p", String(pid)], { encoding: "utf8" }).stdout ?? "";
    return !state.trim().startsWith("Z");
  };
  mkdirSync(candidate);
  try {
    writeFileSync(join(candidate, owner), "");
    for (;;) {
      try {
        // Never publish an empty directory: a recovery rmdir must not remove a new live owner.
        renameSync(candidate, lock);
        let released = false;
        return () => {
          if (released) return;
          released = true;
          rmSync(join(lock, owner), { force: true });
          removeEmpty(lock);
        };
      } catch (error) {
        if (!contended(error) || Date.now() >= deadline) throw error;
        try {
          for (const claim of readdirSync(lock)) {
            const match = /^(\d+)_[a-f0-9-]+$/.exec(claim);
            if (match && !alive(Number(match[1]))) rmSync(join(lock, claim), { force: true });
          }
          // A late recovery can remove only the dead owner's uniquely named claim.
          // If another contender has since published, its nonempty directory refuses rmdir.
          removeEmpty(lock);
        } catch (readError) {
          if (!contended(readError)) throw readError;
        }
        Atomics.wait(pause, 0, 0, 10);
      }
    }
  } finally {
    // Only this call's unpublished candidate; a successful rename already moved it away.
    rmSync(join(candidate, owner), { force: true });
    removeEmpty(candidate);
  }
}
