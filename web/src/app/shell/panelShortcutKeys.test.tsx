import { act, render, screen } from "@testing-library/react";
import userEvent, { type UserEvent } from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { KeybindingRule } from "../../api/types";
import { useUi } from "../../product/state/ui";
import { installMockFetch, setViewportWidth } from "../../test-support";
import { App } from "../App";

/**
 * Parity row K42. Each test presses the real chord at the document and lets the shell's dispatcher
 * find the command, so a missing fallback rule, a wrong `when` or a dead handler fails. The chords
 * mirror crates/orbyn-core/src/settings/keybindings.rs.
 */
const DEFAULTS: KeybindingRule[] = [
  { key: "mod+t", command: "rightPanel.new", when: "rightPanelOpen && !terminalFocus", remove: false },
  { key: "mod+alt+shift+b", command: "rightPanel.maximize", when: "rightPanelOpen && !terminalFocus", remove: false },
];

beforeEach(() => {
  installMockFetch();
  const inner = globalThis.fetch;
  vi.stubGlobal("fetch", (input: string, init?: RequestInit) =>
    input === "/api/keybindings"
      ? Promise.resolve(new Response(JSON.stringify({ path: "keybindings.json", rules: DEFAULTS, issues: [] })))
      : inner(input, init),
  );
  // Wide, so the side panel docks and it has a maximise; a narrow window draws an overlay too.
  setViewportWidth(1400);
  useUi.setState({ panelOpen: false, panelMaximized: false, terminalOpen: false });
});

async function openPanel(user: UserEvent) {
  window.location.hash = "#/threads/t-approval";
  render(<App />);
  await screen.findByRole("button", { name: "Show side panel" }, { timeout: 8000 });
  // The dispatcher needs the keymap, which loads asynchronously.
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 20)); });
  await user.click(screen.getByRole("button", { name: "Show side panel" }));
  return screen.getByRole("complementary", { name: "Thread context" });
}

describe("right panel shortcut keys (K42)", () => {
  it("rightPanel.new fires from mod+t and opens the tab launcher", async () => {
    const user = userEvent.setup();
    await openPanel(user);
    await user.keyboard("{Control>}t{/Control}");
    expect(await screen.findByRole("menu", { name: "Open a tab" })).toBeInTheDocument();
  });

  it("rightPanel.maximize fires from mod+alt+shift+b and toggles the panel back", async () => {
    const user = userEvent.setup();
    await openPanel(user);
    expect(useUi.getState().panelMaximized).toBe(false);
    await user.keyboard("{Control>}{Alt>}{Shift>}b{/Shift}{/Alt}{/Control}");
    expect(useUi.getState().panelMaximized).toBe(true);
    await user.keyboard("{Control>}{Alt>}{Shift>}b{/Shift}{/Alt}{/Control}");
    expect(useUi.getState().panelMaximized).toBe(false);
  });

  it("keeps both chords to the open panel", async () => {
    const user = userEvent.setup();
    window.location.hash = "#/threads/t-approval";
    render(<App />);
    await screen.findByRole("button", { name: "Show side panel" }, { timeout: 8000 });
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 20)); });
    await user.keyboard("{Control>}t{/Control}");
    expect(screen.queryByRole("menu", { name: "Open a tab" })).toBeNull();
    await user.keyboard("{Control>}{Alt>}{Shift>}b{/Shift}{/Alt}{/Control}");
    expect(useUi.getState().panelMaximized).toBe(false);
  });
});
