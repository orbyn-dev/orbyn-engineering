import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { wireWrites } from "../../../wire.test-support";
import { AcpRegistryView } from "./AcpRegistry";
import { HistoryImportView } from "./ProviderViews";

let mock: MockApi;
beforeEach(async () => {
  mock = installMockFetch();
  // The registry says Installed with the version the engine list reports.
  await useWorkspace.getState().refreshEngines();
});

it("filters the registry as you type and says when nothing matches", async () => {
  const user = userEvent.setup();
  render(<AcpRegistryView onBack={() => undefined} />);
  expect(await screen.findByText("Installed · 0.9.2")).toBeVisible();
  await user.type(screen.getByRole("textbox", { name: "Search the registry" }), "goos");
  expect(screen.getByRole("button", { name: "Install Goose" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Install OpenCode" })).toBeNull();
  await user.type(screen.getByRole("textbox", { name: "Search the registry" }), "x");
  expect(screen.getByText("No agent matches “goosx”")).toBeVisible();
});

it("says in each row what it installs with and what that downloads", async () => {
  render(<AcpRegistryView onBack={() => undefined} />);
  expect((await screen.findAllByText("Checksummed binary")).length).toBe(2);
  expect(screen.getByText("Runs with uvx · downloads when you start it")).toBeVisible();
  expect(screen.getAllByText("Runs with npx · downloads when you start it").length).toBeGreaterThan(0);
  for (const pill of ["Binary", "uvx", "Installed"]) expect(screen.getAllByText(pill, { selector: ".set-pill" }).length).toBeGreaterThan(0);
  expect(screen.getByRole("button", { name: "Install Kimi Code" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Remove Gemini CLI" })).toBeVisible();
  expect(screen.getByText("Can't be installed here")).toBeVisible();
});

it("installs only the way the row says", async () => {
  const user = userEvent.setup();
  // The service refuses an archive whose digest does not match the registry's.
  render(<AcpRegistryView onBack={() => undefined} />);
  const install = await screen.findByRole("button", { name: "Install Kimi Code" });
  mock.fail("/api/acp/registry/kimi", 1, 422, "unsupported", "That agent's archive does not match its digest, so it was not installed.");
  await user.click(install);
  await waitFor(() => expect(wireWrites()).toEqual([{ method: "POST", path: "/api/acp/registry/kimi", body: { distribution: "binary" } }]));
  await waitFor(() => expect(useUi.getState().toasts.map((toast) => toast.text)).toEqual([
    "Couldn't install Kimi Code. That agent's archive does not match its digest, so it was not installed."]));
});

it("asks before removing, and says why a removal is refused while a thread runs", async () => {
  const user = userEvent.setup();
  render(<AcpRegistryView onBack={() => undefined} />);
  await user.click(await screen.findByRole("button", { name: "Remove Gemini CLI" }));
  const dialog = await screen.findByRole("dialog", { name: "Remove Gemini CLI?" });
  expect(within(dialog).getByText("It leaves Orbyn’s list. Its sign-in and sessions stay with the engine.")).toBeVisible();
  expect(wireWrites()).toEqual([]);
  mock.fail("/api/acp/registry/gemini", 1, 409, "engine_busy", "acp-gemini has an active run or update");
  await user.click(within(dialog).getByRole("button", { name: "Remove" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("A thread is running on Gemini CLI. Stop it, then remove it.");
  expect(wireWrites()).toEqual([{ method: "DELETE", path: "/api/acp/registry/gemini" }]);
});

it("imports exactly the sessions that are ticked", async () => {
  const user = userEvent.setup();
  // The mock service finds two importable sessions in one project and one in a folder Orbyn does not track.
  render(<HistoryImportView onBack={() => undefined} />);
  expect(await screen.findByText("Found 3 sessions in 2 projects")).toBeVisible();
  await user.click(screen.getByRole("checkbox", { name: "Import Sync conflict on rename" }));
  await user.click(screen.getByRole("button", { name: "Import 1 session" }));
  await waitFor(() => expect(wireWrites()).toContainEqual({ method: "POST", path: "/api/history/import", body: { engine: "claude", session_id: "h1" } }));
  expect(wireWrites()).not.toContainEqual(expect.objectContaining({ body: { engine: "claude", session_id: "h2" } }));
  expect(wireWrites()).not.toContainEqual(expect.objectContaining({ body: { engine: "claude", session_id: "h3" } }));
});
