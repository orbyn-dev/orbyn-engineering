import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { ApiFailure, request } from "../../../api/client";
import { AcpRegistryView } from "./AcpRegistry";
import { HistoryImportView } from "./ProviderViews";

vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));
vi.mock("../../../product/state/workspace", () => ({ useWorkspace: Object.assign((select: (state: unknown) => unknown) => select({ refreshEngines: vi.fn(), engines: { data: [{ id: "gemini", version: "2.2.5" }] } }), { getState: () => ({ refreshAll: vi.fn() }) }) }));

const entries = [
  { id: "gemini", name: "Gemini CLI", version: "1", description: "Google", installable: ["npx"], engine: "gemini" },
  { id: "goose", name: "Goose", version: "1", description: "Block", installable: ["uvx"], engine: null },
  { id: "kimi", name: "Kimi Code", version: "1", description: "Moonshot", installable: ["binary"], engine: null },
  { id: "old", name: "Old Agent", version: "1", description: "Nothing fits", installable: [], engine: null },
];

it("filters the registry as you type and says when nothing matches", async () => {
  const user = userEvent.setup();
  vi.mocked(request).mockResolvedValue(entries);
  render(<AcpRegistryView onBack={() => undefined} />);
  expect(await screen.findByText("Installed · 2.2.5")).toBeVisible();
  await user.type(screen.getByRole("textbox", { name: "Search the registry" }), "goos");
  expect(screen.getByRole("button", { name: "Install Goose" })).toBeVisible();
  await user.type(screen.getByRole("textbox", { name: "Search the registry" }), "x");
  expect(screen.getByText("No agent matches “goosx”")).toBeVisible();
});

it("says in each row what it installs with and what that downloads", async () => {
  vi.mocked(request).mockResolvedValue(entries);
  render(<AcpRegistryView onBack={() => undefined} />);
  expect(await screen.findByText("Checksummed binary")).toBeVisible();
  expect(screen.getByText("Runs with uvx · downloads when you start it")).toBeVisible();
  for (const pill of ["Binary", "uvx", "Installed"]) expect(screen.getByText(pill)).toBeVisible();
  expect(screen.getByRole("button", { name: "Install Kimi Code" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Remove Gemini CLI" })).toBeVisible();
  expect(screen.getByText("Can't be installed here")).toBeVisible();
});

it("installs only the way the row says", async () => {
  const user = userEvent.setup();
  vi.mocked(request).mockImplementation(async (method) => {
    if (method === "POST") throw new ApiFailure("unsupported", "That agent's archive does not match its digest, so it was not installed.", 422);
    return entries as never;
  });
  render(<AcpRegistryView onBack={() => undefined} />);
  await user.click(await screen.findByRole("button", { name: "Install Kimi Code" }));
  await waitFor(() => expect(request).toHaveBeenCalledWith("POST", "/api/acp/registry/kimi", { distribution: "binary" }));
});

it("asks before removing, and says why a removal is refused while a thread runs", async () => {
  const user = userEvent.setup();
  vi.mocked(request).mockImplementation(async (method) => {
    if (method === "DELETE") throw new ApiFailure("engine_busy", "acp-gemini has an active run or update", 409);
    return entries as never;
  });
  render(<AcpRegistryView onBack={() => undefined} />);
  await user.click(await screen.findByRole("button", { name: "Remove Gemini CLI" }));
  const dialog = await screen.findByRole("dialog", { name: "Remove Gemini CLI?" });
  expect(within(dialog).getByText("It leaves Orbyn’s list. Its sign-in and sessions stay with the engine.")).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Remove" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("A thread is running on Gemini CLI. Stop it, then remove it.");
  expect(request).toHaveBeenCalledWith("DELETE", "/api/acp/registry/gemini");
});

it("imports exactly the sessions that are ticked", async () => {
  const user = userEvent.setup();
  const session = (id: string, importable = true) => ({ engine: "claude", session_id: id, title: `Session ${id}`, cwd: "/code/app", updated_at: "2026-11-06T10:00:00Z", importable, thread_id: null, project_id: "p" });
  vi.mocked(request).mockImplementation(async (_method, path) => {
    if (path === "/api/history/sessions") return { sessions: [session("a"), session("b"), session("c", false)], truncated: false } as never;
    return { thread_id: "t" } as never;
  });
  render(<HistoryImportView onBack={() => undefined} />);
  expect(await screen.findByText("Found 3 sessions in 1 projects")).toBeVisible();
  await user.click(screen.getByRole("checkbox", { name: "Import Session b" }));
  const go = screen.getByRole("button", { name: "Import 1 session" });
  await user.click(go);
  await waitFor(() => expect(request).toHaveBeenCalledWith("POST", "/api/history/import", { engine: "claude", session_id: "a" }));
  expect(request).not.toHaveBeenCalledWith("POST", "/api/history/import", { engine: "claude", session_id: "b" });
});
