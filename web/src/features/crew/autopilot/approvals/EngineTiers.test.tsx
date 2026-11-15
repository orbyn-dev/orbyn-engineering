import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { exampleTierView } from "../../../../../mock/reviewControls";
import { useConnection } from "../../../../product/state/connection";
import { goToHash, useRoute } from "../../../../product/state/route";
import { reviewControlApi } from "../../shared/reviewControlApi";
import { useTierEditors } from "./engineTierWrites";
import { EngineTiers } from "./EngineTiers";

beforeEach(() => {
  useConnection.setState({ remote: false }); useTierEditors.setState({ editors: {} });
  window.history.replaceState(null, "", "#/settings/crew");
  vi.spyOn(reviewControlApi, "tiers").mockImplementation(async () => exampleTierView());
});
afterEach(() => vi.restoreAllMocks());
async function edit() { const user = userEvent.setup(); await user.click(await screen.findByRole("button", { name: "Edit tiers" })); return user; }

it("keeps native engines read-only and saves the explicit full map and model pin", async () => {
  const save = vi.spyOn(reviewControlApi, "saveTiers").mockResolvedValue({ revision: 2, assignments: { "example-acp": { tier: "lead", model: "example-model-id" } } });
  render(<EngineTiers />); const user = await edit();
  expect(screen.getAllByText("Uses its model-family tiers")).toHaveLength(2);
  await user.selectOptions(screen.getByLabelText("Tier · Example ACP"), "lead");
  await user.click(screen.getByRole("button", { name: "Save tiers" }));
  expect(await screen.findByText("Engine tiers saved.")).toBeVisible();
  expect(save).toHaveBeenCalledExactlyOnceWith({ seen: 1, assignments: [{ engine: "example-acp", tier: "lead", expected_default_model: "example-model-id" }] });
});
it("requires explicit acceptance of a changed default before a tier edit can save", async () => {
  const view = exampleTierView(); view.engines[2]!.defaults = ["new-default"];
  vi.mocked(reviewControlApi.tiers).mockResolvedValue(view); const save = vi.spyOn(reviewControlApi, "saveTiers").mockRejectedValue(new Error("offline"));
  render(<EngineTiers />); const user = await edit();
  await user.selectOptions(screen.getByLabelText("Tier · Example ACP"), "lead");
  expect(screen.getByRole("button", { name: "Save tiers" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Use current default" }));
  expect(screen.getByRole("button", { name: "Save tiers" })).toBeEnabled(); expect(save).not.toHaveBeenCalled();
});
it("reconciles a lost response, blocks blind retry on a CAS conflict and preserves the old choice for comparison", async () => {
  const view = exampleTierView(); const current = exampleTierView(); current.settings.revision = 3;
  const read = vi.mocked(reviewControlApi.tiers).mockResolvedValueOnce(view).mockResolvedValue(current);
  const send = vi.spyOn(reviewControlApi, "saveTiers").mockRejectedValue(new Error("reply lost"));
  render(<EngineTiers />); const user = await edit(); await user.selectOptions(screen.getByLabelText("Tier · Example ACP"), "scout"); await user.click(screen.getByRole("button", { name: "Save tiers" }));
  await user.click(await screen.findByRole("button", { name: "Reload mappings" }));
  expect(await screen.findByText(/Previous choice: Scout/)).toBeVisible();
  expect(screen.getByLabelText("Tier · Example ACP")).toHaveValue("worker"); expect(send).toHaveBeenCalledTimes(1); expect(read).toHaveBeenCalledTimes(3);
});
it("retains an uncertain write across unmount, offers reads only and never posts it twice", async () => {
  const savedView = exampleTierView(); savedView.settings = { revision: 2, assignments: { "example-acp": { tier: "lead", model: "example-model-id" } } };
  vi.mocked(reviewControlApi.tiers).mockResolvedValueOnce(exampleTierView()).mockResolvedValueOnce(exampleTierView()).mockResolvedValue(savedView);
  let resolve!: (value: ReturnType<typeof exampleTierView>["settings"]) => void;
  const send = vi.spyOn(reviewControlApi, "saveTiers").mockImplementation(() => new Promise((done) => { resolve = done; }));
  const page = render(<EngineTiers />), user = await edit(); await user.selectOptions(screen.getByLabelText("Tier · Example ACP"), "lead"); await user.click(screen.getByRole("button", { name: "Save tiers" }));
  page.unmount(); render(<EngineTiers />); expect(await screen.findByRole("button", { name: "Saving…" })).toBeDisabled();
  await act(async () => { resolve({ revision: 2, assignments: { "example-acp": { tier: "lead", model: "example-model-id" } } }); });
  expect(await screen.findByText("Engine tiers saved.")).toBeVisible(); expect(send).toHaveBeenCalledTimes(1);
  expect(screen.getByText(/Mapped by you/)).toHaveTextContent("Lead");
});
it("guards dirty typed navigation, keeps focus and invalidates discarded intents on unmount", async () => {
  const page = render(<EngineTiers />), user = await edit(); const field = screen.getByLabelText("Tier · Example ACP"); await user.selectOptions(field, "lead");
  act(() => goToHash("/settings/providers")); expect(window.location.hash).toBe("#/settings/crew");
  await user.click(screen.getByRole("button", { name: "Keep editing" })); expect(field).toHaveFocus();
  act(() => goToHash("/settings/about")); await user.click(screen.getByRole("button", { name: "Discard changes" }));
  await waitFor(() => expect(window.location.hash).toBe("#/settings/about"));
  page.unmount(); act(() => goToHash("/settings/providers")); expect(window.location.hash).toBe("#/settings/providers");
});
it("allows no writes for a remote viewer", async () => {
  useConnection.setState({ remote: true }); const save = vi.spyOn(reviewControlApi, "saveTiers"); render(<EngineTiers />);
  expect(await screen.findByRole("button", { name: "Edit tiers" })).toBeDisabled(); expect(save).not.toHaveBeenCalled();
});
it("discards an idle edit and restores focus to Edit tiers on Cancel", async () => {
  const send = vi.spyOn(reviewControlApi, "saveTiers"); render(<EngineTiers />); const user = await edit();
  await user.selectOptions(screen.getByLabelText("Tier · Example ACP"), "lead"); await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(await screen.findByRole("button", { name: "Edit tiers" })).toHaveFocus(); expect(send).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Edit tiers" })); expect(screen.getByLabelText("Tier · Example ACP")).toHaveValue("worker");
});
it("offers only read reconciliation after an unknown result and keeps the draft across remount", async () => {
  vi.mocked(reviewControlApi.tiers).mockResolvedValueOnce(exampleTierView()).mockRejectedValue(new Error("cannot read"));
  const send = vi.spyOn(reviewControlApi, "saveTiers").mockRejectedValue(new Error("lost reply"));
  const page = render(<EngineTiers />), user = await edit(); await user.selectOptions(screen.getByLabelText("Tier · Example ACP"), "scout"); await user.click(screen.getByRole("button", { name: "Save tiers" }));
  expect(await screen.findByRole("button", { name: "Check again" })).toBeEnabled();
  page.unmount(); render(<EngineTiers />); await user.click(await screen.findByRole("button", { name: "Check again" }));
  expect(send).toHaveBeenCalledTimes(1); expect(screen.queryByRole("button", { name: "Save tiers" })).toBeNull(); expect(screen.getByLabelText("Tier · Example ACP")).toHaveValue("scout"); expect(screen.getByLabelText("Tier · Example ACP")).toBeDisabled();
});
it.each([{ defaults: [] }, { defaults: ["one", "two"] }])("blocks saving unresolved explicit defaults $defaults without losing the saved pin", async ({ defaults }) => {
  const view = exampleTierView(); view.engines[2]!.defaults = defaults; vi.mocked(reviewControlApi.tiers).mockResolvedValue(view);
  render(<EngineTiers />); await edit();
  expect(screen.getByRole("button", { name: "Save tiers" })).toBeDisabled(); expect(screen.getByText(/Saved model:/)).toHaveTextContent("example-model-id");
  expect(screen.getByLabelText("Tier · Example ACP")).toHaveAttribute("aria-invalid", "true");
});
it.each(["constructor", "custom-acp"])("keeps the configured engine %s unmapped until Map it and an explicit tier choice", async (id) => {
  const view = exampleTierView(); view.engines.push({ ...view.engines[2]!, id, name: `Configured ${id}`, defaults: [`${id}-default`] });
  vi.mocked(reviewControlApi.tiers).mockResolvedValue(view);
  const send = vi.spyOn(reviewControlApi, "saveTiers").mockRejectedValue(new Error("offline")); render(<EngineTiers />); const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Map it" }));
  const field = screen.getByLabelText(`Tier · Configured ${id}`); expect(field).toHaveValue("unmapped"); expect(field).toHaveFocus(); expect(send).not.toHaveBeenCalled();
  await user.selectOptions(field, "lead"); await user.click(screen.getByRole("button", { name: "Save tiers" }));
  expect(send).toHaveBeenCalledWith({ seen: 1, assignments: [{ engine: "example-acp", tier: "worker", expected_default_model: "example-model-id" }, { engine: id, tier: "lead", expected_default_model: `${id}-default` }] });
});

// A router render can read the assigned hash before the browser emits hashchange.
it("keeps the dirty editor mounted when the router reads a direct hash assignment before its event", async () => {
  function Page({tick}: {tick: number}) { const route = useRoute(); return route.name === "settings" && route.section === "crew" ? <div data-tick={tick}><EngineTiers /></div> : <p>Departed</p>; }
  const page = render(<Page tick={0} />), user = await edit();
  await user.selectOptions(screen.getByLabelText("Tier · Example ACP"), "lead");
  act(() => { window.location.hash = "/settings/providers"; page.rerender(<Page tick={1} />); });
  expect(screen.queryByText("Departed")).not.toBeInTheDocument();
  expect(await screen.findByRole("button", {name: "Keep editing"})).toBeVisible();
  expect(window.location.hash).toBe("#/settings/crew");
  await user.click(screen.getByRole("button", {name: "Discard changes"}));
  expect(await screen.findByText("Departed")).toBeVisible();
});

function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((done) => { resolve = done; }); return { promise, resolve }; }
it("preserves a newer edit and draft when an earlier view refresh completes", async () => {
  const refresh = deferred<ReturnType<typeof exampleTierView>>();
  const read = vi.mocked(reviewControlApi.tiers).mockResolvedValueOnce(exampleTierView()).mockImplementationOnce(() => refresh.promise);
  render(<EngineTiers />); const user = userEvent.setup();
  await user.click(await screen.findByRole("button", {name: "Refresh engines"})); expect(read).toHaveBeenCalledTimes(2);
  await user.click(screen.getByRole("button", {name: "Edit tiers"}));
  await user.selectOptions(screen.getByLabelText("Tier · Example ACP"), "lead");
  const current = Object.values(useTierEditors.getState().editors)[0]!;
  await act(async () => { refresh.resolve(exampleTierView()); });
  expect(screen.getByLabelText("Tier · Example ACP")).toHaveValue("lead");
  expect(screen.getByRole("button", {name: "Save tiers"})).toBeEnabled();
  expect(Object.values(useTierEditors.getState().editors)[0]).toBe(current);
  expect(current.base.revision).toBe(1); expect(current.base.assignments["example-acp"]?.model).toBe("example-model-id");
});
it("retains pending save ownership when an earlier view refresh completes and then accepts its receipt", async () => {
  const refresh = deferred<ReturnType<typeof exampleTierView>>(), write = deferred<ReturnType<typeof exampleTierView>["settings"]>();
  const savedView = exampleTierView(); savedView.settings = {revision: 2, assignments: {"example-acp": {tier: "lead", model: "example-model-id"}}};
  vi.mocked(reviewControlApi.tiers).mockResolvedValueOnce(exampleTierView()).mockImplementationOnce(() => refresh.promise).mockResolvedValue(savedView);
  const send = vi.spyOn(reviewControlApi, "saveTiers").mockImplementation(() => write.promise);
  render(<EngineTiers />); const user = userEvent.setup();
  await user.click(await screen.findByRole("button", {name: "Refresh engines"}));
  await user.click(screen.getByRole("button", {name: "Edit tiers"}));
  await user.selectOptions(screen.getByLabelText("Tier · Example ACP"), "lead");
  await user.click(screen.getByRole("button", {name: "Save tiers"}));
  expect(send).toHaveBeenCalledTimes(1); const pending = Object.values(useTierEditors.getState().editors)[0]!;
  await act(async () => { refresh.resolve(exampleTierView()); });
  expect(screen.getByRole("button", {name: "Saving…"})).toBeDisabled();
  expect(screen.getByLabelText("Tier · Example ACP")).toBeDisabled();
  expect(screen.getByLabelText("Tier · Example ACP")).toHaveValue("lead");
  expect(Object.values(useTierEditors.getState().editors)[0]).toBe(pending);
  await user.click(screen.getByRole("button", {name: "Saving…"}));
  act(() => goToHash("/settings/providers")); expect(window.location.hash).toBe("#/settings/crew");
  expect(send).toHaveBeenCalledTimes(1);
  await act(async () => { write.resolve(savedView.settings); });
  expect(await screen.findByText("Engine tiers saved.")).toBeVisible();
  expect(Object.values(useTierEditors.getState().editors)[0]?.base).toEqual(savedView.settings);
  expect(screen.getByText(/Mapped by you/)).toHaveTextContent("Lead"); expect(send).toHaveBeenCalledTimes(1);
});
