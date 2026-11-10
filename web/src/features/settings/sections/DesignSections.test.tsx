import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { request } from "../../../api/client";
import type { EngineStatus } from "../../../api/types";
import { useWorkspace } from "../../../product/state/workspace";
import { KeybindingsSection } from "../pages/Keybindings";
import { AboutSection } from "./AboutSection";
import { ProviderTable } from "../providers/ProviderTable";

vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

const store = { raw: [], instances: [{ id: "codex", driver: "codex", enabled: true }], disabled: false, error: null, conflict: false,
  save: vi.fn(), reload: vi.fn() };
const engine = (patch: Partial<EngineStatus>): EngineStatus => ({
  id: "claude", name: "Claude Code", tier: "native", installed: true, path: "/bin/claude", version: "2.1.318", auth: "logged_in",
  latest: null, support: "tested", update_command: null, sign_in_command: null, updating: false, problem: null, ...patch,
} as EngineStatus);

it("shows each provider's state in the table and opens its editor from Edit", async () => {
  const user = userEvent.setup();
  const edit = vi.fn();
  vi.mocked(request).mockResolvedValue({ catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [] } as never);
  render(<ProviderTable store={store} onOpen={() => undefined} onEdit={edit} engines={[
    engine({}),
    engine({ id: "codex", name: "Codex", version: "0.98", auth: "logged_out", sign_in_command: "codex login" }),
    engine({ id: "gemini", name: "Gemini CLI", tier: "acp", version: "0.31", latest: "0.33" }),
  ]} />);
  const rows = screen.getAllByRole("row").slice(1);
  expect(within(rows[0]!).getByText("Ready")).toBeVisible();
  expect(within(rows[1]!).getByText("codex login")).toBeVisible();
  expect(within(rows[2]!).getByText("Update 0.33")).toBeVisible();
  expect(within(rows[2]!).getByText("via ACP")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Edit Codex" }));
  expect(edit).toHaveBeenCalledWith("codex");
});

it("draws shortcuts as key caps and names the command a shortcut clashes with", async () => {
  vi.mocked(request).mockResolvedValue({
    path: "keys.json", issues: [],
    rules: [
      { key: "mod+k", command: "commandPalette.toggle", when: null, remove: false },
      { key: "mod+backspace", command: "thread.archive", when: null, remove: false },
    ],
    conflicts: [{ key: "mod+backspace", when: null, commands: ["thread.archive", "Delete"] }],
  });
  render(<KeybindingsSection />);
  expect(await screen.findByText("Command palette")).toBeVisible();
  expect(screen.getByText(/Archive thread and Delete both use/)).toBeVisible();
  await userEvent.type(screen.getByRole("textbox", { name: "Filter keybindings" }), "archive");
  expect(screen.queryByText("Command palette")).toBeNull();
});

it("opens the licence lists from About and narrows them by package name", async () => {
  vi.mocked(request).mockResolvedValue({ version: "1.0.0", platform: "macos", remote: false });
  const user = userEvent.setup();
  render(<AboutSection />);
  await user.click(await screen.findByRole("button", { name: "View lists" }));
  const table = screen.getByRole("table", { name: "Packages" });
  const before = within(table).getAllByRole("row").length;
  await user.type(screen.getByRole("textbox", { name: "Find a package" }), "zzzz-no-such-package");
  expect(within(table).getAllByRole("row")).toHaveLength(1);
  expect(before).toBeGreaterThan(1);
  await user.click(screen.getByRole("button", { name: "About" }));
  expect(screen.getByRole("button", { name: "View lists" })).toBeVisible();
});

it("lists the installed agent CLIs and their versions on About", async () => {
  vi.mocked(request).mockResolvedValue({ version: "1.0.0", platform: "macos", remote: false });
  useWorkspace.setState({ engines: { data: [engine({}), engine({ id: "codex", name: "Codex", version: "0.98" }), engine({ id: "x", name: "Work", instance_of: "claude" })], error: null, loading: false } });
  render(<AboutSection />);
  expect(await screen.findByText("Claude Code 2.1.318 · Codex 0.98")).toBeVisible();
});

it("leads the About card with the 40 px mark tile, not a lettered placeholder", async () => {
  vi.mocked(request).mockResolvedValue({ version: "1.0.0", platform: "macos", remote: false });
  const { container } = render(<AboutSection />);
  const tile = container.querySelector(".set-card .logo-tile");
  expect(tile).toHaveAttribute("data-size", "40");
  expect(tile?.querySelector("svg.logo")).toHaveAttribute("data-logo", "28");
  expect(container.querySelector(".set-card img")).toBeNull();
});

it("labels every action and context in words, keeps the ids in a tooltip, copies an id from the row menu, and searches both", async () => {
  vi.mocked(request).mockResolvedValue({
    path: "/tmp/keybindings.json", issues: [], conflicts: [],
    rules: [
      { key: "mod+b", command: "sidebar.toggle", when: null, remove: false },
      { key: "mod+j", command: "rightPanel.new", when: "rightPanelOpen && terminalFocus", remove: false },
    ],
  });
  const user = userEvent.setup();
  // user-event installs its own clipboard on setup, so the stand-in goes in after it.
  const write = vi.fn(async () => undefined);
  Object.defineProperty(navigator, "clipboard", { value: { writeText: write }, configurable: true });
  render(<KeybindingsSection />);
  const label = await screen.findByText("Toggle sidebar");
  expect(label).toHaveAttribute("title", "sidebar.toggle");
  expect(screen.queryByText("sidebar.toggle")).toBeNull();
  expect(screen.getByText("New right panel tab")).toBeVisible();
  expect(screen.getByText("Anywhere")).toBeVisible();
  expect(screen.getByText("When the right panel is open and the terminal is focused")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "More for New right panel tab" }));
  await user.click(await screen.findByRole("menuitem", { name: "Copy id" }));
  expect(write).toHaveBeenCalledWith("rightPanel.new");
  await user.type(screen.getByRole("textbox", { name: "Filter keybindings" }), "rightPanel.new");
  expect(screen.queryByText("Toggle sidebar")).toBeNull();
});
