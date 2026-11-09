import { render, screen } from "@testing-library/react";
import { beforeEach, expect, it } from "vitest";
import type { Thread } from "../../../api/types";
import { useWorkspace } from "../../../product/state/workspace";
import { emptyLog } from "../../../product/timeline/projection";
import type { ThreadItem } from "../../../product/timeline/types";
import { useThreadLogs } from "../../../product/state/threadLogs";
import { ActivityPanel } from "./ActivityPanel";

const thread = { id: "t", active_run: { id: "r", thread_id: "t", state: "running", error: null, started_at: "", ended_at: null } } as unknown as Thread;
const tool: ThreadItem = { kind: "tool", key: "tool:1", callId: "1", seq: 1, runId: "r", ts: "2026-11-08T10:00:00Z", name: "Bash", args: "{}", output: "", result: null, done: false, exitCode: null };

beforeEach(() => {
  useThreadLogs.setState({ logs: {}, streams: {}, prompts: {} });
  useWorkspace.setState({ threads: { t: thread } });
});

it("lists the tool calls running now at the top of the Activity tab", () => {
  useThreadLogs.setState((state) => ({
    logs: { ...state.logs, t: { ...emptyLog, items: [tool], timeline: [{ seq: 1, ts: "2026-11-08T10:00:00Z", label: "Tool: Bash", detail: null, tone: "neutral" }] } },
  }));
  render(<ActivityPanel threadId="t" />);
  const section = screen.getByRole("region", { name: "This run" });
  expect(section).toHaveTextContent("This run · 1 running");
  expect(section).toHaveTextContent("Bash");
});

it("shows nothing for a run that has no call in flight", () => {
  useThreadLogs.setState((state) => ({
    logs: { ...state.logs, t: { ...emptyLog, items: [{ ...tool, result: "ok", done: true }], timeline: [{ seq: 1, ts: "", label: "Tool: Bash", detail: null, tone: "neutral" }] } },
  }));
  render(<ActivityPanel threadId="t" />);
  expect(screen.queryByRole("region", { name: "This run" })).toBeNull();
});
