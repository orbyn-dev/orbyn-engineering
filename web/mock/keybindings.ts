/**
 * The mock's Settings › Keybindings: `GET /api/keybindings`, the save at `POST /api/keybindings` and the reset at
 * `POST /api/keybindings/reset`, with the service's rules (crates/orbyn-core/src/settings/keybindings.rs): a rule needs a
 * key and a command, a new rule replaces the one for the same command and context (or the same key and context), and a
 * reset drops the person's own rule for a command and context so the built-in one shows in its built-in place. The
 * conflicts follow from the returned rules; `set` seeds a view a screen cannot reach by clicking (a clash, a rule with a
 * context), and `resetView` seeds the scenarios the catalog draws.
 */
import type { KeybindingConflict } from "../src/generated/KeybindingConflict.ts";
import type { KeybindingReset } from "../src/generated/KeybindingReset.ts";
import type { KeybindingRule } from "../src/generated/KeybindingRule.ts";
import type { KeybindingViewRule } from "../src/generated/KeybindingViewRule.ts";
import type { KeybindingsView } from "../src/generated/KeybindingsView.ts";
import type { MockResponse } from "./api.ts";

export type KeybindingsSeed = "default" | "moved" | "clash";

const whenOf = (when: string | null | undefined) => (when?.trim() ? when : null);
const normalisedKey = (key: string) => key.split("+").map((part) => part.trim().toLowerCase()).join("+");

/** One built-in rule, with the key a reset restores and the marker the page does not draw. */
const builtIn = (key: string, command: string, when: string | null = null): KeybindingViewRule => ({
  key,
  command,
  when,
  remove: false,
  custom: false,
  default_keys: [key],
});

/** The rows the reset design frames draw, in the service's built-in order. */
const frameBuiltIns = (): KeybindingViewRule[] => [
  builtIn("mod+b", "sidebar.toggle"),
  builtIn("mod+j", "terminal.toggle"),
  builtIn("mod+shift+j", "preview.toggle"),
  builtIn("mod+r", "preview.refresh", "previewFocus"),
  builtIn("mod+k", "commandPalette.toggle", "!terminalFocus"),
  builtIn("mod+p", "filePicker.toggle", "!terminalFocus"),
  builtIn("mod+shift+f", "projectSearch.toggle", "!terminalFocus"),
];

/** The service's built-in order; `merged()` lists the person's own rules after these. */
function defaultView(): KeybindingsView {
  const rules = [
    ...frameBuiltIns(),
    builtIn("mod+n", "thread.new"),
    builtIn("mod+shift+b", "threadPanel.toggle", "!terminalFocus"),
    builtIn("mod+enter", "approval.allow"),
    builtIn("alt+down", "thread.nextNeedsYou"),
    builtIn("mod+f", "chat.find", "!terminalFocus"),
    builtIn("mod+backspace", "thread.archive"),
  ];
  return { path: "~/.orbyn/keybindings.json", rules, issues: [], conflicts: conflictsOf(rules) };
}

/** A rule of the person's own with no built-in behind it: there is no default to put back. */
const ownRule = (key: string, command: string, when: string | null = null): KeybindingViewRule => ({
  key,
  command,
  when,
  remove: false,
  custom: true,
  default_keys: [],
});

/** The views the reset frames seed: a Command palette moved to mod+shift+p, and Toggle sidebar moved onto mod+k. */
function seededView(seed: Exclude<KeybindingsSeed, "default">): KeybindingsView {
  const built = frameBuiltIns();
  const commandPalette = built.find((rule) => rule.command === "commandPalette.toggle")!;
  const sidebar = built.find((rule) => rule.command === "sidebar.toggle")!;
  // The person's own rules come after the built-in ones, in the order their file holds them.
  const rules = [
    ...built.filter((rule) => rule.command !== "commandPalette.toggle" && (seed !== "clash" || rule.command !== "sidebar.toggle")),
    ownRule("mod+shift+s", "composer.stash", "!terminalFocus"),
    { ...commandPalette, key: "mod+shift+p", custom: true },
    ...(seed === "clash" ? [{ ...sidebar, key: "mod+k", custom: true }] : []),
  ];
  return { ...defaultView(), rules, conflicts: conflictsOf(rules) };
}

/**
 * What the service reports as clashing: one key in one exact context, plus an unconditional rule that
 * overlaps any condition (since #1251), and a command listed once.
 */
function conflictsOf(rules: KeybindingViewRule[]): KeybindingConflict[] {
  const found: KeybindingConflict[] = [];
  for (const rule of rules) {
    const key = normalisedKey(rule.key);
    const when = whenOf(rule.when);
    const known = found.find((conflict) => conflict.key === key && conflict.when === when);
    if (known) known.commands.push(rule.command);
    else found.push({ key, when, commands: [rule.command] });
  }
  for (const conflict of found) {
    if (!conflict.when) continue;
    const unconditional = rules
      .filter((rule) => normalisedKey(rule.key) === conflict.key && whenOf(rule.when) === null)
      .map((rule) => rule.command);
    conflict.commands = [...unconditional, ...conflict.commands];
  }
  for (const conflict of found) conflict.commands = [...new Set(conflict.commands)];
  return found.filter((conflict) => conflict.commands.length > 1);
}

export function createKeybindingsMock() {
  let view = defaultView();
  return {
    /** Back to the built-in view, or the view a reset frame opens on. */
    resetView(seed: KeybindingsSeed = "default") {
      view = seed === "default" ? defaultView() : seededView(seed);
    },
    set(next: KeybindingsView) {
      view = structuredClone(next);
    },
    get(): KeybindingsView {
      return view;
    },
    save(saved: KeybindingRule): MockResponse {
      if (!saved.key.trim() || !saved.command.trim()) {
        return { status: 400, body: { code: "invalid_input", message: "A rule needs a key and a command." } };
      }
      const when = whenOf(saved.when);
      const known = defaultView().rules.find((rule) => rule.command === saved.command && rule.when === when);
      const defaultKeys = known ? [known.key] : [];
      const next: KeybindingViewRule = {
        key: saved.key,
        command: saved.command,
        when,
        remove: false,
        custom: defaultKeys.length === 0 || normalisedKey(saved.key) !== normalisedKey(defaultKeys[0]!),
        default_keys: defaultKeys,
      };
      const rules = view.rules.filter((rule) => !(rule.command === saved.command && whenOf(rule.when) === when)
        && !(normalisedKey(rule.key) === normalisedKey(saved.key) && whenOf(rule.when) === when));
      const nextRules = saved.remove ? rules : [...rules, next];
      view = { ...view, rules: nextRules, conflicts: conflictsOf(nextRules) };
      return { status: 200, body: view };
    },
    /** A built-in comes back in its built-in place; a rule with no built-in simply goes. */
    reset(body: KeybindingReset): MockResponse {
      if (!body?.command?.trim()) {
        return { status: 400, body: { code: "invalid_input", message: "A reset needs a command." } };
      }
      const when = whenOf(body.when);
      const builtIns = defaultView().rules;
      const restored = builtIns.find((rule) => rule.command === body.command && rule.when === when);
      if (!view.rules.some((rule) => rule.command === body.command && whenOf(rule.when) === when)) {
        return { status: 200, body: view };
      }
      const remaining = view.rules.filter((rule) => !(rule.command === body.command && whenOf(rule.when) === when));
      let rules = remaining;
      if (restored) {
        // The built-in rules keep their table order, and the person's own rules follow them.
        const position = (rule: KeybindingViewRule) => builtIns.findIndex((known) => known.command === rule.command
          && known.when === whenOf(rule.when) && normalisedKey(known.key) === normalisedKey(rule.key));
        const built = remaining.filter((rule) => position(rule) >= 0);
        const own = remaining.filter((rule) => position(rule) < 0);
        built.push({ ...restored, custom: false, default_keys: [restored.key] });
        built.sort((a, b) => position(a) - position(b));
        rules = [...built, ...own];
      }
      view = { ...view, rules, conflicts: conflictsOf(rules) };
      return { status: 200, body: view };
    },
  };
}
