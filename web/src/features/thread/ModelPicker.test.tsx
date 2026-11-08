import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeEach, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import type { EngineModels, EngineStatus } from "../../api/types";
import { ModelPickerTrigger } from "./ModelPicker";
import { useModelStore } from "../../state/modelStore";

vi.mock("../../api/client", async (original) => {
  const actual = await original<typeof import("../../api/client")>();
  return { ...actual, api: { ...actual.api, models: vi.fn() } };
});
const engines = [{ id: "codex", name: "Codex", installed: true, capabilities: { managed: true } }] as EngineStatus[];
const catalog: EngineModels = { engine: "codex", source: "engine", checked_at: "fixture", error: null, fallback: false,
  models: [{ id: "small", label: "Small model", description: null, is_default: true, custom: false, options: [] }],
  options: [], agents: [], output_styles: [] };
beforeEach(() => {
  vi.mocked(api.models).mockResolvedValue(catalog);
  useModelStore.setState({ byEngine: { codex: catalog }, loading: {}, errors: {} });
});

it("keeps the actual model picker inside its owning settings dialog and restores focus after choosing", async () => {
  const picked = vi.fn(); const user = userEvent.setup();
  render(<dialog open aria-label="Settings"><ModelPickerTrigger engines={engines}
    selected={{ engine: "codex", model: "small" }} onPick={picked} /></dialog>);
  const settings = screen.getByRole("dialog", { name: "Settings" });
  const trigger = within(settings).getByRole("button", { name: "Small model" });
  await user.click(trigger);
  const picker = await within(settings).findByRole("dialog", { name: "Choose a model" });
  expect(picker.parentElement).toBe(settings);
  await user.click(within(picker).getByRole("option", { name: /Small model/ }));
  expect(picked).toHaveBeenCalledWith({ engine: "codex", model: "small" });
  expect(screen.queryByRole("dialog", { name: "Choose a model" })).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
});

it("retains the body portal for an ordinary composer picker", async () => {
  render(<ModelPickerTrigger engines={engines} selected={{ engine: "codex", model: "small" }} onPick={() => undefined} />);
  await userEvent.setup().click(screen.getByRole("button", { name: "Small model" }));
  const picker = await screen.findByRole("dialog", { name: "Choose a model" });
  expect(picker.parentElement).toBe(document.body);
});

it("shows the supplied Mixed or Automatic label without changing the underlying selection", async () => {
  const props = { engines, selected: { engine: "codex", model: "small" }, onPick: vi.fn() };
  const { rerender } = render(<ModelPickerTrigger {...props} triggerLabel="Mixed" />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Mixed" })).toBeVisible());
  rerender(<ModelPickerTrigger {...props} triggerLabel="Automatic" />);
  expect(screen.getByRole("button", { name: "Automatic" })).toBeVisible();
  expect(props.onPick).not.toHaveBeenCalled();
});

async function openWithNeighbours() {
  const user = userEvent.setup();
  render(<><button>Before</button><ModelPickerTrigger engines={engines} selected={{ engine: "codex", model: "small" }} onPick={() => undefined} /><button>After</button></>);
  const trigger = screen.getByRole("button", { name: "Small model" });
  await user.click(trigger);
  const picker = await screen.findByRole("dialog", { name: "Choose a model" });
  return { user, trigger, picker, search: within(picker).getByRole("combobox", { name: "Search models" }), chip: within(picker).getByRole("tab", { name: "All" }) };
}

it("Shift+Tab from the chips goes to the search field, and from the search field closes before the trigger", async () => {
  const { user, picker, search, chip } = await openWithNeighbours();
  chip.focus();
  await user.keyboard("{Shift>}{Tab}{/Shift}");
  expect(search).toHaveFocus();
  expect(picker).not.toHaveAttribute("inert");
  await user.keyboard("{Shift>}{Tab}{/Shift}");
  expect(screen.getByRole("button", { name: "Before" })).toHaveFocus();
});

it("Tab goes search field to the chips, and from the chips closes past the trigger", async () => {
  const { user, chip, search } = await openWithNeighbours();
  expect(search).toHaveFocus();
  await user.keyboard("{Tab}");
  expect(chip).toHaveFocus();
  await user.keyboard("{Tab}");
  expect(screen.getByRole("button", { name: "After" })).toHaveFocus();
});

it("Escape closes from the chips and returns focus to the trigger", async () => {
  const { user, trigger, chip } = await openWithNeighbours();
  chip.focus();
  await user.keyboard("{Escape}");
  expect(trigger).toHaveFocus();
});

// Several providers, with notes only where the engine reported one.
const claude: EngineModels = { ...catalog, engine: "claude",
  models: [
    { id: "opus", label: "Opus 5.5", description: "Deepest reasoning.", is_default: true, custom: false, options: [] },
    { id: "sonnet", label: "Sonnet 5.5", description: null, is_default: false, custom: false, options: [] },
  ] };
const codex: EngineModels = { ...catalog, models: [
  { id: "small", label: "Small model", description: "Quick answers.", is_default: true, custom: false, options: [] },
  { id: "big", label: "Big model", description: null, is_default: false, custom: false, options: [] },
] };
const two = [
  { id: "claude", name: "Claude Code", installed: true, capabilities: { managed: true } },
  { id: "codex", name: "Codex", installed: true, capabilities: { managed: true } },
] as EngineStatus[];
async function openTwo() {
  useModelStore.setState({ byEngine: { claude, codex }, loading: {}, errors: {} });
  const user = userEvent.setup();
  render(<ModelPickerTrigger engines={two} selected={{ engine: "codex", model: "small" }} onPick={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Small model" }));
  const picker = await screen.findByRole("dialog", { name: "Choose a model" });
  return { user, picker, search: within(picker).getByRole("combobox", { name: "Search models" }) };
}

it("builds its chips from the engines it is given: All, then each by its full name", async () => {
  const { picker } = await openTwo();
  expect(within(picker).getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["All", "Claude Code", "Codex"]);
  expect(within(picker).getByRole("tab", { name: "All" })).toHaveAttribute("aria-selected", "true");
});

it("lists every provider under its full name, with the note only where the engine reported one", async () => {
  const { picker } = await openTwo();
  const groups = within(picker).getAllByRole("group");
  expect(groups.map((group) => group.getAttribute("aria-label"))).toEqual(["Claude Code", "Codex"]);
  expect(within(picker).getByRole("option", { name: /Opus 5.5/ })).toHaveTextContent("Deepest reasoning.");
  const bare = within(picker).getByRole("option", { name: /Sonnet 5.5/ });
  expect(bare.querySelector("small")).toBeNull();
  expect(within(picker).getByRole("option", { name: /Small model/ })).toHaveAttribute("aria-selected", "true");
});

it("a chip shows only that provider's models, and the arrow keys move between chips", async () => {
  const { user, picker } = await openTwo();
  await user.click(within(picker).getByRole("tab", { name: "Codex" }));
  expect(within(picker).getAllByRole("option").map((option) => option.querySelector("b")?.textContent)).toEqual(["Small model", "Big model"]);
  within(picker).getByRole("tab", { name: "Codex" }).focus();
  await user.keyboard("{ArrowLeft}");
  expect(within(picker).getByRole("tab", { name: "Claude Code" })).toHaveFocus();
  expect(within(picker).getByRole("tab", { name: "Claude Code" })).toHaveAttribute("aria-selected", "true");
  await user.keyboard("{Home}");
  expect(within(picker).getByRole("tab", { name: "All" })).toHaveAttribute("aria-selected", "true");
  expect(within(picker).getAllByRole("option")).toHaveLength(4);
});

it("typing filters across all providers and drops a heading that has no match", async () => {
  const { user, picker, search } = await openTwo();
  await user.click(within(picker).getByRole("tab", { name: "Claude Code" }));
  await user.type(search, "big");
  expect(within(picker).getAllByRole("option").map((option) => option.querySelector("b")?.textContent)).toEqual(["Big model"]);
  expect(within(picker).getAllByRole("group")).toHaveLength(1);
  expect(within(picker).getByRole("tab", { name: "All" })).toHaveAttribute("aria-selected", "true");
  await user.clear(search);
  await user.type(search, "zzz");
  expect(within(picker).getByRole("status")).toHaveTextContent("No models found");
});

it("moves through the rows with the arrow keys, announces the active one, and picks it with Enter", async () => {
  const pick = vi.fn();
  useModelStore.setState({ byEngine: { claude, codex }, loading: {}, errors: {} });
  const user = userEvent.setup();
  render(<ModelPickerTrigger engines={two} selected={{ engine: "codex", model: "small" }} onPick={pick} />);
  await user.click(screen.getByRole("button", { name: "Small model" }));
  const picker = await screen.findByRole("dialog", { name: "Choose a model" });
  const search = within(picker).getByRole("combobox", { name: "Search models" });
  await user.keyboard("{ArrowDown}{ArrowDown}");
  const rows = within(picker).getAllByRole("option");
  expect(search).toHaveAttribute("aria-activedescendant", rows[1]!.id);
  await user.keyboard("{ArrowUp}{ArrowUp}");
  expect(search).toHaveAttribute("aria-activedescendant", rows[3]!.id);
  await user.keyboard("{Enter}");
  expect(pick).toHaveBeenCalledWith({ engine: "codex", model: "big" });
});

it("ArrowLeft in an empty search field moves to the chosen chip", async () => {
  const { user, picker } = await openTwo();
  await user.keyboard("{ArrowLeft}");
  expect(within(picker).getByRole("tab", { name: "All" })).toHaveFocus();
});

it("keeps its list scrollable to the end: the rows scroll, with room under the last one", () => {
  const css = readFileSync(join(process.cwd(), "src/styles/thread.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const rows = /\.model-rows \{([^}]*)\}/.exec(css)![1]!;
  expect(rows).toMatch(/overflow-y:\s*auto/);
  expect(rows).toMatch(/padding:\s*0 5px 8px/);
});

it("takes every colour from the theme tokens, so Light, Dark and Dim all follow", () => {
  const css = readFileSync(join(process.cwd(), "src/styles/thread.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const start = css.indexOf(".model-picker {");
  const block = css.slice(start, css.indexOf(".traits-menu {\n  width: 304px"));
  expect(block.match(/#[0-9a-fA-F]{3,8}\b|rgba?\(/g) ?? []).toEqual([]);
});

it("has no hidden shortcuts: a modifier and a number does not pick, and Shift-click picks like a click", async () => {
  const pick = vi.fn();
  useModelStore.setState({ byEngine: { claude, codex }, loading: {}, errors: {} });
  const user = userEvent.setup();
  render(<ModelPickerTrigger engines={two} selected={{ engine: "codex", model: "small" }} onPick={pick} />);
  await user.click(screen.getByRole("button", { name: "Small model" }));
  const picker = await screen.findByRole("dialog", { name: "Choose a model" });
  await user.keyboard("{Control>}1{/Control}");
  expect(pick).not.toHaveBeenCalled();
  await user.keyboard("{Escape}");
  await user.click(screen.getByRole("button", { name: "Small model" }));
  await user.keyboard("{Shift>}");
  await user.click(within(await screen.findByRole("dialog", { name: "Choose a model" })).getByRole("option", { name: /Opus 5.5/ }));
  await user.keyboard("{/Shift}");
  expect(pick).toHaveBeenCalledTimes(1);
  expect(pick).toHaveBeenCalledWith({ engine: "claude", model: "opus" });
  expect(picker).toBeDefined();
});
