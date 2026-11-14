import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { BoardView } from "../board/BoardView";
import { FlowView } from "../flow/live/FlowView";
import { bodyOf, validTaskId } from "./api";
import { runWrite, useWrites } from "./store";
import { ConfirmWrite } from "./TaskWrite";

let mock: MockApi;
beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-07T15:00:00Z"));
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  useWrites.setState({ ops: {} });
  window.location.hash = "";
});
afterEach(() => vi.useRealTimers());

const fetched = () => vi.mocked(fetch).mock.calls;
const posts = () => fetched().filter(([, init]) => init?.method === "POST");
const nonGets = () => fetched().filter(([, init]) => init?.method && init.method !== "GET");
const boardReads = () => fetched().filter(([url, init]) => String(url).includes("/api/crew/flow/board") && (init?.method ?? "GET") === "GET").length;

async function board(scenario: "crew" | `crew-${string}` = "crew") {
  mock.reset(scenario);
  const user = userEvent.setup();
  render(<BoardView />);
  await screen.findByRole("region", { name: /^Queued/ });
  return user;
}
const card = (column: RegExp, name: RegExp) => within(screen.getByRole("region", { name: column })).getByRole("button", { name });
async function choose(user: ReturnType<typeof userEvent.setup>, column: RegExp, task: RegExp, item: RegExp) {
  await user.click(card(column, task));
  await user.click(within(await screen.findByRole("menu", { name: "Task actions" })).getByRole("menuitem", { name: item }));
}
const confirmCancel = () => screen.getByRole("button", { name: "Cancel task" });
const confirmRerun = () => screen.getByRole("button", { name: "Re-run checks" });

/** Holds every write until `release` is called, to see the busy state. */
function hold() {
  const real = vi.mocked(fetch).getMockImplementation()!;
  let release: () => void = () => undefined;
  const gate = new Promise<void>((resolve) => (release = resolve));
  vi.mocked(fetch).mockImplementation(async (input, init) => {
    if (init?.method === "POST") await gate;
    return real(input, init);
  });
  return release;
}

describe("the body of a write", () => {
  it("is nothing without a reason, and a trimmed reason of at most 200 characters", () => {
    expect(bodyOf("")).toBeUndefined();
    expect(bodyOf("   ")).toBeUndefined();
    expect(bodyOf("  too slow ")).toEqual({ reason: "too slow" });
    expect(bodyOf("x".repeat(300))).toEqual({ reason: "x".repeat(200) });
  });
});

describe("Cancel on a Taskboard card", () => {
  it("asks first: nothing is sent before the confirm, and Not now, Escape and a closed dialog send nothing", async () => {
    const user = await board();
    await choose(user, /^Running/, /^Actions for Use next/, /^Cancel$/);
    expect(await screen.findByRole("heading", { name: "Cancel this task?" })).toBeVisible();
    expect(screen.getByText("Its worker is stopped and the task ends as cancelled.")).toBeVisible();
    expect(nonGets()).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Not now" }));
    expect(screen.queryByRole("heading", { name: "Cancel this task?" })).toBeNull();
    await choose(user, /^Running/, /^Actions for Use next/, /^Cancel$/);
    await screen.findByRole("heading", { name: "Cancel this task?" });
    // Escape is the dialog's own cancel event.
    fireEvent(document.querySelector("dialog")!, new Event("cancel", { cancelable: true }));
    await waitFor(() => expect(document.querySelector("dialog")).toBeNull());
    expect(nonGets()).toHaveLength(0);
  });

  it("sends one POST with no body when there is no reason, then reads the board again and says it is cancelled", async () => {
    const user = await board();
    const before = boardReads();
    await choose(user, /^Running/, /^Actions for Use next/, /^Cancel$/);
    await user.click(await screen.findByRole("button", { name: "Cancel task" }));
    expect(await screen.findByText(/^Cancelled\./)).toBeVisible();
    expect(posts()).toHaveLength(1);
    const [url, init] = posts()[0]!;
    expect(String(url)).toBe("/api/crew/tasks/b-running/cancel");
    expect(init?.body).toBeUndefined();
    expect(new Headers(init?.headers).get("Content-Type")).toBeNull();
    await waitFor(() => expect(boardReads()).toBeGreaterThan(before));
  });

  it("sends the reason when there is one, and the field stops at 200 characters", async () => {
    const user = await board();
    await choose(user, /^Running/, /^Actions for Use next/, /^Cancel$/);
    const field = await screen.findByRole("textbox", { name: /Reason/ });
    expect(field).toHaveAttribute("maxlength", "200");
    await user.type(field, "wrong branch");
    await user.click(confirmCancel());
    await screen.findByText(/^Cancelled\./);
    expect(posts()).toHaveLength(1);
    expect(JSON.parse(String(posts()[0]![1]?.body))).toEqual({ reason: "wrong branch" });
  });

  it("sends one POST for a double click on the confirm", async () => {
    const user = await board();
    await choose(user, /^Running/, /^Actions for Use next/, /^Cancel$/);
    await user.dblClick(await screen.findByRole("button", { name: "Cancel task" }));
    await screen.findByText(/^Cancelled\./);
    expect(posts()).toHaveLength(1);
  });

  it("shows it is on its way, and the item is off meanwhile", async () => {
    const user = await board();
    const release = hold();
    await choose(user, /^Running/, /^Actions for Use next/, /^Cancel$/);
    await user.click(await screen.findByRole("button", { name: "Cancel task" }));
    expect(await screen.findByText("Cancelling…")).toBeVisible();
    await user.click(card(/^Running/, /^Actions for Use next/));
    expect(within(await screen.findByRole("menu", { name: "Task actions" })).getByRole("menuitem", { name: /^Cancel/ })).toBeDisabled();
    await user.keyboard("{Escape}");
    release();
    expect(await screen.findByText(/^Cancelled\./)).toBeVisible();
    expect(posts()).toHaveLength(1);
  });

  it("says a task already cancelled is already cancelled", async () => {
    const user = await board("crew-write-already");
    await choose(user, /^Running/, /^Actions for Use next/, /^Cancel$/);
    await user.click(await screen.findByRole("button", { name: "Cancel task" }));
    expect(await screen.findByText(/^Already cancelled\./)).toBeVisible();
  });

  it.each([
    ["crew-write-ended", "This task already ended."],
    // The service's own two messages for a 502, copied from crates/orbyn-core/src/crew/cancel.rs: shown as they are, and nothing added.
    ["crew-write-engine", "The task was not cancelled. Try again."],
    ["crew-write-engine-cancelled", "The task is cancelled, but its worker could not be stopped. Try again."],
  ])("shows a refusal (%s) in the service's words and does not retry", async (scenario, message) => {
    const user = await board(scenario as `crew-${string}`);
    await choose(user, /^Running/, /^Actions for Use next/, /^Cancel$/);
    await user.click(await screen.findByRole("button", { name: "Cancel task" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toBe(`${message}Dismiss`);
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(posts()).toHaveLength(1);
    await user.click(within(alert).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it.each([
    ["a 502 with a message it does not know", () => new Response(JSON.stringify({ code: "engine_unavailable", message: "The engine said something else." }), { status: 502 })],
    ["a 502 with another code", () => new Response(JSON.stringify({ code: "bad_gateway", message: "The task was not cancelled. Try again." }), { status: 502 })],
    ["a proxy 502 with an HTML body", () => new Response("<html><body>Bad Gateway</body></html>", { status: 502, headers: { "Content-Type": "text/html" } })],
    ["a 502 with an empty body", () => new Response(null, { status: 502 })],
  ])("says only to check the task for %s, and never that it is or is not cancelled", async (_name, response) => {
    const user = await board();
    const real = vi.mocked(fetch).getMockImplementation()!;
    vi.mocked(fetch).mockImplementation(async (input, init) => (init?.method === "POST" ? response() : real(input, init)));
    await choose(user, /^Running/, /^Actions for Use next/, /^Cancel$/);
    await user.click(await screen.findByRole("button", { name: "Cancel task" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toBe("Check the task’s state before trying again.Dismiss");
    expect(alert.textContent).not.toMatch(/cancelled/i);
  });

  it("offers Cancel only for a task the card's flag allows, and is off for a remote window", async () => {
    const user = await board();
    await user.click(await openPassed(user));
    const menu = await screen.findByRole("menu", { name: "Task actions" });
    expect(within(menu).queryByRole("menuitem", { name: /^Cancel/ })).toBeNull();
    expect(within(menu).getByRole("menuitem", { name: /^Re-run checks/ })).toBeEnabled();
    await user.keyboard("{Escape}");
    useConnection.setState({ remote: true });
    await user.click(card(/^Running/, /^Actions for Use next/));
    const remote = within(await screen.findByRole("menu", { name: "Task actions" })).getByRole("menuitem", { name: /^Cancel/ });
    expect(remote).toBeDisabled();
    expect(remote).toHaveTextContent("Not available from another computer");
  });
});

async function openPassed(user: ReturnType<typeof userEvent.setup>) {
  await user.click(within(screen.getByRole("region", { name: /^Passed/ })).getByRole("button", { name: /Passed/ }));
  return within(screen.getByRole("region", { name: /^Passed/ })).getAllByRole("button", { name: /^Actions for Fix the typo/ })[0]!;
}

describe("Re-run checks", () => {
  async function rerun(user: ReturnType<typeof userEvent.setup>) {
    await user.click(within(screen.getByRole("region", { name: /^Passed/ })).getByRole("button", { name: /Passed/ }));
    await user.click(within(screen.getByRole("region", { name: /^Passed/ })).getByRole("button", { name: /^Actions for Fix the typo/ }));
    await user.click(within(await screen.findByRole("menu", { name: "Task actions" })).getByRole("menuitem", { name: /^Re-run checks/ }));
  }

  it("asks first with what it does, sends nothing before the confirm, then shows counts and never a command", async () => {
    const user = await board();
    await rerun(user);
    expect(await screen.findByRole("heading", { name: "Re-run this task’s checks?" })).toBeVisible();
    expect(screen.getByText("This runs the checks again on this computer and can take up to 30 minutes.")).toBeVisible();
    expect(nonGets()).toHaveLength(0);
    await user.click(confirmRerun());
    const status = await screen.findByText(/^Checks: 3 of 4 ran/);
    expect(status).toHaveTextContent("Checks: 3 of 4 ran · 2 passed · 1 refused · 1 skipped · verdict fail");
    expect(screen.getByText("Refused: check-2")).toBeVisible();
    expect(posts()).toHaveLength(1);
    const [url, init] = posts()[0]!;
    expect(String(url)).toBe("/api/crew/tasks/b-pass1/rerun-checks");
    expect(init?.body).toBeUndefined();
    expect(document.body.textContent).not.toMatch(/\bnpm\b|\btsc\b|\bcargo\b|exit \d|Proves/);
  });

  it("is a busy state that can last, with every other Re-run off, one POST for a double click and no retry", async () => {
    const user = await board();
    const release = hold();
    await rerun(user);
    await user.dblClick(await screen.findByRole("button", { name: "Re-run checks" }));
    expect(await screen.findByText("Re-running the checks. This can take up to 30 minutes.")).toBeVisible();
    await new Promise((resolve) => setTimeout(resolve, 80));
    expect(posts()).toHaveLength(1);
    release();
    await screen.findByText(/^Checks: 3 of 4 ran/);
    expect(posts()).toHaveLength(1);
  });

  it("shows the service's refusal as it words it, and sends nothing again", async () => {
    const user = await board("crew-write-busy");
    await rerun(user);
    await user.click(await screen.findByRole("button", { name: "Re-run checks" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Another re-run is running. Try again in 7 seconds.");
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(posts()).toHaveLength(1);
  });
});

describe("no write is sent but by a click on a confirm", () => {
  it("sends none on render, on focus, on a reconnect-style re-check or on the retry of a read", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/flow/board", 1);
    const user = userEvent.setup();
    render(<BoardView />);
    await screen.findByText("Can’t read the board");
    window.dispatchEvent(new Event("focus"));
    window.dispatchEvent(new Event("online"));
    document.dispatchEvent(new Event("visibilitychange"));
    await user.click(screen.getByRole("button", { name: "Retry" }));
    await screen.findByRole("region", { name: /^Queued/ });
    expect(nonGets()).toHaveLength(0);
  });
});

describe("Cancel and Re-run checks in the Flow zoom", () => {
  async function zoom(task: RegExp) {
    mock.reset("crew");
    const user = userEvent.setup();
    render(<FlowView />);
    await user.click(within(await screen.findByRole("list", { name: "Tasks" })).getByRole("button", { name: task }));
    await screen.findByRole("heading", { name: task });
    return user;
  }

  it("Cancel is enabled for a task that has not ended, asks first, and sends one POST on the confirm", async () => {
    const user = await zoom(/Use next in the auth redirect/);
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Re-run checks" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByRole("heading", { name: "Cancel this task?" })).toBeVisible();
    expect(nonGets()).toHaveLength(0);
    await user.dblClick(screen.getByRole("button", { name: "Cancel task" }));
    expect(await screen.findByText(/^Cancelled\./)).toBeVisible();
    expect(posts()).toHaveLength(1);
    expect(String(posts()[0]![0])).toBe("/api/crew/tasks/task-redirect/cancel");
    expect(document.body.textContent).not.toContain("Available when the write route ships");
  });

  it("Re-run checks is enabled for a passed task and Cancel is not; the counts show and no command does", async () => {
    const user = await zoom(/Update the login docs/);
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Re-run checks" }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Re-run checks" }));
    expect(await screen.findByText(/^Checks: 3 of 4 ran/)).toBeVisible();
    expect(String(posts()[0]![0])).toBe("/api/crew/tasks/task-merged/rerun-checks");
  });
});

describe("each guard on its own", () => {
  it("runWrite called twice for one task sends one POST", async () => {
    mock.reset("crew");
    const release = hold();
    const first = runWrite("cancel", "b-running", "");
    const second = runWrite("cancel", "b-running", "");
    release();
    await Promise.all([first, second]);
    expect(posts()).toHaveLength(1);
  });

  it("runWrite called for a re-run of two different tasks sends one POST while the first runs", async () => {
    mock.reset("crew");
    const release = hold();
    const first = runWrite("rerun", "b-pass1", "");
    const second = runWrite("rerun", "b-pass2", "");
    release();
    await Promise.all([first, second]);
    expect(posts()).toHaveLength(1);
    // A cancel of another task is not held back by a re-run.
    const release2 = hold();
    const third = runWrite("rerun", "b-pass1", "");
    const cancel = runWrite("cancel", "b-running", "");
    release2();
    await Promise.all([third, cancel]);
    expect(posts().map(([url]) => String(url))).toEqual(["/api/crew/tasks/b-pass1/rerun-checks", "/api/crew/tasks/b-pass1/rerun-checks", "/api/crew/tasks/b-running/cancel"]);
  });

  it("opens with Not now focused, so an Enter pressed at once sends nothing", async () => {
    const user = await board();
    await choose(user, /^Running/, /^Actions for Use next/, /^Cancel$/);
    await screen.findByRole("heading", { name: "Cancel this task?" });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Not now" }));
    await user.keyboard("{Enter}");
    expect(nonGets()).toHaveLength(0);
    expect(document.querySelector("dialog")).toBeNull();
  });

  it("the confirm handler called twice in one tick confirms once", () => {
    const onConfirm = vi.fn();
    render(<ConfirmWrite kind="cancel" name="A task" onClose={() => undefined} onConfirm={onConfirm} />);
    const button = screen.getByRole("button", { name: "Cancel task" });
    fireEvent.click(button);
    fireEvent.click(button);
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});

describe("a task id that would change the route", () => {
  it.each(["..", ".", "a/b", "a?b", "a#b", "", ".hidden", "x".repeat(97), "a b", "a%2Fb"])("is refused before any request: %j", async (id) => {
    expect(validTaskId(id)).toBe(false);
    mock.reset("crew");
    await runWrite("cancel", id, "");
    await runWrite("rerun", id, "");
    expect(fetched()).toHaveLength(0);
    expect(useWrites.getState().ops[id]).toMatchObject({ phase: "failed", message: "That task’s id is not valid." });
    useWrites.setState({ ops: {} });
  });

  it("accepts the ids the service accepts", () => {
    for (const id of ["b-running", "task_1", "a:b", "x.y", "A1", "x".repeat(96)]) expect(validTaskId(id)).toBe(true);
  });
});
