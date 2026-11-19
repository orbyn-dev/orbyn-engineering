import { api } from "../api";
import { pullsApi } from "../api";
import type { Thread } from "../../../api/types";
import { chipId, type ThreadChip } from "../../../product/composer/composerContext";
import { insertToken, pullToken, type Trigger } from "../../../product/composer/skillMention";
import { LOCAL_COMMANDS, MENU_ROWS, PLAN_COMMANDS } from "./composerConstants";
import type { Suggestion } from "./MentionMenu";

/** What a pick puts in the draft, and the chip it adds instead, when any. */
export function pickResult(
  draft: string,
  caret: number,
  trigger: Trigger,
  suggestion: Suggestion,
): { text: string; caret: number; chip?: ThreadChip } {
  // A picked thread is context, not text: it waits above the draft as a chip
  // and its block carries the link the agent reads on demand.
  if (suggestion.thread) {
    return {
      text: draft.slice(0, trigger.start) + draft.slice(caret),
      caret: trigger.start,
      chip: { id: chipId(), kind: "thread", threadId: suggestion.thread.id, title: suggestion.thread.title },
    };
  }
  const next = insertToken(draft, caret, trigger, suggestion.value, suggestion.insert);
  return { text: next.text, caret: next.caret };
}

/** What the @, $, / and # menus offer for the trigger under the caret. */
export async function loadSuggestions(
  trigger: Trigger | null,
  thread: Thread,
  planControls: boolean,
  skillsInSlash: boolean,
  signal: AbortSignal,
): Promise<Suggestion[]> {
  if (!trigger) return [];
  if (trigger.char === "/") {
    const commands = await api.commands(thread.id, signal);
    const needle = trigger.query.toLowerCase();
    const local = LOCAL_COMMANDS.filter(
      (command) => command.value.includes(needle) && (planControls || !PLAN_COMMANDS.has(command.value)),
    );
    const skills = skillsInSlash ? (await api.mentions(thread.id, trigger.query, signal)).skills : [];
    return [
      ...local,
      ...commands
        .filter((command) => command.name.toLowerCase().includes(needle))
        .map((command) => ({ value: command.name, detail: command.description })),
      ...skills.map((value) => ({ value, detail: "Skill" })),
    ];
  }
  if (trigger.char === "#") {
    const digits = /^\d+$/.test(trigger.query);
    const pulls = await pullsApi.projectPulls(thread.project_id, digits ? "" : trigger.query, signal);
    return pulls
      .filter((pull) => !digits || String(pull.number).includes(trigger.query))
      .map((pull) => ({
        value: String(pull.number),
        detail: pull.title,
        insert: pullToken(pull.number, pull.title, pull.url),
      }));
  }
  const found = await api.mentions(thread.id, trigger.query, signal);
  if (trigger.char === "$") return found.skills.map((value) => ({ value, detail: null }));
  const threads = found.threads.map((other) => ({
    value: other.title,
    detail: "Thread",
    thread: { id: other.id, title: other.title },
  }));
  const files = found.files.slice(0, Math.max(MENU_ROWS - threads.length, 2));
  return [...files.map((value) => ({ value, detail: null })), ...threads];
}
