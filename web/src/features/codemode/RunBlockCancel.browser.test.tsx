import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import { codeRun } from "../../../mock/codemode";
import { canSendPersonKey, forgetPersonKey } from "../../api/personKey";
import type { ToolItem } from "../../product/timeline/types";
import { installMockFetch } from "../../test-support";
import { CodeRunBlock } from "./RunBlock";
import type { CodeRunRecord } from "./types";
import { resetCodeRuns } from "./useCodeRun";

// Deliberately no `vi.mock("../../api/personKey")`: RunBlockCancel.test.tsx replaces `canSendPersonKey`, so only this file runs
// the real browser-tab check from the block to the module.
const NAME = "mcp__orbyn_engineering__orbyn_code_run";
const STARTED = "2026-11-21T10:00:00.000Z";
const RUN = "run_7c41e2";

const record = (extra: Partial<CodeRunRecord> = {}): CodeRunRecord =>
  codeRun({
    run_id: RUN,
    status: "running",
    script: "return 1;",
    started_at: STARTED,
    ended_at: null,
    ...extra,
  });

const item = (run: CodeRunRecord): ToolItem => ({
  kind: "tool",
  key: `tool:${run.run_id}`,
  callId: "c",
  seq: 1,
  runId: "r",
  ts: run.started_at,
  endedAt: run.ended_at ?? undefined,
  name: NAME,
  args: JSON.stringify({ code: run.script ?? "return 1;" }),
  output: "",
  result: JSON.stringify({ status: run.status, runId: run.run_id }),
  done: run.status === "completed" || run.status === "error",
  exitCode: null,
});

const show = (run: CodeRunRecord) => render(<CodeRunBlock item={item(run)} live={false} inspect={null} />);
const footer = () => document.querySelector(".cf .tfm");
const cancelButton = () => screen.queryByRole("button", { name: "Cancel run" });

let mock: MockApi;
beforeEach(() => {
  vi.stubGlobal("__ORBYN_PREVIEWS__", false);
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  resetCodeRuns();
  mock = installMockFetch();
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.stubGlobal("__ORBYN_PREVIEWS__", true);
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  forgetPersonKey();
});

describe("the Cancel run button in a browser tab", () => {
  it("is not drawn where the page has no shell bridge and no dev preview, so it cannot send the person key", async () => {
    mock.codeMode.putRun(record());
    show(record());
    await screen.findByText("Calls · 0");
    expect(footer()).toHaveTextContent("Running");
    expect(screen.getByText(RUN)).toBeInTheDocument();
    expect(canSendPersonKey()).toBe(false);
    expect(cancelButton()).toBeNull();
    expect(document.querySelector(".cm-cancel")).toBeNull();
  });
});
