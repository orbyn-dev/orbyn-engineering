import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HOSTILE } from "../../runpage/fixture";
import { taskGates, unfinishedTaskGates } from "./fixture";
import { TaskGates } from "./TaskGates";

const answer = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
const read = () => Promise.resolve(answer(taskGates()));

afterEach(() => vi.unstubAllGlobals());

describe("the gates view", () => {
  it("asks for the task's own read and says it is reading", async () => {
    const fetch = vi.fn((_input: RequestInfo | URL, _init?: RequestInit) => new Promise<Response>(() => undefined));
    vi.stubGlobal("fetch", fetch);
    render(<TaskGates id="task-redirect" onBack={() => undefined} />);
    expect(screen.getByRole("status")).toHaveTextContent("Reading the task’s gates");
    await waitFor(() => expect(fetch).toHaveBeenCalledTimes(1));
    expect(fetch.mock.calls[0]![0]).toBe("/api/crew/flow/tasks/task-redirect");
  });

  it("draws one card per served gate: its stage and decision, who decided, the lane, the reason and the time", async () => {
    vi.stubGlobal("fetch", vi.fn(read));
    render(<TaskGates id="task-redirect" name="Use next in the auth redirect" onBack={() => undefined} />);
    const view = await screen.findByRole("region", { name: "Gates" });
    expect(view).toHaveTextContent("Gates");
    expect(view).toHaveTextContent("Use next in the auth redirect");
    const cards = [...view.querySelectorAll<HTMLElement>(".cx-fbx")];
    expect(cards).toHaveLength(4);
    // The design's own card and decided-gate classes carry each card.
    expect(view.querySelectorAll(".cx-fgt.cx-done")).toHaveLength(4);
    // A card is titled by its served stage and decision, in the design's words.
    expect(cards[0]).toHaveTextContent("Checks → passed");
    expect(cards[1]).toHaveTextContent("Checks → passed");
    expect(cards[2]).toHaveTextContent("Finding check → confirmed");
    expect(cards[3]).toHaveTextContent("Review → passed");
    // Who decided it, in the design's words: the code, a confirmed finding or you.
    expect(cards[0]).toHaveTextContent("Decided by");
    expect(cards[0]).toHaveTextContent("Code");
    expect(cards[1]).toHaveTextContent("You");
    expect(cards[2]).toHaveTextContent("A confirmed finding");
    expect(cards[3]).toHaveTextContent("Code");
    // The lane only when the gate names one.
    expect(cards[3]).toHaveTextContent("Lane");
    expect(cards[3]).toHaveTextContent("security");
    expect(cards[0]).not.toHaveTextContent("Lane");
    expect(cards[2]).not.toHaveTextContent("Lane");
    // The reason the service serves for a task that is over.
    expect(cards[0]).toHaveTextContent("3 of 3 checks ran on the commit and passed");
    expect(cards[1]).toHaveTextContent("A person checked this, and it holds.");
    expect(cards[2]).toHaveTextContent("A repro failed on the commit, so the finding blocks.");
    expect(cards[3]).toHaveTextContent("This lane read the current head and no finding holds it.");
    // The decision time.
    for (const card of cards) expect(card).toHaveTextContent(/Nov/);
    // The design's red card marks the gate that blocks; a passed gate stays plain.
    expect(cards[0]!.className).not.toContain("cx-red");
    expect(cards[1]!.className).not.toContain("cx-red");
    expect(cards[2]!.className).toContain("cx-red");
    expect(cards[3]!.className).not.toContain("cx-red");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("hides a gate's reason while the task is not over, even if the service sends one", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(unfinishedTaskGates()))));
    render(<TaskGates id="task-redirect" onBack={() => undefined} />);
    const view = await screen.findByRole("region", { name: "Gates" });
    expect(view).toHaveTextContent("Checks → passed");
    expect(view.querySelectorAll(".cx-fbx")).toHaveLength(4);
    expect(view).not.toHaveTextContent("A reason for a task that is not over");
    expect(document.body.textContent).not.toContain("A reason for a task that is not over");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("says there are no gate decisions yet, and draws no card", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(taskGates({ gates: [] })))));
    render(<TaskGates id="task-redirect" onBack={() => undefined} />);
    const view = await screen.findByRole("region", { name: "Gates" });
    expect(view).toHaveTextContent("No gate decisions yet.");
    expect(view.querySelectorAll(".cx-fbx")).toHaveLength(0);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("says a list was cut at the read's cap", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(taskGates({ truncated: true })))));
    render(<TaskGates id="task-redirect" onBack={() => undefined} />);
    expect(await screen.findByRole("note")).toHaveTextContent("A list of this task was cut at its cap; the newest are shown.");
  });

  it("says a service without the read has none, without an error, and keeps the Planned marks", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer({ code: "not_found", message: "not found" }, 404))));
    render(<TaskGates id="task-redirect" onBack={() => undefined} />);
    expect(await screen.findByText("This service has no gates read")).toBeVisible();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText(/stay Planned/)).toBeVisible();
  });

  it("reports any other failure beside the task and Retry reads it again", async () => {
    const user = userEvent.setup();
    const fetch = vi.fn()
      .mockResolvedValueOnce(answer({ code: "unavailable", message: "The gates service is busy." }, 503))
      .mockResolvedValueOnce(answer(taskGates()));
    vi.stubGlobal("fetch", fetch);
    render(<TaskGates id="task-redirect" onBack={() => undefined} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Can’t read this task’s gates");
    expect(screen.getByRole("alert")).toHaveTextContent("The gates service is busy.");
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect((await screen.findAllByText("Checks → passed")).length).toBeGreaterThan(0);
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("puts a hostile stage, decision, decider, lane, reason or time on the page as text", async () => {
    const hostile = taskGates({
      gates: [{ stage: HOSTILE, decision: HOSTILE, decided_by: HOSTILE, lane: HOSTILE, reason: HOSTILE, at: HOSTILE }],
    });
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(hostile))));
    const { container } = render(<TaskGates id="task-redirect" onBack={() => undefined} />);
    await screen.findByRole("region", { name: "Gates" });
    expect(container.querySelector("img, script")).toBeNull();
    expect(container.innerHTML).not.toContain("<img src=x");
    expect(container).toHaveTextContent(HOSTILE);
  });

  it("goes back to the task", async () => {
    vi.stubGlobal("fetch", vi.fn(read));
    const onBack = vi.fn();
    const user = userEvent.setup();
    render(<TaskGates id="task-redirect" onBack={onBack} />);
    await user.click(await screen.findByRole("button", { name: "Task" }));
    expect(onBack).toHaveBeenCalledTimes(1);
  });
});
