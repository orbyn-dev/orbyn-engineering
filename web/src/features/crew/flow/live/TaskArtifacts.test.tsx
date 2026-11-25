import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HOSTILE } from "../../runpage/fixture";
import { emptyTaskArtifacts, taskArtifacts } from "./fixture";
import { TaskArtifacts } from "./TaskArtifacts";

const answer = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
const read = () => Promise.resolve(answer(taskArtifacts()));

afterEach(() => vi.unstubAllGlobals());

describe("the artifacts view", () => {
  it("asks for the task's own read and says it is reading", async () => {
    const fetch = vi.fn((_input: RequestInfo | URL, _init?: RequestInit) => new Promise<Response>(() => undefined));
    vi.stubGlobal("fetch", fetch);
    render(<TaskArtifacts id="task-redirect" onBack={() => undefined} />);
    expect(screen.getByRole("status")).toHaveTextContent("Reading the task’s artifacts");
    await waitFor(() => expect(fetch).toHaveBeenCalledTimes(1));
    expect(fetch.mock.calls[0]![0]).toBe("/api/crew/flow/tasks/task-redirect/artifacts");
  });

  it("draws the checks, the artifacts they named and the diff pinned to the commit the task started from", async () => {
    vi.stubGlobal("fetch", vi.fn(read));
    render(<TaskArtifacts id="task-redirect" name="Use next in the auth redirect" onBack={() => undefined} />);
    const view = await screen.findByRole("region", { name: "Artifacts" });
    expect(view).toHaveTextContent("Artifacts");
    expect(view).toHaveTextContent("Use next in the auth redirect");
    // The checks bar: each row by its kind and id, never a check's own text, and the commit the checks read.
    expect(view).toHaveTextContent("On commit");
    expect(view).toHaveTextContent("a41c9e2");
    expect(view).toHaveTextContent("Check 1 of 2");
    expect(view).toHaveTextContent("test · check-1 · pass");
    expect(view).toHaveTextContent("Check 2 of 2");
    expect(view).toHaveTextContent("lint · check-2 · fail");
    // The artifacts those rows named, with the hash the store keeps instead of the bytes.
    expect(view).toHaveTextContent("log · pnpm-test-auth.log");
    expect(view).toHaveTextContent("sha256:7e1c4b94a");
    // The diff, pinned to the base commit, as counts.
    expect(view).toHaveTextContent("Pinned to");
    expect(view).toHaveTextContent("0b1e5f0");
    expect(view).toHaveTextContent("2 files · +18 −4");
    // What this read does not carry stays Planned instead of being filled in.
    expect(screen.getByTitle("The artifacts read carries no author")).toBeInTheDocument();
    expect(screen.getByTitle("The artifacts read carries no branch")).toBeInTheDocument();
    expect(screen.getByTitle("The artifacts read carries no file lease")).toBeInTheDocument();
    expect(screen.getByTitle("The artifacts read carries the checks, the artifacts and the change, never the brief")).toBeInTheDocument();
    expect(screen.getByTitle("The artifacts read carries no findings")).toBeInTheDocument();
    expect(screen.getByTitle("The artifacts read carries no pull request")).toBeInTheDocument();
    // The bytes and a pruned mark have no answer; the rules stay the design's own.
    expect(screen.getByTitle(/Nothing records that an artifact’s bytes were pruned/)).toBeInTheDocument();
    expect(view).toHaveTextContent("Rules for every artifact");
    expect(view).toHaveTextContent("Pinned to a commit.");
    expect(screen.getByTitle("No read reports the checkout’s head, so this view cannot mark an artifact stale")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("says a read with no check and no checkout is empty, and keeps the Planned marks", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(emptyTaskArtifacts()))));
    render(<TaskArtifacts id="task-redirect" onBack={() => undefined} />);
    const view = await screen.findByRole("region", { name: "Artifacts" });
    expect(view).toHaveTextContent("No check has run for this build yet.");
    expect(view).toHaveTextContent("This task has no checkout, so there is no change to read.");
    expect(view).not.toHaveTextContent("Check 1 of");
    expect(screen.getAllByText("Planned").length).toBeGreaterThan(3);
  });

  it("says a list was cut at the read's cap", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(taskArtifacts({ truncated: true })))));
    render(<TaskArtifacts id="task-redirect" onBack={() => undefined} />);
    expect(await screen.findByRole("note")).toHaveTextContent("A list was cut at this read’s cap; the newest are shown.");
  });

  it("says a service without the read has none, without an error, and keeps the Planned marks", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer({ code: "not_found", message: "not found" }, 404))));
    render(<TaskArtifacts id="task-redirect" onBack={() => undefined} />);
    expect(await screen.findByText("This service has no artifacts read")).toBeVisible();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText(/stay Planned/)).toBeVisible();
  });

  it("reports any other failure beside the task and Retry reads it again", async () => {
    const user = userEvent.setup();
    const fetch = vi.fn()
      .mockResolvedValueOnce(answer({ code: "unavailable", message: "The artifacts service is busy." }, 503))
      .mockResolvedValueOnce(answer(taskArtifacts()));
    vi.stubGlobal("fetch", fetch);
    render(<TaskArtifacts id="task-redirect" onBack={() => undefined} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Can’t read this task’s artifacts");
    expect(screen.getByRole("alert")).toHaveTextContent("The artifacts service is busy.");
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Check 1 of 2")).toBeInTheDocument();
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("puts a hostile check id or artifact name on the page as text", async () => {
    const hostile = taskArtifacts({
      checks: [{ id: HOSTILE, kind: "test", task: "task-redirect", outcome: "pass", duration_ms: 1, commit: "a41c9e2", artifacts: [{ kind: "log", name: HOSTILE, hash: "sha256:abc" }] }],
    });
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(hostile))));
    const { container } = render(<TaskArtifacts id="task-redirect" onBack={() => undefined} />);
    await screen.findByText("Check 1 of 1");
    expect(container.querySelector("img, script")).toBeNull();
    expect(container.innerHTML).not.toContain("<img src=x");
    expect(container).toHaveTextContent(HOSTILE);
  });

  it("goes back to the task", async () => {
    vi.stubGlobal("fetch", vi.fn(read));
    const onBack = vi.fn();
    const user = userEvent.setup();
    render(<TaskArtifacts id="task-redirect" onBack={onBack} />);
    await user.click(await screen.findByRole("button", { name: "Task" }));
    expect(onBack).toHaveBeenCalledTimes(1);
  });
});
