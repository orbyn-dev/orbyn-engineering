/**
 * The mock's Settings › Keybindings: `GET /api/keybindings` and the save at `POST /api/keybindings`, with the service's
 * rules for a save (crates/orbyn-core/src/settings/keybindings.rs): a rule needs a key and a command, and a new rule
 * replaces the one for the same command and context, or for the same key and context. The conflicts stay as the view
 * names them; `set` seeds a view a screen cannot reach by clicking (a clash, a rule with a context).
 */
import type { KeybindingRule } from "../src/generated/KeybindingRule.ts";
import type { KeybindingsView } from "../src/generated/KeybindingsView.ts";
import type { MockResponse } from "./api.ts";

const rule = (key: string, command: string): KeybindingRule => ({ key, command, when: null, remove: false });

/** Fixed shortcuts for the Keybindings page: one clash, no issues. */
function defaultView(): KeybindingsView {
  return {
    path: "~/.orbyn/keybindings.json",
    rules: [
      rule("mod+n", "thread.new"), rule("mod+k", "commandPalette.toggle"), rule("mod+shift+b", "threadPanel.toggle"),
      rule("ctrl+`", "terminal.toggle"), rule("mod+enter", "approval.allow"), rule("alt+down", "thread.nextNeedsYou"),
      rule("mod+f", "chat.find"), rule("mod+backspace", "thread.archive"),
    ],
    issues: [],
    conflicts: [{ key: "mod+backspace", when: null, commands: ["thread.archive", "Delete"] }],
  };
}

export function createKeybindingsMock() {
  let view = defaultView();
  return {
    reset() {
      view = defaultView();
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
      const rules = view.rules.filter((known) => !(known.command === saved.command && known.when === saved.when)
        && !(known.key === saved.key && known.when === saved.when));
      view = { ...view, rules: saved.remove ? rules : [...rules, saved] };
      return { status: 200, body: view };
    },
  };
}
