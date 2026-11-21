import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { Project } from "../../../../api/types";
import type { CrewClassesView } from "../../../../generated/CrewClassesView";
import { useWorkspace } from "../../../../product/state/workspace";
import { installMockFetch } from "../../../../test-support";
import { PathClasses } from "./PathClasses";

const view = (notes: string[]): CrewClassesView => ({ built_in: "Everything else", rules: [], problems: [], notes, working_copy_differs: true, working_copy_note: "The working copy differs." });
beforeEach(() => {
  installMockFetch().reset("crew-classes");
  useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-app" } as Project], error: null, loading: false } });
});

it("shows generic notes as text, separately from the working-copy notice, and caps the displayed list", async () => {
  vi.mocked(fetch).mockResolvedValue(new Response(JSON.stringify(view(["<img src=x onerror=alert(1)>", ...Array.from({ length: 20 }, (_, i) => `Generic rule note ${i}`)])), { status: 200 }));
  render(<PathClasses />);
  const notes = await screen.findByRole("complementary", { name: "Rule notes" });
  expect(within(notes).getAllByRole("listitem")).toHaveLength(16);
  expect(notes).toHaveTextContent("More rule notes were omitted. Review the remaining entries in .orbyn/crew.json.");
  expect(notes.querySelector("img")).toBeNull();
  expect(notes).toHaveTextContent("<img src=x onerror=alert(1)>");
  expect(notes).not.toHaveTextContent("The working copy differs.");
  expect(screen.getByRole("table", { name: "Path classes" })).toHaveTextContent("Any signed-in engine");
});

it("Check again reads only the selected project and ignores a late reply after selection changes", async () => {
  let finish: ((value: Response) => void) | undefined;
  let reads = 0;
  vi.mocked(fetch).mockImplementation(async (input) => {
    const project = new URL(String(input), "http://local").searchParams.get("project");
    if (project === "p-other") return new Response(JSON.stringify(view(["Other project note"])), { status: 200 });
    if (++reads === 1) return new Response(JSON.stringify(view(["Original note"])), { status: 200 });
    return new Promise<Response>((resolve) => { finish = resolve; });
  });
  const user = userEvent.setup();
  render(<PathClasses />);
  const notes = await screen.findByRole("complementary", { name: "Rule notes" });
  await user.click(within(notes).getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(reads).toBe(2));
  expect(within(notes).getByRole("button", { name: "Check again" })).toBeDisabled();
  act(() => useWorkspace.setState({ projects: { data: [{ id: "p-other", name: "other" } as Project], error: null, loading: false } }));
  expect(await screen.findByText("Other project note")).toBeVisible();
  await act(async () => { finish?.(new Response(JSON.stringify(view(["Stale note"])), { status: 200 })); });
  expect(screen.queryByText("Stale note")).toBeNull();
  expect(screen.getByText("Other project note")).toBeVisible();
  expect(vi.mocked(fetch).mock.calls.every(([input, init]) => String(input).includes("/api/crew/classes?project=") && (init?.method ?? "GET") === "GET")).toBe(true);
});

it("does not duplicate the default-branch warning and refreshes generic notes without a write", async () => {
  let current = view(["Original generic note", "default branch changed: A person must check the ref.", "The working copy differs."]);
  const original = vi.mocked(fetch).getMockImplementation()!;
  vi.mocked(fetch).mockImplementation(async (input, init) => String(input).includes("/api/crew/classes?")
    ? new Response(JSON.stringify(current), { status: 200 }) : original(input, init));
  const user = userEvent.setup();
  render(<PathClasses />);
  const notes = await screen.findByRole("complementary", { name: "Rule notes" });
  expect(within(notes).getAllByRole("listitem")).toHaveLength(1);
  expect(notes).not.toHaveTextContent("A person must check the ref.");
  expect(screen.getByRole("region", { name: "Default branch changed" })).toBeVisible();
  current = view(["Refreshed generic note"]);
  await user.click(within(notes).getByRole("button", { name: "Check again" }));
  expect(await screen.findByText("Refreshed generic note")).toBeVisible();
  expect(screen.queryByRole("region", { name: "Default branch changed" })).toBeNull();
  expect(vi.mocked(fetch).mock.calls.every(([, init]) => (init?.method ?? "GET") === "GET")).toBe(true);
});

it("shows no Rule notes surface for an empty list", async () => {
  vi.mocked(fetch).mockResolvedValue(new Response(JSON.stringify(view([])), { status: 200 }));
  render(<PathClasses />);
  await screen.findByRole("table", { name: "Path classes" });
  expect(screen.queryByRole("complementary", { name: "Rule notes" })).toBeNull();
});
