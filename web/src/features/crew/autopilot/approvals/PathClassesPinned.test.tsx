import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { readPersonKey } from "../../../../api/personKey";
import type { CrewClassesView } from "../../../../generated/CrewClassesView";
import type { CrewPinnedRefView } from "../../../../generated/CrewPinnedRefView";
import { useConnection } from "../../../../product/state/connection";
import { useWorkspace } from "../../../../product/state/workspace";
import type { Project } from "../../../../api/types";
import { installMockFetch } from "../../../../test-support";
import { PathClasses } from "./PathClasses";

vi.mock("../../../../api/personKey", async (importOriginal) => ({
  ...await importOriginal<typeof import("../../../../api/personKey")>(),
  readPersonKey: vi.fn(),
}));

const oldCommit = "a".repeat(40);
const newCommit = "b".repeat(40);
const reason = "The default branch was rewritten. Ask a person to confirm the new default branch before a task starts.";
const view: CrewClassesView = {
  built_in: "Everything else", rules: [], problems: [],
  notes: [`default branch changed: ${reason}. Every engine is blocked until a person confirms or resets the remembered default branch.`],
  working_copy_differs: false,
};
const pin: CrewPinnedRefView = {
  project: "p-app", stored_ref: "origin/main", stored_commit: oldCommit,
  current_ref: "origin/main", current_commit: newCommit,
  accepted: false, missing: false, blocked: reason,
};

function setup(pinView: CrewPinnedRefView = pin, classesView: CrewClassesView = view) {
  const mock = installMockFetch();
  mock.reset("crew-classes");
  const original = vi.mocked(fetch).getMockImplementation()!;
  vi.mocked(fetch).mockImplementation(async (input, init) => {
    const url = String(input);
    if (url.includes("/api/crew/classes?")) return new Response(JSON.stringify(classesView), { status: 200 });
    if (url.includes("/pinned-ref") && (init?.method ?? "GET") === "GET") {
      return new Response(JSON.stringify(pinView), { status: 200 });
    }
    return original(input, init);
  });
}

beforeEach(() => {
  window.location.hash = "";
  useConnection.setState({ remote: false, crewPreview: true });
  useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-app" } as Project], error: null, loading: false } });
  vi.mocked(readPersonKey).mockResolvedValue("present");
});

describe("pinned default-branch recovery", () => {
  it("shows the authoritative block and confirms the entire displayed commit", async () => {
    setup();
    const user = userEvent.setup();
    render(<PathClasses />);
    const confirm = await screen.findByRole("button", { name: "Confirm this commit" });
    expect(screen.getByText(reason, { exact: false })).toBeVisible();
    expect(screen.getByText("Every engine is blocked until a person confirms or resets the remembered default branch.")).toBeVisible();
    expect(screen.getByRole("region", { name: "Default branch changed" }).textContent).not.toContain("..");
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText(newCommit)).toBeVisible();
    expect(within(screen.getByRole("table", { name: "Path classes" })).getByText("No engine")).toBeVisible();
    const original = vi.mocked(fetch).getMockImplementation()!;
    vi.mocked(fetch).mockImplementation(async (input, init) =>
      String(input).endsWith("/pinned-ref/confirm")
        ? new Response(JSON.stringify({ ...pin, stored_commit: newCommit, accepted: true, blocked: null }), { status: 200 })
        : original(input, init),
    );
    await user.click(confirm);
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([input, init]) =>
      String(input).endsWith("/pinned-ref/confirm") && init?.body === JSON.stringify({ commit: newCommit }),
    )).toBe(true));
  });

  it("reports no allowed engine in a rule drawer while the pin blocks the project", async () => {
    setup(pin, {
      ...view,
      rules: [{ name: "Docs", paths: ["docs/**"], reason: "Docs changes", engines: [
        { id: "codex", name: "Codex", on_this_machine: true },
      ] }],
    });
    const user = userEvent.setup();
    render(<PathClasses />);

    await user.click(await screen.findByRole("button", { name: "Docs" }));
    const detail = screen.getByRole("complementary", { name: "The class Docs" });
    expect(within(detail).getByText("No engine")).toBeVisible();
    expect(within(detail).queryByText("codex")).toBeNull();
  });

  it("hides actions from remote viewers", async () => {
    setup();
    useConnection.setState({ remote: true, crewPreview: true });
    render(<PathClasses />);
    expect(await screen.findByText("Only a person on this computer can confirm or forget the remembered default branch.")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Forget it" })).toBeNull();
  });

  it("withholds writes when the class and pin observations disagree", async () => {
    setup({ ...pin, accepted: true, blocked: null });
    render(<PathClasses />);
    expect(await screen.findByText("Couldn’t check the remembered default branch.")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Confirm this commit" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Forget it" })).toBeNull();
    expect(screen.getByRole("region", { name: "Default branch changed" }).textContent).not.toContain("..");
  });

  it("still allows writes when only the trailing sentence of the note is reworded", async () => {
    setup(pin, { ...view, notes: [`default branch changed: ${reason}. Every engine stays blocked until someone confirms the new default branch.`] });
    render(<PathClasses />);
    expect(await screen.findByRole("button", { name: "Confirm this commit" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Forget it" })).toBeEnabled();
  });

  it("withholds writes when the two reads describe different blocks", async () => {
    setup({ ...pin, blocked: "A newer block" });
    render(<PathClasses />);
    expect(await screen.findByText("Couldn’t check the remembered default branch.")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Confirm this commit" })).toBeNull();
  });

  it("offers reset alone when no current ref exists", async () => {
    setup({ ...pin, current_ref: null, current_commit: null, missing: true });
    render(<PathClasses />);
    expect(await screen.findByText("No default-branch ref")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Confirm this commit" })).toBeNull();
    expect(screen.getByRole("button", { name: "Forget it" })).toBeEnabled();
    expect(screen.getByText(/next start reads the checkout’s current commit/)).toBeVisible();
  });

  it("requires a fresh read after a failed write before allowing another", async () => {
    setup();
    const original = vi.mocked(fetch).getMockImplementation()!;
    let attempts = 0;
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (String(input).endsWith("/pinned-ref/confirm")) {
        attempts += 1;
        return new Response(JSON.stringify({ code: "invalid", message: "the default branch is not the commit that was shown; reload and confirm the commit it has now" }), { status: 400 });
      }
      return original(input, init);
    });
    const user = userEvent.setup();
    render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Confirm this commit" }));
    expect(await screen.findByText(/the default branch is not the commit that was shown/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "Confirm this commit" })).toBeNull();
    expect(attempts).toBe(1);
    await user.click(screen.getByRole("button", { name: "Check again" }));
    expect(await screen.findByRole("button", { name: "Confirm this commit" })).toBeEnabled();
    expect(attempts).toBe(1);
  });

  it("does not offer writes when the local person capability is unavailable", async () => {
    setup();
    vi.mocked(readPersonKey).mockResolvedValue(null);
    render(<PathClasses />);
    expect(await screen.findByText(newCommit)).toBeVisible();
    expect(screen.getByText("Only a person on this computer can confirm or forget the remembered default branch.")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Confirm this commit" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Forget it" })).toBeNull();
  });

  it("keeps recovery busy until the successful reset has been read back", async () => {
    setup();
    const original = vi.mocked(fetch).getMockImplementation()!;
    let reset = false;
    let complete!: (value: Response) => void;
    const reread = new Promise<Response>((resolve) => { complete = resolve; });
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      const url = String(input);
      if (url.endsWith("/pinned-ref/reset")) {
        reset = true;
        return new Response(JSON.stringify({ ...pin, stored_ref: null, stored_commit: null, accepted: true, blocked: null }), { status: 200 });
      }
      if (reset && url.includes("/api/crew/classes?")) return reread;
      return original(input, init);
    });
    const user = userEvent.setup();
    render(<PathClasses />);

    await user.click(await screen.findByRole("button", { name: "Forget it" }));
    expect(await screen.findByText("Remembered default branch forgotten.")).toBeVisible();
    try {
      expect(screen.queryByRole("button", { name: "Check again" })).toBeNull();
      expect(screen.getByRole("button", { name: "Forget it" })).toBeDisabled();
    } finally {
      await act(async () => { complete(new Response(JSON.stringify({ ...view, notes: [] }), { status: 200 })); });
    }
    await waitFor(() => expect(screen.queryByRole("button", { name: "Forget it" })).toBeNull());
  });

  it("retains a pending write across remount and requires a new read after it settles", async () => {
    setup();
    const original = vi.mocked(fetch).getMockImplementation()!;
    let finish: (value: Response) => void = () => {};
    let writes = 0;
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (String(input).endsWith("/pinned-ref/reset")) {
        writes += 1;
        return new Promise<Response>((resolve) => { finish = resolve; });
      }
      return original(input, init);
    });
    const user = userEvent.setup();
    const first = render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Forget it" }));
    expect(screen.getByRole("button", { name: "Forgetting…" })).toBeDisabled();
    first.unmount();
    render(<PathClasses />);
    expect(await screen.findByRole("button", { name: "Forgetting…" })).toBeDisabled();
    finish(new Response(JSON.stringify({ ...pin, stored_ref: null, stored_commit: null, accepted: true, blocked: null }), { status: 200 }));
    expect(await screen.findByText("Remembered default branch forgotten.")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Forget it" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Check again" }));
    expect(await screen.findByRole("button", { name: "Forget it" })).toBeEnabled();
    expect(writes).toBe(1);
  });

  it("keeps an old project's completion out of the newly selected project", async () => {
    setup();
    const original = vi.mocked(fetch).getMockImplementation()!;
    let finish: (value: Response) => void = () => {};
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      const url = String(input);
      if (url.endsWith("/p-app/pinned-ref/reset")) return new Promise<Response>((resolve) => { finish = resolve; });
      if (url.endsWith("/p-other/pinned-ref") && (init?.method ?? "GET") === "GET") {
        return new Response(JSON.stringify({ ...pin, project: "p-other" }), { status: 200 });
      }
      return original(input, init);
    });
    const user = userEvent.setup();
    render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Forget it" }));
    useWorkspace.setState({ projects: { data: [{ id: "p-other", name: "other" } as Project], error: null, loading: false } });
    expect(await screen.findByText("Read for the project other.")).toBeVisible();
    expect(await screen.findByRole("button", { name: "Forget it" })).toBeEnabled();
    finish(new Response(JSON.stringify({ ...pin, stored_ref: null, stored_commit: null, accepted: true, blocked: null }), { status: 200 }));
    await waitFor(() => expect(screen.queryByText("Remembered default branch forgotten.")).toBeNull());
    expect(screen.getByRole("button", { name: "Forget it" })).toBeEnabled();
    useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-app" } as Project], error: null, loading: false } });
    expect(await screen.findByText("Remembered default branch forgotten.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Check again" }));
    expect(await screen.findByRole("button", { name: "Forget it" })).toBeEnabled();
  });

  it("shows the error again when a later foreground read fails", async () => {
    setup(pin, { ...view, problems: ["Rule 3 has no engines listed."], notes: [] });
    const original = vi.mocked(fetch).getMockImplementation()!;
    const user = userEvent.setup();
    render(<PathClasses />);
    expect(await screen.findByRole("table", { name: "What is wrong in the file" })).toBeVisible();
    vi.mocked(fetch).mockImplementation(async (input, init) =>
      String(input).includes("/api/crew/classes?") ? new Response(JSON.stringify({ code: "boom", message: "nope" }), { status: 500 }) : original(input, init),
    );
    await user.click(screen.getByRole("button", { name: "Check again" }));
    expect(await screen.findByText("Can't read the path classes")).toBeVisible();
    expect(screen.queryByRole("table", { name: "Path classes" })).toBeNull();
  });

  it("drops a stale failed-write notice when fresh data arrives after a remount", async () => {
    setup();
    const original = vi.mocked(fetch).getMockImplementation()!;
    let fail: (value: Response) => void = () => {};
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (String(input).endsWith("/pinned-ref/confirm")) return new Promise<Response>((resolve) => { fail = resolve; });
      return original(input, init);
    });
    const user = userEvent.setup();
    const first = render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Confirm this commit" }));
    first.unmount();
    await act(async () => { fail(new Response(JSON.stringify({ code: "invalid", message: "the default branch moved" }), { status: 400 })); });
    render(<PathClasses />);
    expect(await screen.findByRole("button", { name: "Confirm this commit" })).toBeEnabled();
    expect(screen.queryByText(/the default branch moved/)).toBeNull();
    expect(screen.queryByText("Couldn’t check the remembered default branch.")).toBeNull();
  });

  it("moves focus to Check again after a failed write", async () => {
    setup();
    const original = vi.mocked(fetch).getMockImplementation()!;
    vi.mocked(fetch).mockImplementation(async (input, init) =>
      String(input).endsWith("/pinned-ref/confirm") ? new Response(JSON.stringify({ code: "invalid", message: "the default branch moved" }), { status: 400 }) : original(input, init),
    );
    const user = userEvent.setup();
    render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Confirm this commit" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Check again" })).toHaveFocus());
  });
});
