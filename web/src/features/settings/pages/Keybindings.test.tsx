import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { request } from "../../../api/client";
import type { KeybindingRule, KeybindingsView } from "../../../api/types";
import { matchesKey } from "../../../product/shell/shortcuts";
import { KeybindingsSection } from "./Keybindings";

vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

afterEach(() => vi.restoreAllMocks());

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
    vi.mocked(request).mockReset().mockResolvedValue(initial);
    render(<KeybindingsSection />);
    fireEvent.click(await screen.findByRole("button", { name: "Change shortcut for Toggle sidebar" }));
    fireEvent.keyDown(screen.getByRole("button", { name: "Press keys…" }), event);
    await waitFor(() => expect(request).toHaveBeenCalledWith("POST", "/api/keybindings", {
      key, command: "sidebar.toggle", when: "!terminalFocus", remove: false,
    }));
    const saved = vi.mocked(request).mock.calls.find(([method]) => method === "POST")?.[2] as KeybindingRule;
    expect(matchesKey(saved.key, new KeyboardEvent("keydown", event))).toBe(true);
    expect(matchesKey(saved.key, new KeyboardEvent("keydown", { ...event, altKey: !event.altKey }))).toBe(false);
  });
}

it("ignores modifier-only presses and cancels Escape without saving", async () => {
  vi.mocked(request).mockReset().mockResolvedValue(initial);
  render(<KeybindingsSection />);
  fireEvent.click(await screen.findByRole("button", { name: "Change shortcut for Toggle sidebar" }));
  const capture = screen.getByRole("button", { name: "Press keys…" });
  for (const key of ["Meta", "Control", "Shift", "Alt"]) fireEvent.keyDown(capture, { key });
  expect(screen.getByRole("button", { name: "Press keys…" })).toBe(capture);
  fireEvent.keyDown(capture, { key: "Escape", code: "Escape" });
  expect(screen.queryByRole("button", { name: "Press keys…" })).toBeNull();
  expect(vi.mocked(request).mock.calls.filter(([method]) => method === "POST")).toHaveLength(0);
});

it("names a clash in plain words, with the context as a phrase", async () => {
  vi.mocked(request).mockReset().mockResolvedValue({
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
  expect(screen.getByText(/Toggle sidebar and Toggle diff both use/).textContent).toBe("Toggle sidebar and Toggle diff both use ⌘B.");
  expect(screen.getByText(/Send the other way and/).textContent).toBe("Send the other way and Send in the background both use ⌘↵ when the composer is focused and a turn is running.");
});

it("shows arrow keys as arrows", async () => {
  vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  vi.mocked(request).mockReset().mockResolvedValue({
    ...initial, rules: [{ key: "alt+arrowup", command: "thread.editQueuedMessage", when: "composerFocus", remove: false }],
  });
  render(<KeybindingsSection />);
  expect((await screen.findByLabelText("⌥↑")).textContent).toBe("⌥↑");
});

it("keeps the raw text for a context it has no phrase for", async () => {
  vi.mocked(request).mockReset().mockResolvedValue({
    ...initial, conflicts: [{ key: "mod+b", when: "a || b", commands: ["sidebar.toggle", "diff.toggle"] }],
  });
  render(<KeybindingsSection />);
  expect((await screen.findByText(/Toggle sidebar and Toggle diff both use/)).textContent).toContain("when a || b.");
});
