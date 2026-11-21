import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as client from "../../api/client";
import type { Thread } from "../../api/types";
import type { ThreadFamily } from "../../generated/ThreadFamily";
import { useWorkspace } from "../../product/state/workspace";
import { FamilyChip, FamilyLine, FamilyPanel } from "./index";

const make = (id: string, parent: string | null, relation: string | null, title = `Thread ${id}`): Thread =>
  ({
    id,
    project_id: "p1",
    title,
    engine: "claude",
    model: null,
    cwd: "/x",
    worktree: null,
    skill_profile: null,
    active_run: null,
    last_run_state: "completed",
    created_at: "2026-11-20T12:00:00Z",
    updated_at: new Date(Date.now() - 2 * 3_600_000).toISOString(),
    scratch: false,
    lifecycle: { parent_thread_id: parent, relation } as Thread["lifecycle"],
  }) as Thread;

const root = make("a", null, null, "Fix login redirect");
const fork = make("b", "a", "fork", "Try cookie-based redirect");
const side = make("c", "a", "side", "<img src=x onerror=alert(1)>");
const family = (threads: Thread[], thread_id: string): ThreadFamily => ({ thread_id, root_thread_id: "a", threads });

function serve(threads: Thread[], thread_id = "a") {
  return vi.spyOn(client, "request").mockResolvedValue(family(threads, thread_id) as never);
}

beforeEach(() => {
  window.location.hash = "";
  useWorkspace.setState({ threads: { a: root, b: fork, c: side } } as never);
});
afterEach(() => vi.restoreAllMocks());

describe("Linked chip", () => {
  it("counts the other threads and opens the tree, root first", async () => {
    serve([root, fork, side]);
    const user = userEvent.setup();
    render(<FamilyChip thread={root} />);
    await user.click(await screen.findByRole("button", { name: "Linked threads" }));
    expect(screen.getByRole("button", { name: "Linked threads" })).toHaveTextContent("Linked 2");
    const dialog = await screen.findByRole("dialog", { name: "Linked threads" });
    expect(within(dialog).getByText("Linked threads · 3 in this family")).toBeInTheDocument();
    const rows = within(dialog).getAllByRole("link");
    expect(rows).toHaveLength(3);
    expect(rows[0]).toHaveAttribute("aria-current", "true");
    expect(within(rows[0]!).getByText("Root")).toBeInTheDocument();
    expect(within(rows[1]!).getByText("Fork")).toBeInTheDocument();
  });

  it("draws a hostile title as text and opens the thread on a click", async () => {
    serve([root, fork, side]);
    const user = userEvent.setup();
    const { container } = render(<FamilyChip thread={root} />);
    await user.click(await screen.findByRole("button", { name: "Linked threads" }));
    const dialog = await screen.findByRole("dialog", { name: "Linked threads" });
    expect(within(dialog).getByText("<img src=x onerror=alert(1)>")).toBeInTheDocument();
    expect(document.querySelector("img")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
    await user.click(within(dialog).getByRole("link", { name: /Try cookie-based redirect/ }));
    expect(window.location.hash).toBe("#/threads/b");
  });

  it("reads once and makes no write", async () => {
    const spy = serve([root, fork]);
    render(<FamilyChip thread={root} />);
    await screen.findByRole("button", { name: "Linked threads" });
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledWith("GET", "/api/threads/a/family", undefined, expect.anything());
  });

  it("is not there for a thread with no relatives", async () => {
    useWorkspace.setState({ threads: { a: root } } as never);
    const spy = serve([root]);
    render(<FamilyChip thread={root} />);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    await act(async () => undefined);
    expect(screen.queryByRole("button", { name: "Linked threads" })).toBeNull();
  });

  it("shows the chip without a count while the family loads, for a thread known to have a parent", async () => {
    vi.spyOn(client, "request").mockReturnValue(new Promise(() => undefined) as never);
    render(<FamilyChip thread={fork} />);
    expect(await screen.findByRole("button", { name: "Linked threads" })).toHaveTextContent(/^Linked$/);
  });

  it("is not there when the family cannot be read", async () => {
    const spy = vi.spyOn(client, "request").mockRejectedValue(new client.ApiFailure("unexpected", "no", 500));
    render(<FamilyChip thread={fork} />);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    await waitFor(() => expect(screen.queryByRole("button", { name: "Linked threads" })).toBeNull());
  });
});

describe("relation line", () => {
  it("names the relation and links the parent", () => {
    render(<FamilyLine thread={fork} />);
    expect(screen.getByText(/Fork of/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Fix login redirect" })).toHaveAttribute("href", "#/threads/a");
  });
  it("is absent for the root", () => {
    const { container } = render(<FamilyLine thread={root} />);
    expect(container).toBeEmptyDOMElement();
  });
});

describe("panel tab", () => {
  it("lists the family", async () => {
    serve([root, fork]);
    render(<FamilyPanel thread={root} onStart={() => undefined} />);
    expect(await screen.findByRole("link", { name: /Try cookie-based redirect/ })).toBeInTheDocument();
  });

  it("says when there are none and starts one only on a click", async () => {
    serve([root]);
    const onStart = vi.fn();
    const user = userEvent.setup();
    render(<FamilyPanel thread={root} onStart={onStart} />);
    await screen.findByText("No linked threads");
    expect(onStart).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Start a linked thread" }));
    expect(onStart).toHaveBeenCalledTimes(1);
  });

  it("shows quiet bars while loading", () => {
    vi.spyOn(client, "request").mockReturnValue(new Promise(() => undefined) as never);
    render(<FamilyPanel thread={root} onStart={() => undefined} />);
    expect(screen.getByLabelText("Loading linked threads")).toHaveAttribute("aria-busy", "true");
  });

  it("says the thread is fine on a failed read and Retry reads again", async () => {
    const spy = vi.spyOn(client, "request").mockRejectedValueOnce(new client.ApiFailure("unexpected", "no", 500)).mockResolvedValue(family([root, fork], "a") as never);
    const user = userEvent.setup();
    render(<FamilyPanel thread={root} onStart={() => undefined} />);
    expect(await screen.findByText("Can’t load linked threads")).toBeInTheDocument();
    expect(screen.getByText(/The thread is fine/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("link", { name: /Try cookie-based redirect/ })).toBeInTheDocument();
    expect(spy).toHaveBeenCalledTimes(2);
  });
});
