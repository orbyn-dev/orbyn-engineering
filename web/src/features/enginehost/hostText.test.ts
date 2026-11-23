import { expect, it } from "vitest";
import type { HostRow } from "./contract";
import { bannerLine, capabilityLine, chipFor, CHIPS, listLine, restartLine, stateLine } from "./hostText";

const row = (patch: Partial<HostRow>): HostRow => ({ id: "x", name: "Codex", state: "running", can_retry: false, ...patch });
const NOW = new Date(2026, 10, 21, 10, 0).getTime();
const at = (h: number, m: number) => new Date(2026, 10, 21, h, m).toISOString();

it("gives green only to Running, blue to the waits that end by themselves, red to what failed", () => {
  const tones = Object.fromEntries(Object.entries(CHIPS).map(([state, chip]) => [state, chip.tone]));
  expect(tones).toEqual({ not_started: "plain", starting: "info", running: "ok", stopped: "plain", crashed: "err",
    restarting: "info", cant_start: "err", version_mismatch: "err", not_used: "plain" });
  expect(CHIPS.cant_start.label).toBe("Can’t start");
});

it("says each list line as the frame does", () => {
  expect(listLine(row({ name: "Claude Code · Personal", host_version: "0.1.0", protocol: 1, since: at(9, 12) }), NOW)).toBe("Host 0.1.0 · protocol 1 · since 09:12");
  expect(listLine(row({ name: "Cursor", pooled: true, since: at(9, 30) }), NOW)).toBe("One host for every Cursor thread · since 09:30");
  expect(listLine(row({ state: "stopped", stopped_at: at(9, 40) }), NOW)).toBe("Stopped 20 minutes ago because nothing was using it. Starts with the next thread.");
  expect(listLine(row({ state: "restarting", restart: { attempt: 3, limit: 5, next_try_seconds: 4 } }), NOW)).toBe("Restart 3 of 5 · next try in 4 s");
  expect(listLine(row({ state: "cant_start", stops_in_window: 5, last_error: { message: "the host did not say hello within 10 seconds", causes: [] } }), NOW))
    .toBe("Stopped 5 times in 5 minutes. Last error: the host did not say hello within 10 seconds.");
  expect(listLine(row({ state: "version_mismatch", protocol: 2, app_protocol: 1 }), NOW)).toBe("The engine host does not match this app version. Host protocol 2, app protocol 1.");
  expect(listLine(row({ state: "not_started" }), NOW)).toBe("Starts with the first thread that uses it.");
  expect(listLine(row({ state: "starting" }), NOW)).toBe("Waiting for the host to say hello");
});

it("names the try and the next try, and drops the next try when none is scheduled", () => {
  expect(restartLine(row({ restart: { attempt: 1, limit: 5, next_try_seconds: null } }))).toBe("Restart 1 of 5");
});

it("says since when on the card, today when it is today", () => {
  expect(stateLine(row({ since: at(9, 12) }), new Date(2026, 10, 21, 12, 0))).toBe("Since 09:12 today");
  expect(stateLine(row({ since: at(9, 12) }), new Date(2026, 10, 23, 12, 0))).toMatch(/^Since 09:12 on /);
});

it("lists what the host said it can do, in its order, and shows a word it does not know as it came", () => {
  expect(capabilityLine(["steer", "fork", "background", "rollback", "models", "usage"])).toBe("Steer a run, fork, background work, roll back, list models, report usage");
  expect(capabilityLine(["steer", "teleport"])).toBe("Steer a run, teleport");
  expect(capabilityLine(["readHistory", "readSubagent", "stopTask", "probe"])).toBe("Read earlier messages, read a sub-agent's messages, stop a background task, check the engine");
});

it("writes the banner for a crash and for a mismatch, and none otherwise", () => {
  expect(bannerLine(row({ state: "restarting", stopped_at: at(9, 41), failed_threads: 2 }))).toBe("The Codex engine host stopped at 09:41. 2 threads that were running on it failed. Your work in the project is kept.");
  expect(bannerLine(row({ state: "crashed", stopped_at: at(9, 41), failed_threads: 1 }))).toContain("1 thread that was running on it failed.");
  expect(bannerLine(row({ state: "version_mismatch", protocol: 2, app_protocol: 1 }))).toBe("The engine host does not match this app version. The host speaks protocol 2; this app speaks protocol 1.");
  expect(bannerLine(row({}))).toBeNull();
});

it("does not crash on a state or capability word that is a prototype name, and cuts host-chosen text", () => {
  expect(chipFor("toString")).toEqual({ label: "Unknown", tone: "plain" });
  expect(chipFor("running")).toBe(CHIPS.running);
  expect(capabilityLine(["constructor", "__proto__", "steer"])).toBe("Constructor, __proto__, Steer a run");
  const long = "n".repeat(300);
  expect(listLine(row({ state: "running", pooled: true, name: long, since: at(9, 12) }), NOW).length).toBeLessThan(160);
  expect(listLine(row({ state: "weird" as never }), NOW)).toBe("");
  expect(bannerLine(row({ state: "crashed", name: long }))!.length).toBeLessThan(200);
  expect(listLine(row({ state: "running", host_version: long, since: at(9, 12) }), NOW).length).toBeLessThan(120);
});
