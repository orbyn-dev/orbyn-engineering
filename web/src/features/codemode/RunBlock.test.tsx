import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import { arrangeTimeline, groupWork } from "../../product/timeline/timeline";
import type { ToolItem } from "../../product/timeline/types";
import { forgetOpenState, OpenStateScope } from "../../ui/openState";
import { installMockFetch } from "../../test-support";
import { TimelineRow } from "../thread/items";
import { TimelineThread } from "../thread/timelineThread";
import { resetCodeRuns } from "./useCodeRun";
import type { CodeRunRecord } from "./types";

void arrangeTimeline;

const NAME = "mcp__orbyn_engineering__orbyn_code_run";
const SCRIPT = [
  "const all = await tools.orbyn_thread_list({});",
  "const failed = [];",
  "for (const t of all) {",
  "  const r = await tools.orbyn_thread_read({ thread_id: t.id });",
  '  if (r.status === "failed") failed.push(t.id);',
  "}",
  "console.log(failed.length + ' of ' + all.length + ' failed');",
  "await tools.orbyn_thread_send({ thread_id: failed[0], prompt: 'Re-run.' });",
  "return failed;",
  "// 10",
  "// 11",
  "// 12",
].join("\n");

const call = (index: number, extra: Record<string, unknown> = {}) => ({ index, tool: "orbyn_thread_read", status: "ok", preview: '{"id":"t_41","status":"done"}', duration_ms: 200, ...extra });

const record = (extra: Partial<CodeRunRecord> = {}): CodeRunRecord => ({
  run_id: "run_7c41e2",
  status: "completed",
  script: SCRIPT,
  script_sha256: "3f9a1c0000000000000000000000000000000000000000000000000000000e07",
  started_at: "2026-11-21T10:00:00.000Z",
  ended_at: "2026-11-21T10:00:03.100Z",
  calls: [call(1, { tool: "orbyn_thread_list", preview: "3 threads", duration_ms: 100 }), call(2), call(3)],
  logs: ["1 of 3 failed"],
  result: ["t_52"],
  ...extra,
});

const item = (reply: unknown | null, extra: Partial<ToolItem> = {}, args: unknown = { code: SCRIPT }): ToolItem => ({
  kind: "tool",
  key: `tool:run:${Math.random()}`,
  callId: "c",
  seq: 1,
  runId: "r",
  ts: "2026-11-21T10:00:00.000Z",
  endedAt: "2026-11-21T10:00:03.100Z",
  name: NAME,
  args: JSON.stringify(args),
  output: "",
  result: reply === null ? null : JSON.stringify(reply),
  done: reply !== null,
  exitCode: null,
  ...extra,
});

function Trail({ items, live = false }: { items: ToolItem[]; live?: boolean }) {
  return (
    <TimelineThread value="t1">
      <OpenStateScope value="t1">
        {groupWork(items).map((entry) => (
          <TimelineRow key={entry.key} entry={entry} live={live} tools={[]} />
        ))}
      </OpenStateScope>
    </TimelineThread>
  );
}

let mock: MockApi;
beforeEach(() => {
  forgetOpenState();
  resetCodeRuns();
  mock = installMockFetch();
});
afterEach(() => vi.restoreAllMocks());

const open = async () => {
  const row = document.querySelector<HTMLButtonElement>("button.tcr")!;
  if (row.getAttribute("aria-expanded") === "false") await act(async () => row.click());
  return row;
};

const runLine = () => document.querySelector(".cm .cx");
const reply = (status: string, more: Record<string, unknown> = {}) => ({ status, runId: "run_7c41e2", ...more });

describe("the run card, completed", () => {
  it("shows the run id, the script excerpt, the calls, the logs, the result and the footer", async () => {
    mock.codeMode.putRun(record());
    render(<Trail items={[item(reply("completed", { result: ["t_52"], logs: ["1 of 3 failed"] }))]} />);
    const row = await open();
    expect(row).toHaveTextContent("orbyn_engineering");
    expect(row).toHaveTextContent("orbyn_code_run");
    await waitFor(() => expect(row).toHaveTextContent("· 3 calls"));
    expect(row).toHaveTextContent("3.1s");
    expect(runLine()).toHaveTextContent("Run ID run_7c41e2");
    expect(screen.getByText(/^Script · 12 lines · \d+ B$/)).toBeInTheDocument();
    expect(screen.getByText("sha256 3f9a1c…e07")).toBeInTheDocument();
    // Five lines closed, never the whole script.
    expect(screen.getByLabelText("Script").textContent).toContain("01  const all");
    expect(screen.getByLabelText("Script").textContent).not.toContain("// 12");
    const calls = await screen.findByRole("group", { name: "Calls" });
    expect(within(calls).getByText("Calls · 3")).toBeInTheDocument();
    expect(within(calls).getByText("3 threads")).toBeInTheDocument();
    expect(within(calls).getByText("0.1s")).toBeInTheDocument();
    expect(screen.getByText("Logs · 1")).toBeInTheDocument();
    expect(screen.getByText("1 of 3 failed")).toBeInTheDocument();
    expect(screen.getByText('["t_52"]')).toBeInTheDocument();
    expect(screen.getByText(/Completed/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Raw result" })).toBeInTheDocument();
  });

  it("opens the whole script in a box that scrolls and folds it back", async () => {
    mock.codeMode.putRun(record());
    render(<Trail items={[item(reply("completed"))]} />);
    await open();
    await userEvent.click(await screen.findByRole("button", { name: "Show all 12 lines" }));
    expect(screen.getByLabelText("Script")).toHaveClass("cm-scr");
    expect(screen.getByLabelText("Script").textContent).toContain("// 12");
    await userEvent.click(screen.getByRole("button", { name: "Hide script" }));
    expect(screen.getByLabelText("Script").textContent).not.toContain("// 12");
  });

  it("says how long a run waited for a person and that it was resumed", async () => {
    mock.codeMode.putRun(record({ resumes: 1, waited_ms: 52_000, ended_at: "2026-11-21T10:01:04.000Z" }));
    render(<Trail items={[item(reply("completed"))]} />);
    const row = await open();
    await waitFor(() => expect(row).toHaveTextContent("1m 04s"));
    expect(runLine()).toHaveTextContent("resumed once");
    expect(screen.getByText(/waited 52s for you/)).toBeInTheDocument();
  });

  it("draws from the engine's reply alone when the record cannot be read, without a call list", async () => {
    render(<Trail items={[item(reply("completed", { result: { a: 1 }, logs: ["x"] }))]} />);
    await open();
    expect(await screen.findByText('{"a":1}')).toBeInTheDocument();
    expect(screen.getByText("x")).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Calls" })).toBeNull();
    // The script still comes from the call's own arguments.
    expect(screen.getByText(/^Script · 12 lines · \d+ B$/)).toBeInTheDocument();
  });
});

describe("the run card, running", () => {
  it("shows the script and a spinner while the first slice of a new run is out, before any run id is known", async () => {
    render(<Trail items={[item(null, { endedAt: undefined })]} live />);
    const row = await open();
    expect(row).toHaveClass("s-run");
    expect(row).toHaveTextContent("· 0 calls so far");
    expect(screen.getByText(/^Script · 12 lines/)).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Calls" })).toBeNull();
  });

  it("spins, lists the call that is still running and counts the calls so far", async () => {
    mock.codeMode.putRun(record({ status: "running", ended_at: null, started_at: new Date(Date.now() - 6_000).toISOString(), calls: [call(1), call(2), call(3, { status: "running", preview: null, duration_ms: null })], result: undefined, logs: [] }));
    render(<Trail items={[item(null, { endedAt: undefined }, { run_id: "run_7c41e2" })]} live />);
    const row = await open();
    await waitFor(() => expect(row).toHaveTextContent("· 3 calls so far"));
    expect(row).toHaveClass("s-run");
    expect(row.querySelector(".spin")).not.toBeNull();
    expect(await screen.findByText("Logs · 0")).toBeInTheDocument();
    expect(screen.getByText("No output.")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Running" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Raw result" })).toBeNull();
  });
});

describe("the run card, failed", () => {
  const fail = (kind: string, message: string, line?: number, more: Partial<CodeRunRecord> = {}) =>
    record({ status: "error", error: { kind: kind as "syntax", message, ...(line ? { line } : {}) }, result: undefined, ...more });

  it("names a syntax error and its line, and shows the script open so the line can be found", async () => {
    mock.codeMode.putRun(fail("syntax", "Unexpected token “)”", 7, { calls: [], logs: [] }));
    render(<Trail items={[item(reply("error", { error: { kind: "syntax", message: "Unexpected token “)”", line: 7 } }))]} />);
    const row = await open();
    await waitFor(() => expect(row).toHaveTextContent("· syntax error"));
    expect(row).toHaveClass("s-fail");
    expect(row).toHaveTextContent("Failed");
    expect(await screen.findByText(/^Error ·/)).toHaveTextContent("Error · syntax · line 7");
    expect(screen.getByText("Unexpected token “)”")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Hide script" })).toBeInTheDocument();
    expect(screen.getByText(/Failed/, { selector: ".tfm *" })).toBeInTheDocument();
  });

  it.each([
    ["runtime", "TypeError: Cannot read properties of undefined", 4, "· runtime error", "Error · runtime · line 4"],
    ["limit", "A script may make 200 calls.", undefined, "· limit", "Error · limit"],
    ["denied", "orbyn_thread_send was declined.", undefined, "· denied", "Error · denied"],
  ] as const)("shows a %s error", async (kind, message, line, note, heading) => {
    mock.codeMode.putRun(fail(kind, message, line));
    render(<Trail items={[item(reply("error", { error: { kind, message, ...(line ? { line } : {}) } }))]} />);
    const row = await open();
    await waitFor(() => expect(row).toHaveTextContent(note));
    expect(await screen.findByText(/^Error ·/)).toHaveTextContent(heading);
    expect(screen.getByText(message)).toBeInTheDocument();
  });

  it("shows a declined call as Declined with the gate's reason", async () => {
    mock.codeMode.putRun(fail("denied", "orbyn_thread_send was declined.", undefined, { calls: [call(1), call(5, { tool: "orbyn_thread_send", status: "denied", preview: null })] }));
    render(<Trail items={[item(reply("error", { error: { kind: "denied", message: "orbyn_thread_send was declined." } }))]} />);
    await open();
    expect(await screen.findByText("Declined")).toBeInTheDocument();
  });

  const LOST = "This run was lost when Orbyn restarted. Call orbyn_code_run again with the same code and run_id to replay it.";
  const recorded = [call(1), call(2), call(3), call(4)];
  const footer = () => document.querySelector(".cf .tfm");

  it("says a lost run was lost, shows the core’s message as it is and the calls the record holds, with nothing to press", async () => {
    mock.codeMode.putRun(fail("lost", LOST, undefined, { calls: recorded }));
    render(<Trail items={[item(reply("error", { error: { kind: "lost", message: "x" } }))]} />);
    const row = await open();
    await waitFor(() => expect(row).toHaveTextContent("· lost"));
    expect(row).toHaveTextContent("Lost");
    expect(await screen.findByText(LOST)).toBeInTheDocument();
    expect(screen.getByText(/Error ·/)).toHaveTextContent("Error · lost");
    expect(screen.getByRole("group", { name: "Calls" })).toHaveTextContent("Calls · 4");
    await waitFor(() => expect(footer()).toHaveTextContent("Lost · 4 calls recorded"));
    expect(screen.queryByText(/Resume replays/)).toBeNull();
    expect(screen.queryByRole("button", { name: /resume|replay|retry/i })).toBeNull();
  });

  it("shows a replay that could not match with the core’s own message, nothing repeated", async () => {
    const changed = "The script changed or is not repeatable, so the recorded calls can't be replayed.";
    mock.codeMode.putRun(fail("lost", changed, undefined, { calls: recorded }));
    render(<Trail items={[item(reply("error", { error: { kind: "lost", message: "x" } }))]} />);
    await open();
    expect(await screen.findByText(changed)).toBeInTheDocument();
    await waitFor(() => expect(footer()).toHaveTextContent("Lost · 4 calls recorded"));
  });

  it("marks a call that was running at the restart, counts it as unknown and never as done", async () => {
    const inflight = "A call that may have changed something was running when Orbyn restarted, so the run can't continue.";
    mock.codeMode.putRun(fail("lost", inflight, undefined, { calls: [...recorded, call(5, { tool: "orbyn_thread_send", status: "in_flight", preview: null })] }));
    render(<Trail items={[item(reply("error", { error: { kind: "lost", message: "x" } }))]} />);
    await open();
    expect(await screen.findByText(inflight)).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Calls" })).toHaveTextContent("Calls · 5");
    expect(screen.getByText("Running when Orbyn restarted")).toBeInTheDocument();
    await waitFor(() => expect(footer()).toHaveTextContent("Lost · 4 calls done, 1 unknown"));
    expect(screen.queryByText(/Resume replays/)).toBeNull();
  });
});

describe("the run card, paused", () => {
  it("is amber, says what the run waits on and offers no script or raw result", async () => {
    mock.codeMode.putRun(record({ status: "paused", ended_at: null, started_at: new Date(Date.now() - 42_000).toISOString(), calls: [call(1), call(5, { tool: "orbyn_thread_send", status: "waiting", preview: null })], pending: [{ tool: "orbyn_thread_send", call: 5 }], logs: [], result: undefined }));
    render(<Trail items={[item(reply("paused", { pending: [{ tool: "orbyn_thread_send", call: 5 }] }))]} />);
    const row = await open();
    await waitFor(() => expect(row).toHaveClass("s-wait"));
    expect(row).toHaveTextContent("Paused");
    expect(runLine()).toHaveTextContent("· paused");
    expect(await screen.findByText("Waiting on · 1")).toBeInTheDocument();
    expect(screen.getByText("call 5 · waiting for approval")).toBeInTheDocument();
    expect(screen.queryByLabelText("Script")).toBeNull();
    expect(screen.queryByRole("button", { name: "Raw result" })).toBeNull();
  });
});

describe("long content never builds an unbounded tree", () => {
  it("shows the first calls and one line for the rest, and all of them on request", async () => {
    mock.codeMode.putRun(record({ calls: Array.from({ length: 112 }, (_, at) => call(at + 1)) }));
    render(<Trail items={[item(reply("completed"))]} />);
    await open();
    const list = await screen.findByRole("group", { name: "Calls" });
    expect(within(list).getByText("Calls · 112")).toBeInTheDocument();
    expect(list.querySelectorAll(".cm-c")).toHaveLength(4);
    await userEvent.click(within(list).getByRole("button", { name: "109 more calls · Show all" }));
    expect(list.querySelectorAll(".cm-c")).toHaveLength(112);
  });

  it("caps a call preview to one short line and says how much of a large result was kept", async () => {
    mock.codeMode.putRun(record({ calls: [call(1, { preview: `{"id":"t_41",${"x".repeat(5000)}`, result_bytes: 184_320 })] }));
    render(<Trail items={[item(reply("completed"))]} />);
    await open();
    const list = await screen.findByRole("group", { name: "Calls" });
    expect(list.querySelector(".cr-t")!.textContent!.length).toBeLessThanOrEqual(160);
    expect(list.querySelector(".cr-n")).toHaveTextContent("… 4 KiB of 180 KiB");
  });

  it("opens a 400-line script one page at a time, 38 KiB of the 64 KiB allowed", async () => {
    const big = Array.from({ length: 412 }, (_, at) => `// line ${at + 1} ${"x".repeat(88)}`).join("\n");
    mock.codeMode.putRun(record({ script: big }));
    render(<Trail items={[item(reply("completed"), {}, { code: big })]} />);
    await open();
    const button = await screen.findByRole("button", { name: /^Show all 412 lines · \d+ KiB of 64 KiB$/ });
    await userEvent.click(button);
    expect(document.querySelectorAll(".cm-line")).toHaveLength(400);
    await userEvent.click(screen.getByRole("button", { name: "Show 12 more lines" }));
    expect(document.querySelectorAll(".cm-line")).toHaveLength(412);
  });

  it("keeps the newest 500 log lines and says the older ones were dropped", async () => {
    const logs = Array.from({ length: 700 }, (_, at) => `checked thread ${at}`);
    mock.codeMode.putRun(record({ logs, logs_total: 1204, truncated: true }));
    render(<Trail items={[item(reply("completed", { truncated: true }))]} />);
    await open();
    expect(await screen.findByText("Logs · 500")).toBeInTheDocument();
    expect(screen.getByText("Older lines were dropped. Showing the last 500 of 1,204.")).toBeInTheDocument();
    const text = document.querySelector("pre.co.tail")!.textContent!;
    expect(text.split("\n")).toHaveLength(500);
    expect(text.endsWith("checked thread 699")).toBe(true);
  });

  it("says so when the reply's result was cut, and shows the preview as text", async () => {
    const cut = { truncated: true, bytes: 311_296, preview: '[{"id":"t_41"' };
    mock.codeMode.putRun(record({ result: cut, truncated: true }));
    render(<Trail items={[item(reply("completed", { result: cut, truncated: true }))]} />);
    await open();
    expect(await screen.findByText("The result was over 256 KiB. This is the first 64 KiB, as text.")).toBeInTheDocument();
  });
});

describe("a script and its results are untrusted", () => {
  const hostile = '<img src=x onerror="window.__owned=1"> https://evil.example/login <script>window.__owned=1</script>';

  it("renders script, logs, result and call previews as text: no markup, no link, nothing opened", async () => {
    const open_ = vi.spyOn(window, "open").mockImplementation(() => null);
    mock.codeMode.putRun(record({ script: `return "${hostile}";`, logs: [hostile], result: hostile, calls: [call(1, { preview: hostile })] }));
    render(<Trail items={[item(reply("completed", { result: hostile }), {}, { code: `return "${hostile}";` })]} />);
    await open();
    await screen.findByText("Calls · 1");
    expect(document.querySelector(".cb img")).toBeNull();
    expect(document.querySelector(".cb script")).toBeNull();
    expect(document.querySelector(".cb a")).toBeNull();
    expect((window as unknown as { __owned?: number }).__owned).toBeUndefined();
    expect(open_).not.toHaveBeenCalled();
    expect(screen.getAllByText(/evil\.example/).length).toBeGreaterThan(0);
  });

  it("never writes a script or a result to the console", async () => {
    const spies = (["log", "info", "warn", "error", "debug"] as const).map((level) => vi.spyOn(console, level).mockImplementation(() => undefined));
    mock.codeMode.putRun(record({ script: "return 'TOP-SECRET-SCRIPT';", result: "TOP-SECRET-RESULT" }));
    render(<Trail items={[item(reply("completed", { result: "TOP-SECRET-RESULT" }), {}, { code: "return 'TOP-SECRET-SCRIPT';" })]} />);
    await open();
    await screen.findByText("Calls · 3");
    for (const spy of spies) for (const args of spy.mock.calls) expect(JSON.stringify(args)).not.toMatch(/TOP-SECRET/);
  });
});

describe("the other tools of the server", () => {
  it("draws an ordinary Orbyn tool as it always was", async () => {
    const plain = item(null, { name: "mcp__orbyn_engineering__orbyn_thread_read", result: '{"id":"t_41"}', done: true }, { thread_id: "t_41" });
    render(<Trail items={[plain]} />);
    const row = document.querySelector("button.tcr")!;
    expect(row).toHaveTextContent("orbyn_thread_read");
    expect(row).not.toHaveClass("s-wait");
  });
});

describe("a run id the model chose", () => {
  it.each(["__proto__", "constructor", "toString"])("does not crash the card for %s", async (id) => {
    render(<Trail items={[item(null, { endedAt: undefined }, { run_id: id })]} live />);
    const row = await open();
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).includes("/api/code-runs/"))).toBe(true));
    expect(row).toHaveClass("s-run");
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(document.querySelector("button.tcr")).not.toBeNull();
  });

  it("does not crash when the reply names such a run", async () => {
    render(<Trail items={[item({ status: "completed", runId: "__proto__", result: 1 })]} />);
    await open();
    expect(await screen.findByText("1")).toBeInTheDocument();
  });
});

describe("text the model controls is capped", () => {
  it("cuts an error message and a script with no record, and keeps keys unique", async () => {
    const long = "e".repeat(50_000);
    render(<Trail items={[item({ status: "error", runId: "run_x", error: { kind: "runtime", message: long } }, {}, { code: "x\n".repeat(100_000) })]} />);
    await open();
    const err = await screen.findByText(/^e+…?$/);
    expect(err.textContent!.length).toBeLessThanOrEqual(4_100);
    expect(document.querySelectorAll(".cm-line").length).toBeLessThanOrEqual(5);
    expect(screen.getByLabelText("Script").textContent!.length).toBeLessThan(2_000);
  });
});
