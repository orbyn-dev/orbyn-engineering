import { spawn, spawnSync } from "node:child_process";
import { once } from "node:events";
import { mkdtempSync, writeFileSync, rmSync, readFileSync, utimesSync, existsSync, renameSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { expect, it } from "vitest";
import { editForgeState } from "../../e2e/real/forge-state";
import { acquireForgeLock } from "../../e2e/real/forge-lock.mjs";

it("keeps forge state readable while a spec edits a pull during a forge read", async () => {
  const dir = mkdtempSync(join(tmpdir(), "orbyn-forge-state-"));
  const path = join(dir, "state.json");
  const done = join(dir, "done");
  writeFileSync(path, JSON.stringify({ pulls: [{ number: 3, title: "Original", headNow: "old" }] }));
  const reader = spawn(process.execPath, ["--input-type=module", "-e", `
    import { existsSync, readFileSync } from "node:fs";
    const [path, done, lockModule] = process.argv.slice(1);
    const { acquireForgeLock } = await import(lockModule);
    const read = () => {
      const release = acquireForgeLock(path);
      try { JSON.parse(readFileSync(path, "utf8")); } finally { release(); }
    };
    let reads = 0;
    // The unchanged state is a control before any edits begin.
    read();
    process.stdout.write("ready\\n");
    try {
      while (!existsSync(done)) { read(); reads++; }
      process.stdout.write(JSON.stringify({ reads }));
    } catch (error) { process.stderr.write(String(error)); process.exitCode = 1; }
  `, path, done, pathToFileURL(join(import.meta.dirname, "../../e2e/real/forge-lock.mjs")).href], { stdio: ["ignore", "pipe", "pipe"], timeout: 5_000 });
  let output = "";
  let failure = "";
  reader.stdout.on("data", (chunk: Buffer) => { output += chunk.toString(); });
  reader.stderr.on("data", (chunk: Buffer) => { failure += chunk.toString(); });
  const closed = once(reader, "close");
  try {
    await once(reader.stdout, "data");
    // Large enough to expose an in-place truncate/write, still a bounded 32 MiB of writes.
    const body = "x".repeat(1024 * 1024);
    for (let index = 0; index < 32; index++) editForgeState(path, 3, { body, title: `Read ${index}` });
    writeFileSync(done, "done");
    const [code] = await closed;
    expect(failure).toBe("");
    expect(code).toBe(0);
    expect(JSON.parse(output.split("\n")[1] ?? "{}").reads).toBeGreaterThan(0);
    expect(JSON.parse(readFileSync(path, "utf8")).pulls[0]).toMatchObject({ number: 3, title: "Read 31", headNow: "old" });
  } finally {
    if (reader.exitCode === null) reader.kill();
    await closed;
    rmSync(dir, { recursive: true, force: true });
  }
});

it("edits forge-wide fields and preserves other pulls", () => {
  const dir = mkdtempSync(join(tmpdir(), "orbyn-forge-state-"));
  const path = join(dir, "state.json");
  try {
    writeFileSync(path, JSON.stringify({ viewer: "me", pulls: [{ number: 1, title: "Control" }, { number: 3, headNow: "old" }] }));
    editForgeState(path, 0, { viewer: "reviewer" });
    editForgeState(path, 3, { headNow: undefined });
    expect(JSON.parse(readFileSync(path, "utf8"))).toEqual({ viewer: "reviewer", pulls: [{ number: 1, title: "Control" }, { number: 3 }] });
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

it("does not steal an old lock from a live fixture process", async () => {
  const dir = mkdtempSync(join(tmpdir(), "orbyn-forge-lock-"));
  const path = join(dir, "state.json");
  const release = acquireForgeLock(path);
  utimesSync(`${path}.lock`, new Date(0), new Date(0));
  const child = spawn(process.execPath, ["--input-type=module", "-e", `
    const { acquireForgeLock } = await import(process.argv[1]);
    process.stderr.write("attempting\\n");
    const release = acquireForgeLock(process.argv[2]);
    process.stdout.write("acquired\\n");
    release();
  `, pathToFileURL(join(import.meta.dirname, "../../e2e/real/forge-lock.mjs")).href, path], { stdio: ["ignore", "pipe", "pipe"], timeout: 5_000 });
  let acquired = false;
  child.stdout.on("data", () => { acquired = true; });
  const closed = once(child, "close");
  try {
    await once(child.stderr, "data");
    await new Promise((resolve) => setTimeout(resolve, 150));
    expect(acquired).toBe(false);
    release();
    expect((await closed)[0]).toBe(0);
    expect(acquired).toBe(true);
  } finally {
    release();
    if (child.exitCode === null) child.kill();
    await closed;
    rmSync(dir, { recursive: true, force: true });
  }
});

it("a delayed release cannot remove a different owner's lock", () => {
  const dir = mkdtempSync(join(tmpdir(), "orbyn-forge-lock-"));
  const path = join(dir, "state.json");
  try {
    const release = acquireForgeLock(path);
    renameSync(`${path}.lock`, `${path}.retired`);
    const releaseReplacement = acquireForgeLock(path);
    const claims = readdirSync(`${path}.lock`);
    release();
    expect(readdirSync(`${path}.lock`)).toEqual(claims);
    releaseReplacement();
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

it("recovers a lock left by a process that has exited", () => {
  const dir = mkdtempSync(join(tmpdir(), "orbyn-forge-lock-"));
  const path = join(dir, "state.json");
  try {
    const child = spawnSync(process.execPath, ["--input-type=module", "-e", `
      const { acquireForgeLock } = await import(process.argv[1]);
      acquireForgeLock(process.argv[2]);
    `, pathToFileURL(join(import.meta.dirname, "../../e2e/real/forge-lock.mjs")).href, path], { timeout: 5_000 });
    expect(child.status).toBe(0);
    expect(existsSync(`${path}.lock`)).toBe(true);
    const release = acquireForgeLock(path);
    release();
    expect(existsSync(`${path}.lock`)).toBe(false);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

it("serializes two contenders recovering the same dead owner", async () => {
  const dir = mkdtempSync(join(tmpdir(), "orbyn-forge-lock-"));
  const path = join(dir, "state.json");
  const lockModule = pathToFileURL(join(import.meta.dirname, "../../e2e/real/forge-lock.mjs")).href;
  const dead = spawnSync(process.execPath, ["--input-type=module", "-e", `
    const { acquireForgeLock } = await import(process.argv[1]);
    acquireForgeLock(process.argv[2]);
  `, lockModule, path], { timeout: 5_000 });
  expect(dead.status).toBe(0);
  const children = [0, 1].map(() => spawn(process.execPath, ["--input-type=module", "-e", `
    import { writeFileSync, rmSync } from "node:fs";
    const { acquireForgeLock } = await import(process.argv[1]);
    process.once("message", () => {
      const release = acquireForgeLock(process.argv[2]);
      const critical = process.argv[2] + ".critical";
      // Exclusive creation fails if stale recovery ever lets both owners in together.
      writeFileSync(critical, String(process.pid), { flag: "wx" });
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 100);
      rmSync(critical);
      release();
      process.disconnect();
    });
    process.send("ready");
  `, lockModule, path], { stdio: ["ignore", "ignore", "pipe", "ipc"], timeout: 5_000 }));
  const closed = children.map((child) => once(child, "close"));
  const failures = children.map(() => "");
  // The IPC spawn overload is nullable in the types; stderr is explicitly piped above.
  children.forEach((child, index) => child.stderr!.on("data", (chunk: Buffer) => { failures[index] += chunk.toString(); }));
  try {
    await Promise.all(children.map((child) => once(child, "message")));
    children.forEach((child) => child.send("start"));
    expect((await Promise.all(closed)).map(([code]) => code)).toEqual([0, 0]);
    expect(failures).toEqual(["", ""]);
    expect(existsSync(`${path}.lock`)).toBe(false);
  } finally {
    children.forEach((child) => { if (child.exitCode === null) child.kill(); });
    await Promise.all(closed);
    rmSync(dir, { recursive: true, force: true });
  }
});
