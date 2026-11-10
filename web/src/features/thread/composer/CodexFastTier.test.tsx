import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, it, vi } from "vitest";
import { api } from "../../../api/client";
import type { EngineModels, EngineStatus } from "../../../api/types";
import type { ModelInfo } from "../../../generated/ModelInfo";
import type { ModelOption } from "../../../generated/ModelOption";
import { resolveModel, useEngineModels, useModelStore } from "../../../product/state/modelStore";
import { EffortTrigger, useTraitsMenu } from "./ComposerTraits";
import { ModelPickerTrigger } from "./ModelPicker";

vi.mock("../../../api/client", async (original) => {
  const actual = await original<typeof import("../../../api/client")>();
  return { ...actual, api: { ...actual.api, models: vi.fn() } };
});

const effort: ModelOption = {
  id: "effort", label: "Effort", values: [{ id: "medium", label: "Medium", default: true }],
};
const defaultTier = { id: "default", label: "Default", default: true };
const speed: ModelOption = {
  id: "service_tier", label: "Speed", values: [
    defaultTier,
    { id: "fast", label: "Fast", default: false },
  ],
};
const model = (id: string, label: string, options: ModelOption[]): ModelInfo => ({
  id, label, options, description: null, is_default: id === "plain", custom: false,
});
const catalog: EngineModels = {
  engine: "codex", source: "engine", checked_at: "fixture", error: null,
  models: [
    model("plain", "Plain model", [effort]),
    model("fast", "Fast model", [effort, speed]),
    model("standard", "Standard model", [effort, { ...speed, values: [defaultTier] }]),
  ],
};
const engines = [{ id: "codex", name: "Codex", installed: true, capabilities: { managed: true } }] as EngineStatus[];

function ComposerMenus({ onOption }: { onOption(option: string, value: string): void }) {
  const [selected, setSelected] = useState({ engine: "codex", model: "plain" as string | null });
  const models = useEngineModels(selected.engine);
  const current = resolveModel(models, selected.model);
  const traits = useTraitsMenu({
    model: current, effort: null, extras: { service_tier: "fast" }, access: null,
    onEffort: () => {}, onOption, onAccess: () => {},
  });
  return <>
    <ModelPickerTrigger engines={engines} selected={selected} onPick={setSelected} />
    <EffortTrigger model={current} effort={null} access={null} onOpen={(element) => traits.openAt(element, false)} />
    {traits.menu}
  </>;
}

it("offers Fast only for the selected model that lists it in its service tier", async () => {
  vi.mocked(api.models).mockResolvedValue(catalog);
  useModelStore.setState({ byEngine: { codex: catalog }, loading: {}, errors: {} });
  const onOption = vi.fn();
  const user = userEvent.setup();
  render(<ComposerMenus onOption={onOption} />);

  const openTraits = async () => {
    await user.click(screen.getByRole("button", { name: "Effort: Medium" }));
    return screen.getByRole("menu", { name: "Composer options" });
  };
  const selectModel = async (current: string, next: string) => {
    await user.click(screen.getByRole("button", { name: current }));
    const picker = screen.getByRole("dialog", { name: "Choose a model" });
    await user.click(within(picker).getByRole("option", { name: new RegExp(next) }));
  };

  let menu = await openTraits();
  expect(within(menu).queryByRole("menuitemradio", { name: "Fast" })).not.toBeInTheDocument();
  expect(within(menu).queryByRole("group", { name: "Speed" })).not.toBeInTheDocument();
  await user.keyboard("{Escape}");

  await selectModel("Plain model", "Fast model");
  menu = await openTraits();
  const fast = within(menu).getByRole("menuitemradio", { name: "Fast" });
  expect(fast).toHaveAttribute("aria-checked", "true");
  await user.click(fast);
  expect(onOption).toHaveBeenCalledWith("service_tier", "fast");

  await selectModel("Fast model", "Standard model");
  menu = await openTraits();
  expect(within(menu).getByRole("group", { name: "Speed" })).toBeInTheDocument();
  expect(within(menu).queryByRole("menuitemradio", { name: "Fast" })).not.toBeInTheDocument();
});
