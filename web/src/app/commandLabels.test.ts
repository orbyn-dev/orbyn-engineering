import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";
import { COMMAND_LABELS, commandLabel, CONTEXT_PHRASES, whenPhrase } from "./commandLabels";

const registry = readFileSync(join(process.cwd(), "../crates/orbyn-core/src/settings/keybindings.rs"), "utf8");

it("names every command the service ships a default for", () => {
  const ids = [...new Set([...registry.matchAll(/"([a-z][a-zA-Z]*\.[a-zA-Z]+)"/g)].map((match) => match[1] ?? ""))]
    .filter((id) => !["keybindings.json", "json.partial"].includes(id));
  expect(ids.length).toBeGreaterThan(40);
  expect(ids.filter((id) => !COMMAND_LABELS[id])).toEqual([]);
});

it("uses sentence case and never an id as a label", () => {
  for (const [id, label] of Object.entries(COMMAND_LABELS)) {
    expect(label, id).not.toMatch(/[a-z]\.[a-z]/);
    expect(label.charAt(0), id).toBe(label.charAt(0).toUpperCase());
    expect(label.slice(1), id).not.toMatch(/\b[A-Z][a-z]+ [A-Z][a-z]+\b/);
  }
  expect(commandLabel("sidebar.toggle")).toBe("Toggle sidebar");
  expect(commandLabel("rightPanel.new")).toBe("New right panel tab");
  expect(commandLabel("custom.unknown")).toBe("custom.unknown");
});

it("names every context the service uses", () => {
  const used = [...new Set([...registry.matchAll(/\b(\w+(?:Focus|Open))\b/g)].map((match) => match[1] ?? ""))];
  expect(used.filter((name) => !CONTEXT_PHRASES[name] && /^(composer|editable|preview|rightPanel|terminal|usagePage)/.test(name))).toEqual([]);
});

it("reads a context as a plain phrase", () => {
  expect(whenPhrase(null)).toBe("Anywhere");
  expect(whenPhrase("terminalFocus")).toBe("When the terminal is focused");
  expect(whenPhrase("rightPanelOpen && terminalFocus")).toBe("When the right panel is open and the terminal is focused");
  expect(whenPhrase("!terminalFocus && !previewFocus")).toBe("When the terminal isn’t focused and the preview isn’t focused");
  expect(whenPhrase("a || b")).toBe("a || b");
});
