import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { KeybindingRule, KeybindingsView } from "../../../api/types";
import { matchesKey } from "../../../product/shell/shortcuts";
import { useUi } from "../../../product/state/ui";
import { Toasts } from "../../../product/ui/Toasts";
import { installMockFetch } from "../../../test-support";
import { KeybindingsSection } from "./Keybindings";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useUi.setState({ toasts: [] });
});
afterEach(() => vi.restoreAllMocks());

/** The saves sent to the service, as it received them. */
const keybindingSaves = () => vi.mocked(fetch).mock.calls
  .filter(([url, init]) => String(url) === "/api/keybindings" && init?.method === "POST")
  .map(([, init]) => JSON.parse(String(init?.body)) as KeybindingRule);

/** The resets sent to the service, as it received them. */
const keybindingResets = () => vi.mocked(fetch).mock.calls
  .filter(([url, init]) => String(url) === "/api/keybindings/reset" && init?.method === "POST")
  .map(([, init]) => JSON.parse(String(init?.body)) as { command: string; when: string | null });

type ViewRule = KeybindingsView["rules"][number];

/** A view rule with the service's own defaults filled in unless the case wants another. */
const viewRule = (key: string, command: string, patch: Partial<ViewRule> = {}): ViewRule => ({
  key, command, when: null, remove: false, custom: false, default_keys: [key], ...patch,
});

const rowOf = async (label: string) => (await screen.findByText(label)).closest('[role="row"]') as HTMLElement;

const openMenu = async (label: string) => {
  fireEvent.click(await screen.findByRole("button", { name: `More for ${label}` }));
  return screen.findByRole("menu", { name: "Keybinding actions" });
};

const closeMenu = async (menu: HTMLElement) => {
  fireEvent.keyDown(menu, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("menu", { name: "Keybinding actions" })).toBeNull());
};

const resetToDefault = async (label: string) => {
  const menu = await openMenu(label);
  fireEvent.click(within(menu).getByRole("menuitem", { name: /Reset to default/ }));
};

const initial: KeybindingsView = {
  path: "keys.json", issues: [], conflicts: [],
  rules: [{ key: "mod+b", command: "sidebar.toggle", when: "!terminalFocus", remove: false }],
};

const cases: { name: string; event: KeyboardEventInit; key: string }[] = [
  { name: "shifted digit", event: { key: "!", code: "Digit1", shiftKey: true }, key: "shift+1" },
  { name: "shifted left bracket", event: { key: "{", code: "BracketLeft", shiftKey: true }, key: "shift+[" },
  { name: "shifted right bracket", event: { key: "}", code: "BracketRight", shiftKey: true }, key: "shift+]" },
  { name: "ordinary letter", event: { key: "k", code: "KeyK" }, key: "k" },
  { name: "shifted letter", event: { key: "K", code: "KeyK", shiftKey: true }, key: "shift+k" },
  { name: "space", event: { key: " ", code: "Space" }, key: "space" },
];

for (const platform of ["Win32", "MacIntel"]) {
  const primary = platform === "MacIntel" ? { metaKey: true } : { ctrlKey: true };
  const secondary = platform === "MacIntel" ? { ctrlKey: true } : { metaKey: true };
  const secondaryName = platform === "MacIntel" ? "ctrl" : "meta";
  it.each([
    ...cases.map((test) => ({ ...test, event: { ...test.event, ...primary }, key: `mod+${test.key}` })),
    { name: "secondary modifier", event: { key: "k", code: "KeyK", ...secondary }, key: `${secondaryName}+k` },
    { name: "both modifiers and Alt", event: { key: "k", code: "KeyK", ...primary, ...secondary, altKey: true }, key: `mod+${secondaryName}+alt+k` },
  ])(`captures, saves and matches $name on ${platform}`, async ({ event, key }) => {
    vi.spyOn(navigator, "platform", "get").mockReturnValue(platform);
    mock.setKeybindings(initial);
    render(<KeybindingsSection />);
    fireEvent.click(await screen.findByRole("button", { name: "Change shortcut for Toggle sidebar" }));
    fireEvent.keyDown(screen.getByRole("button", { name: "Press keys…" }), event);
    await waitFor(() => expect(keybindingSaves()).toEqual([{ key, command: "sidebar.toggle", when: "!terminalFocus", remove: false }]));
    const saved = keybindingSaves()[0]!;
    expect(matchesKey(saved.key, new KeyboardEvent("keydown", event))).toBe(true);
    expect(matchesKey(saved.key, new KeyboardEvent("keydown", { ...event, altKey: !event.altKey }))).toBe(false);
  });
}

it("ignores modifier-only presses and cancels Escape without saving", async () => {
  mock.setKeybindings(initial);
  render(<KeybindingsSection />);
  fireEvent.click(await screen.findByRole("button", { name: "Change shortcut for Toggle sidebar" }));
  const capture = screen.getByRole("button", { name: "Press keys…" });
  for (const key of ["Meta", "Control", "Shift", "Alt"]) fireEvent.keyDown(capture, { key });
  expect(screen.getByRole("button", { name: "Press keys…" })).toBe(capture);
  fireEvent.keyDown(capture, { key: "Escape", code: "Escape" });
  expect(screen.queryByRole("button", { name: "Press keys…" })).toBeNull();
  expect(keybindingSaves()).toHaveLength(0);
});

it("names a clash in plain words, with the context as a phrase", async () => {
  mock.setKeybindings({
    ...initial,
    conflicts: [
      { key: "mod+shift+b", when: "terminalFocus", commands: ["terminal.split", "terminal.new"] },
      { key: "mod+b", when: null, commands: ["sidebar.toggle", "diff.toggle"] },
      { key: "mod+enter", when: "composerFocus && turnRunning", commands: ["composer.sendAlternate", "composer.sendBackground"] },
    ],
  });
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  render(<KeybindingsSection />);
  expect((await screen.findByText(/Split terminal and New terminal both use/)).textContent).toBe("Split terminal and New terminal both use ⌘⇧B when the terminal is focused.");
  expect(screen.getByText(/Toggle sidebar and Toggle Changes both use/).textContent).toBe("Toggle sidebar and Toggle Changes both use ⌘B.");
  expect(screen.getByText(/Send the other way and/).textContent).toBe("Send the other way and Send in the background both use ⌘↵ when the composer is focused and a turn is running.");
});

it.each([null, "  "])("highlights both unconditional and conditional rules with context %s", async (when) => {
  mock.setKeybindings({
    ...initial,
    rules: [
      { key: "mod+k", command: "sidebar.toggle", when, remove: false },
      { key: "mod+k", command: "commandPalette.toggle", when: "!terminalFocus", remove: false },
      { key: "mod+j", command: "thread.archive", when: null, remove: false },
    ],
    conflicts: [{ key: "mod+k", when: "!terminalFocus", commands: ["sidebar.toggle", "commandPalette.toggle"] }],
  });
  render(<KeybindingsSection />);
  for (const name of ["Toggle sidebar", "Command palette"]) {
    const row = (await screen.findByText(name)).closest('[role="row"]');
    expect(row?.querySelector(".kc.warn")).not.toBeNull();
  }
  const unrelated = (await screen.findByText("Archive thread")).closest('[role="row"]');
  expect(unrelated?.querySelector(".kc.warn")).toBeNull();
});

it("highlights a rule whose key is spelled differently from the clash the service reports", async () => {
  mock.setKeybindings({
    ...initial,
    rules: [
      { key: "Mod+K", command: "sidebar.toggle", when: null, remove: false },
      { key: "mod+k", command: "commandPalette.toggle", when: "!terminalFocus", remove: false },
    ],
    conflicts: [{ key: "mod+k", when: "!terminalFocus", commands: ["sidebar.toggle", "commandPalette.toggle"] }],
  });
  render(<KeybindingsSection />);
  const row = (await screen.findByText("Toggle sidebar")).closest('[role="row"]');
  expect(row?.querySelector(".kc.warn")).not.toBeNull();
});

it("shows arrow keys as arrows", async () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  mock.setKeybindings({
    ...initial, rules: [{ key: "alt+arrowup", command: "thread.editQueuedMessage", when: "composerFocus", remove: false }],
  });
  render(<KeybindingsSection />);
  expect((await screen.findByLabelText("⌥↑")).textContent).toBe("⌥↑");
});

it("keeps the raw text for a context it has no phrase for", async () => {
  mock.setKeybindings({
    ...initial, conflicts: [{ key: "mod+b", when: "a || b", commands: ["sidebar.toggle", "diff.toggle"] }],
  });
  render(<KeybindingsSection />);
  expect((await screen.findByText(/Toggle sidebar and Toggle Changes both use/)).textContent).toContain("when a || b.");
});

it("marks only a rule that differs from its built-in with a Custom chip", async () => {
  mock.setKeybindings({
    path: "keys.json", issues: [], conflicts: [],
    rules: [
      viewRule("mod+b", "sidebar.toggle"),
      viewRule("mod+shift+p", "commandPalette.toggle", { when: "!terminalFocus", custom: true, default_keys: ["mod+k"] }),
    ],
  });
  render(<KeybindingsSection />);
  expect(within(await rowOf("Command palette")).getByText("Custom")).toBeVisible();
  expect(within(await rowOf("Toggle sidebar")).queryByText("Custom")).toBeNull();
});

it("offers Reset to default with the built-in key only on a changed rule", async () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  mock.setKeybindings({
    path: "keys.json", issues: [], conflicts: [],
    rules: [
      viewRule("mod+b", "sidebar.toggle"),
      viewRule("mod+shift+p", "commandPalette.toggle", { when: "!terminalFocus", custom: true, default_keys: ["mod+k"] }),
    ],
  });
  render(<KeybindingsSection />);

  const changed = await openMenu("Command palette");
  const reset = within(changed).getByRole("menuitem", { name: /Reset to default/ });
  expect(reset).toBeEnabled();
  expect(within(reset).getByText("⌘K")).toBeVisible();
  await closeMenu(changed);

  const untouched = await openMenu("Toggle sidebar");
  const off = within(untouched).getByRole("menuitem", { name: /Reset to default/ });
  expect(off).toBeDisabled();
  expect(off).toHaveAttribute("title", "Already the default");
  expect(off.querySelector(".menu-shortcut")).toBeNull();
});

it("leaves Reset to default off for the person's own rule with no built-in default", async () => {
  mock.setKeybindings({
    path: "keys.json", issues: [], conflicts: [],
    rules: [viewRule("mod+shift+s", "composer.stash", { when: "!terminalFocus", custom: true, default_keys: [] })],
  });
  render(<KeybindingsSection />);
  const menu = await openMenu("Stash the draft");
  const reset = within(menu).getByRole("menuitem", { name: /Reset to default/ });
  expect(reset).toBeDisabled();
  expect(reset).toHaveAttribute("title", "Already the default");
  expect(reset.querySelector(".menu-shortcut")).toBeNull();
});

it("resets a changed rule to its built-in key at once and offers Undo in a toast", async () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  mock.setKeybindings({
    path: "keys.json", issues: [], conflicts: [],
    rules: [
      viewRule("mod+b", "sidebar.toggle"),
      viewRule("mod+shift+p", "commandPalette.toggle", { when: "!terminalFocus", custom: true, default_keys: ["mod+k"] }),
    ],
  });
  render(<><KeybindingsSection /><Toasts /></>);

  await resetToDefault("Command palette");

  await waitFor(() => expect(keybindingResets()).toEqual([{ command: "commandPalette.toggle", when: "!terminalFocus" }]));
  expect(keybindingSaves()).toEqual([]);
  await waitFor(() => {
    const row = (screen.getByText("Command palette")).closest('[role="row"]') as HTMLElement;
    expect(within(row).queryByText("Custom")).toBeNull();
    expect(within(row).getByLabelText("⌘K")).toBeVisible();
  });
  expect(await screen.findByText("Reset to default")).toBeVisible();
  expect(await screen.findByRole("button", { name: "Undo" })).toBeVisible();
});

it("undoes a reset with one save of the rule's previous key", async () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  mock.setKeybindings({
    path: "keys.json", issues: [], conflicts: [],
    rules: [
      viewRule("mod+b", "sidebar.toggle"),
      viewRule("mod+shift+p", "commandPalette.toggle", { when: "!terminalFocus", custom: true, default_keys: ["mod+k"] }),
    ],
  });
  render(<><KeybindingsSection /><Toasts /></>);

  await resetToDefault("Command palette");
  fireEvent.click(await screen.findByRole("button", { name: "Undo" }));

  await waitFor(() => expect(keybindingSaves()).toEqual([{
    key: "mod+shift+p", command: "commandPalette.toggle", when: "!terminalFocus", remove: false,
  }]));
  await waitFor(() => {
    const row = (screen.getByText("Command palette")).closest('[role="row"]') as HTMLElement;
    expect(within(row).getByText("Custom")).toBeVisible();
    expect(within(row).getByLabelText("⌘⇧P")).toBeVisible();
    expect(screen.queryByText("Reset to default")).toBeNull();
    expect(screen.queryByRole("button", { name: "Undo" })).toBeNull();
  });
  expect(keybindingResets()).toHaveLength(1);
});

it("shows a reset's clash in the amber banner and caps, and Undo clears it", async () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  mock.setKeybindings({
    path: "keys.json", issues: [], conflicts: [],
    rules: [
      viewRule("mod+k", "sidebar.toggle", { custom: true, default_keys: ["mod+b"] }),
      viewRule("mod+shift+p", "commandPalette.toggle", { when: "!terminalFocus", custom: true, default_keys: ["mod+k"] }),
    ],
  });
  render(<><KeybindingsSection /><Toasts /></>);

  await resetToDefault("Command palette");
  expect(await screen.findByText(/Toggle sidebar and Command palette both use/)).toBeVisible();
  for (const label of ["Toggle sidebar", "Command palette"]) {
    expect((await rowOf(label)).querySelector(".kc.warn")).not.toBeNull();
  }

  fireEvent.click(screen.getByRole("button", { name: "Undo" }));
  await waitFor(() => expect(screen.queryByText(/both use/)).toBeNull());
  for (const label of ["Toggle sidebar", "Command palette"]) {
    expect((await rowOf(label)).querySelector(".kc.warn")).toBeNull();
  }
});

it("shows the field error and leaves the rule when a reset fails", async () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  mock.setKeybindings({
    path: "keys.json", issues: [], conflicts: [],
    rules: [viewRule("mod+shift+p", "commandPalette.toggle", { when: "!terminalFocus", custom: true, default_keys: ["mod+k"] })],
  });
  mock.fail("/api/keybindings/reset", 1, 400, "invalid_input", "A reset needs a command.", "POST");
  render(<><KeybindingsSection /><Toasts /></>);

  await resetToDefault("Command palette");
  expect(await screen.findByRole("alert")).toHaveTextContent("A reset needs a command.");
  const row = await rowOf("Command palette");
  expect(within(row).getByText("Custom")).toBeVisible();
  expect(within(row).getByLabelText("⌘⇧P")).toBeVisible();
  expect(screen.queryByText("Reset to default")).toBeNull();
});

/** The minimum width of each track of a grid template, in order. */
function trackMins(template: string): number[] {
  return (template.match(/minmax\([^)]*\)|\S+/g) ?? []).map((track) =>
    Number(/minmax\(\s*([\d.]+)px/.exec(track)?.[1] ?? /^([\d.]+)px$/.exec(track)?.[1]),
  );
}

// A narrow window (1024 px with the sidebar open, 830 px with it closed, the 375 px phone
// preset) used to squeeze the Action track to nothing, so its label drew over the When
// column. jsdom has no layout, so this pins the rule that prevents it: a minimum width on
// the Action and When tracks, and a scroller for a table narrower than those minimums.
it("gives Action and When their own minimum widths and scrolls the table below them", async () => {
  mock.setKeybindings({
    ...initial,
    rules: [
      { key: "mod+b", command: "sidebar.toggle", when: "!terminalFocus", remove: false },
      { key: "mod+shift+b", command: "terminal.split", when: "terminalFocus", remove: false },
      { key: "mod+enter", command: "composer.sendAlternate", when: "composerFocus && turnRunning", remove: false },
    ],
  });
  render(<KeybindingsSection />);
  const table = await screen.findByRole("table", { name: "Keybindings" });
  const rows = within(table).getAllByRole("row");
  expect(rows).toHaveLength(4);
  const template = (row: HTMLElement) => /grid-template-columns:\s*([^;]+)/.exec(row.getAttribute("style") ?? "")?.[1]?.trim() ?? "";
  // One template for the header and every row, so a row cannot drift from the header.
  expect(new Set(rows.map(template)).size).toBe(1);
  // No track may collapse to zero width; that is what let the Action label draw over When.
  const mins = trackMins(template(rows[0]!));
  expect(mins).toHaveLength(4);
  expect(mins.every((min) => min > 0)).toBe(true);
  expect(mins[0]).toBeGreaterThanOrEqual(120);
  expect(mins[1]).toBeGreaterThanOrEqual(120);
  // Below the room those minimums need, the table scrolls sideways instead of overlapping.
  expect(table.parentElement).toHaveClass("kb-scroll");
  const css = readFileSync(join(process.cwd(), "src/features/settings/pages/keybindings.css"), "utf8");
  expect(css).toMatch(/\.kb-scroll\s*\{[^}]*overflow-x:\s*auto/);
  expect(css).toMatch(/\.kb-table \.set-tr > \*\s*\{[^}]*min-width:\s*0[^}]*overflow-wrap:\s*anywhere/);
});
