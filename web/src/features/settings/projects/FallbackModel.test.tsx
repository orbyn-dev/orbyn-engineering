import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { EngineModels, EngineStatus } from "../../../api/types";
import { useModelStore } from "../../../product/state/modelStore";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { FallbackControl } from "./ProjectModelDefaults";

/**
 * The fallback model control with its real collaborators: the engines the workspace store holds
 * and the model list the model store reads from the service. Only the service is faked.
 */
const claude = { id: "claude", name: "Claude Code", installed: true, capabilities: {} } as unknown as EngineStatus;
const catalog: EngineModels = {
  engine: "claude", source: "engine", checked_at: "fixture", error: null, fallback: false,
  models: [
    { id: "sonnet", label: "Sonnet 5.5", description: null, is_default: true, custom: false, options: [] },
    { id: "opus", label: "Opus 5.5", description: null, is_default: false, custom: false, options: [] },
  ],
};
let mock: MockApi;
beforeEach(async () => {
  mock = installMockFetch();
  mock.setEngines([claude]);
  // The list the service answers and the one the store already holds, which are the same.
  mock.setEngineModels("claude", catalog);
  useModelStore.setState({ byEngine: { claude: catalog }, loading: {}, errors: {} });
  await useWorkspace.getState().refreshEngines();
});

it("picks a known model from the select", async () => {
  const save = vi.fn();
  render(<FallbackControl value="" save={save} disabled={false} />);
  expect(screen.getByRole("option", { name: "Other…" })).toBeInTheDocument();
  expect(screen.getByRole("option", { name: "Opus 5.5" })).toBeInTheDocument();
  await userEvent.setup().selectOptions(screen.getByRole("combobox", { name: "Fallback model" }), "opus");
  expect(save).toHaveBeenCalledWith("opus");
});

it("reveals a model id field for Other… and saves it when you leave it", async () => {
  const save = vi.fn();
  const user = userEvent.setup();
  render(<FallbackControl value="sonnet" save={save} disabled={false} />);
  expect(screen.queryByRole("textbox", { name: "Model id" })).toBeNull();
  await user.selectOptions(screen.getByRole("combobox", { name: "Fallback model" }), "__other");
  const field = screen.getByRole("textbox", { name: "Model id" });
  expect(save).not.toHaveBeenCalled();
  await user.type(field, "claude-fable-5-1");
  await user.tab();
  expect(save).toHaveBeenCalledWith("claude-fable-5-1");
});

it("shows a saved id the provider doesn't list under Other…", () => {
  render(<FallbackControl value="claude-fable-5-1" save={() => undefined} disabled={false} />);
  expect(screen.getByRole("combobox", { name: "Fallback model" })).toHaveValue("__other");
  expect(screen.getByRole("textbox", { name: "Model id" })).toHaveValue("claude-fable-5-1");
});
