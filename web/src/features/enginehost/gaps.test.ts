import { expect, it } from "vitest";
import type { EngineStatus } from "../../api/types";
import type { HostRow } from "./contract";
import { cannotSend, engineGap, pickerGap, programOf } from "./gaps";

const engine = (patch: Partial<EngineStatus> = {}) => ({ id: "claude", name: "Claude Code", installed: true, ...patch }) as EngineStatus;
const host = (patch: Partial<HostRow> = {}): HostRow => ({ id: "claude", name: "Claude Code", state: "running", can_retry: false, ...patch });

it("says an engine that is not installed once, naming its program", () => {
  const gap = engineGap(engine({ id: "cursor", name: "Cursor", installed: false }), null);
  expect(gap).toMatchObject({ kind: "not_found", sentence: "cursor isn’t installed on this computer.", chip: { label: "Not found" }, action: "check_again" });
});

it("says a missing SDK fails the run, and names the CLI path only for Claude", () => {
  expect(engineGap(engine(), host({ sdk_missing: true }))).toMatchObject({
    kind: "sdk_missing", title: "SDK missing in this build", chip: { label: "SDK missing", tone: "plain" }, action: "open",
    sentence: "This build does not include the Claude Code SDK, so a run on Claude Code fails with that message. Choose Claude (CLI) in Claude path to run it.",
  });
  expect(engineGap(engine({ id: "cursor", name: "Cursor" }), host({ id: "cursor", sdk_missing: true, has_driver: true }))).toMatchObject({
    kind: "sdk_missing", title: "SDK missing in this build", chip: { label: "SDK missing", tone: "err" }, action: null,
    sentence: "This build does not include the Cursor SDK, so a run on Cursor fails with that message.",
  });
});

it("reads an auth error as an expired sign-in with the engine's own command", () => {
  const gap = engineGap(engine(), host({ last_error: { kind: "auth", message: "401", causes: [] } }));
  expect(gap).toMatchObject({ kind: "expired", chip: { label: "Expired" }, action: "sign_in_again", sentence: "Your sign-in expired. Sign in with claude." });
  expect(engineGap(engine(), host({ last_error: { kind: "network", message: "x", causes: [] } }))).toBeNull();
});

it("shows a host that is not ready in the picker, and models for one that is", () => {
  expect(pickerGap(engine(), host())).toBeNull();
  expect(pickerGap(engine(), null)).toBeNull();
  expect(pickerGap(engine(), host({ state: "starting" }))).toMatchObject({ title: "Starting the engine host", chip: { label: "Starting", spin: true }, action: null });
  expect(pickerGap(engine(), host({ state: "restarting", restart: { attempt: 3, limit: 5, next_try_seconds: 4 } }))).toMatchObject({
    title: "Restarting the engine host", sentence: "Restart 3 of 5 · next try in 4 s. Your work in the project is kept.",
  });
  expect(pickerGap(engine(), host({ state: "cant_start", stops_in_window: 5, can_retry: true, last_error: { kind: "host", message: "The engine host closed its output", causes: [] } }))).toMatchObject({
    title: "Can’t start", sentence: "The engine host stopped 5 times in 5 minutes. Last error: The engine host closed its output.", action: "try_again",
  });
  expect(pickerGap(engine(), host({ state: "version_mismatch" }))).toMatchObject({ title: "Version mismatch", action: "update" });
});

it("cuts what the service chose and never keeps a marker or a name uncut", () => {
  const gap = pickerGap(engine(), host({ state: "cant_start", can_retry: true, last_error: { kind: "host", message: "z".repeat(900), causes: [] } }));
  expect(gap!.sentence.length).toBeLessThan(300);
  expect(programOf(engine({ id: `${"a".repeat(80)}:work` }))).toHaveLength(25);
});

it("knows Send is off on a host that can’t start, whatever a wrapped driver says", () => {
  expect(cannotSend(host({ state: "cant_start" }))).toBe(true);
  expect(cannotSend(host({ state: "cant_start", has_driver: true }))).toBe(true);
  expect(cannotSend(host({ state: "restarting" }))).toBe(false);
  expect(cannotSend(null)).toBe(false);
});
