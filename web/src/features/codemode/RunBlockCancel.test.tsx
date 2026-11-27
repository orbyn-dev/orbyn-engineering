import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import { codeRun } from "../../../mock/codemode";
import type { ToolItem } from "../../product/timeline/types";
import { installMockFetch } from "../../test-support";
import { CodeRunBlock } from "./RunBlock";
import type { CodeRunRecord, CodeRunStatus } from "./types";
import { resetCodeRuns } from "./useCodeRun";

// The page's ability to send the person key decides whether the button is offered at all (api/personKey.ts).
let canCancel = true;
vi.mock("../../api/personKey", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../api/personKey")>()),
  canSendPersonKey: () => canCancel,
}));

const NAME = "mcp__orbyn_engineering__orbyn_code_run";
const STARTED = "2026-11-21T10:00:00.000Z";
const RUN = "run_7c41e2";
const CANCEL_PATH = `/api/code-runs/${RUN}/cancel`;

const record = (extra: Partial<CodeRunRecord> = {}): CodeRunRecord =>
  codeRun({
    run_id: RUN,
    status: "running",
    script: "return 1;",
    started_at: STARTED,
    ended_at: null,
    ...extra,
  });

const settled: Partial<Record<CodeRunStatus, Partial<CodeRunRecord>>> = {
  completed: { ended_at: "2026-11-21T10:00:03.100Z" },
  error: { ended_at: "2026-11-21T10:00:03.100Z", error: { kind: "runtime", message: "It broke." } },
};

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
const posts = () => vi.mocked(fetch).mock.calls.filter(([, init]) => (init as RequestInit | undefined)?.method === "POST");
const reads = () => vi.mocked(fetch).mock.calls.filter(([url, init]) => ((init as RequestInit | undefined)?.method ?? "GET") === "GET" && String(url) === `/api/code-runs/${RUN}`);

let mock: MockApi;
beforeEach(() => {
  canCancel = true;
  resetCodeRuns();
  mock = installMockFetch();
});
afterEach(() => vi.restoreAllMocks());

describe("the Cancel run button", () => {
  it.each(["running", "paused"] as const)("is offered on a %s run", async (status) => {
    mock.codeMode.putRun(record({ status }));
    show(record({ status }));
    expect(await screen.findByRole("button", { name: "Cancel run" })).toBeVisible();
  });

  it.each(["completed", "error"] as const)("is absent on a %s run", async (status) => {
    mock.codeMode.putRun(record({ status, ...settled[status] }));
    show(record({ status, ...settled[status] }));
    await waitFor(() => expect(footer()).toHaveTextContent(status === "completed" ? "Completed" : "Failed"));
    expect(cancelButton()).toBeNull();
  });

  it("is absent where the page cannot send the person key", async () => {
    canCancel = false;
    mock.codeMode.putRun(record());
    show(record());
    await waitFor(() => expect(reads()).toHaveLength(1));
    expect(cancelButton()).toBeNull();
  });

  it("sends exactly one POST to the run's cancel route, then settles to Cancelled", async () => {
    mock.codeMode.putRun(record());
    show(record());
    await userEvent.click(await screen.findByRole("button", { name: "Cancel run" }));
    await waitFor(() => expect(footer()).toHaveTextContent("Cancelled"));
    expect(posts()).toHaveLength(1);
    expect(String(posts()[0]?.[0])).toBe(CANCEL_PATH);
    expect(cancelButton()).toBeNull();
  });

  it("shows a disabled spinner and Cancelling while the request is out, with the block otherwise unchanged", async () => {
    mock.codeMode.putRun(record());
    let release!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const base = vi.mocked(fetch).getMockImplementation()!;
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (init?.method === "POST" && String(input).endsWith("/cancel")) await held;
      return base(input, init);
    });
    show(record());
    await userEvent.click(await screen.findByRole("button", { name: "Cancel run" }));
    const cancelling = await screen.findByRole("button", { name: "Cancelling" });
    expect(cancelling).toBeDisabled();
    expect(cancelling).toHaveAttribute("aria-busy", "true");
    expect(footer()).toHaveTextContent("Running");
    expect(posts()).toHaveLength(1);
    await act(async () => release());
    await waitFor(() => expect(footer()).toHaveTextContent("Cancelled"));
  });

  it("says the run already ended on 409, reloads it, and offers the button again", async () => {
    mock.codeMode.putRun(record());
    show(record());
    const button = await screen.findByRole("button", { name: "Cancel run" });
    mock.fail(CANCEL_PATH, 1, 409, "run_finished", "This run already ended.");
    await userEvent.click(button);
    expect(await screen.findByRole("alert")).toHaveTextContent("This run already ended.");
    expect(await screen.findByRole("button", { name: "Cancel run" })).toBeEnabled();
    await waitFor(() => expect(reads().length).toBeGreaterThanOrEqual(2));
  });

  it("says the person is needed at the desktop app on 403, and offers the button again", async () => {
    mock.codeMode.putRun(record());
    show(record());
    const button = await screen.findByRole("button", { name: "Cancel run" });
    mock.fail(CANCEL_PATH, 1, 403, "person_required", "This action needs the person at the desktop app.");
    await userEvent.click(button);
    expect(await screen.findByRole("alert")).toHaveTextContent("This action needs the person at the desktop app.");
    expect(await screen.findByRole("button", { name: "Cancel run" })).toBeEnabled();
  });

  it("reloads a run that answered 404 silently", async () => {
    mock.codeMode.putRun(record());
    show(record());
    const button = await screen.findByRole("button", { name: "Cancel run" });
    mock.fail(CANCEL_PATH, 1, 404, "not_found", "That run is gone.");
    await userEvent.click(button);
    expect(await screen.findByRole("button", { name: "Cancel run" })).toBeEnabled();
    expect(screen.queryByRole("alert")).toBeNull();
    await waitFor(() => expect(reads().length).toBeGreaterThanOrEqual(2));
  });

  it("shows another failure's own message and offers the button again", async () => {
    mock.codeMode.putRun(record());
    show(record());
    const button = await screen.findByRole("button", { name: "Cancel run" });
    mock.fail(CANCEL_PATH, 1, 503, "unavailable", "The service is restarting.");
    await userEvent.click(button);
    expect(await screen.findByRole("alert")).toHaveTextContent("The service is restarting.");
    expect(await screen.findByRole("button", { name: "Cancel run" })).toBeEnabled();
  });
});
