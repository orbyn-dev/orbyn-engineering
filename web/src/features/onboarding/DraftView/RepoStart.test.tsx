import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createReposMock } from "../../../../mock/repos";
import type { EngineStatus } from "../../../api/types";
import { setReposApi } from "../../repos";
import { RepoStart } from "./RepoStart";

vi.mock("../../../product/state/modelStore", () => ({ useEngineModels: () => null, resolveModel: () => null }));
vi.mock("../../thread/composer/ModelPicker", async (original) => ({ ...(await original<typeof import("../../thread/composer/ModelPicker")>()), ModelPickerTrigger: ({ selected, engines, onPick }: {
  selected: { engine: string; model: string | null }; engines: { id: string }[]; onPick(choice: { engine: string; model: string | null }): void;
}) => <button type="button" onClick={() => onPick({ engine: engines.at(-1)!.id, model: "composer-2" })}>Engine {selected.engine}/{selected.model ?? "automatic"}</button> }));

afterEach(() => { setReposApi(null); window.location.hash = ""; });

const claude = { id: "claude", name: "Claude Code", installed: true, capabilities: { managed: true } } as EngineStatus;
const cursor = { id: "cursor", name: "Cursor", installed: true, capabilities: { managed: true, permission_modes: ["no_approvals", "auto", "full_access"] } } as unknown as EngineStatus;
const draw = (engine = claude, onModel = vi.fn(), onAccess = vi.fn(), access: "supervised" | "full_access" = "supervised") =>
  render(<RepoStart project={{ id: "p1", name: "orbyn" }} engines={[claude, cursor]} engine={engine} model="opus" access={access} onModel={onModel} onAccess={onAccess} />);

it("draws nothing, and no error, when the service answers the repository list with 404", async () => {
  const fetching = vi.fn(async () => new Response(JSON.stringify({ code: "not_found", message: "No such route." }), { status: 404 }));
  vi.stubGlobal("fetch", fetching);
  const { container } = draw();
  await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
  await act(async () => { await Promise.resolve(); });
  expect(container).toBeEmptyDOMElement();
  vi.unstubAllGlobals();
});

it("draws nothing for a project with one repository, which keeps today's New thread", async () => {
  setReposApi(createReposMock({ repositories: [{ ...(await import("../../../../mock/repos")).repoFixtures()[0]! }] }));
  const { container } = draw();
  await act(async () => { await Promise.resolve(); });
  expect(container).toBeEmptyDOMElement();
});

it("opens the repository dialog with the composer's own engine and permission, and starts with them", async () => {
  const mock = createReposMock();
  setReposApi(mock);
  draw();
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Repositories: 4" }));
  const dialog = await screen.findByRole("dialog", { name: /New thread in orbyn/ });
  expect(within(dialog).getByText("Engine")).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Engine claude/opus" })).toBeVisible();
  expect(within(dialog).getByText("Permission")).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Permissions: Supervised" })).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Start in 2 repositories" }));
  await waitFor(() => expect(mock.writes).toHaveLength(1));
  expect(mock.starts).toEqual([{ engine: "claude", model: "opus", permission_mode: "supervised" }]);
});

it("hands a picked engine back to the composer and the next start uses it", async () => {
  const onModel = vi.fn();
  const mock = createReposMock();
  setReposApi(mock);
  draw(claude, onModel);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Repositories: 4" }));
  await user.click(await screen.findByRole("button", { name: "Engine claude/opus" }));
  expect(onModel).toHaveBeenCalledWith({ engine: "cursor", model: "composer-2" });
});

it("dims the permission modes an engine that never asks does not offer, with the reason, and picks only an offered one", async () => {
  const onAccess = vi.fn();
  setReposApi(createReposMock());
  draw(cursor, vi.fn(), onAccess, "full_access");
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Repositories: 4" }));
  const dialog = await screen.findByRole("dialog", { name: /New thread in orbyn/ });
  expect(within(dialog).getByText(/Cursor does not ask before acting, so Supervised and Auto-accept edits are not available here/)).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Permissions: Full access" }));
  const menu = await screen.findByRole("menu", { name: "Composer options" });
  expect(within(menu).getByRole("menuitemradio", { name: "Supervised" })).toHaveAttribute("aria-disabled", "true");
  await user.click(within(menu).getByRole("menuitemradio", { name: "Supervised" }));
  expect(onAccess).not.toHaveBeenCalled();
  await user.click(within(dialog).getByRole("button", { name: "Permissions: Full access" }));
  await user.click(within(await screen.findByRole("menu", { name: "Composer options" })).getByRole("menuitemradio", { name: "Auto" }));
  expect(onAccess).toHaveBeenCalledWith("auto");
});
