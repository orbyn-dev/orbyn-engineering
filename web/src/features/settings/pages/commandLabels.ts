/**
 * The names people read for every command and context in Settings > Keybindings. The ids stay in the tooltip and
 * the row menu. A command with no name here fails commandLabels.test.ts, which reads the service's defaults.
 */
export const COMMAND_LABELS: Record<string, string> = {
  "appearance.cycle": "Change appearance",
  "approval.allow": "Allow the waiting command",
  "chat.find": "Find in thread",
  "chat.new": "New thread",
  "chat.newLocal": "New local thread",
  "chat.newWithoutProject": "New thread without a project",
  "commandPalette.toggle": "Command palette",
  "composer.branch": "Choose branch",
  "composer.effort": "Choose effort",
  "composer.mode": "Choose permissions",
  "composer.previousWorktree": "Previous worktree",
  "composer.sendAlternate": "Send the other way",
  "composer.sendAndNewThread": "Send and start a new thread",
  "composer.sendBackground": "Send in the background",
  "composer.stash": "Stash the draft",
  "composer.workspace": "Choose workspace",
  "diff.toggle": "Toggle diff",
  "editor.openFavorite": "Open in editor",
  "filePicker.toggle": "Find a file",
  "modelPicker.toggle": "Choose model",
  "navigation.back": "Back",
  "navigation.forward": "Forward",
  "preview.focusUrl": "Focus preview address",
  "preview.refresh": "Refresh preview",
  "preview.resetZoom": "Reset preview zoom",
  "preview.toggle": "Toggle preview",
  "preview.zoomIn": "Zoom in preview",
  "preview.zoomOut": "Zoom out preview",
  "projectSearch.toggle": "Search projects",
  "pullRequest.copyNumber": "Copy pull request number",
  "rightPanel.close": "Close right panel tab",
  "rightPanel.new": "New right panel tab",
  "rightPanel.toggle": "Toggle right panel",
  "sidebar.toggle": "Toggle sidebar",
  "terminal.close": "Close terminal",
  "terminal.new": "New terminal",
  "terminal.split": "Split terminal",
  "terminal.splitVertical": "Split terminal vertically",
  "terminal.toggle": "Toggle terminal",
  "theme.select": "Choose theme",
  "themeEditor.toggle": "Toggle theme editor",
  "thread.archive": "Archive thread",
  "thread.copyReference": "Copy thread reference",
  "thread.editQueuedMessage": "Edit queued message",
  "thread.new": "New thread",
  "thread.jump.1": "Go to thread 1",
  "thread.jump.2": "Go to thread 2",
  "thread.jump.3": "Go to thread 3",
  "thread.jump.4": "Go to thread 4",
  "thread.jump.5": "Go to thread 5",
  "thread.jump.6": "Go to thread 6",
  "thread.jump.7": "Go to thread 7",
  "thread.jump.8": "Go to thread 8",
  "thread.jump.9": "Go to thread 9",
  "thread.next": "Next thread",
  "thread.nextNeedsYou": "Next thread that needs you",
  "thread.pin": "Pin thread",
  "thread.previous": "Previous thread",
  "thread.settle": "Settle thread",
  "thread.steerQueuedMessage": "Steer with queued message",
  "thread.undo": "Undo last turn",
  "threadPanel.toggle": "Toggle side panel",
  "usage.cost": "Usage by cost",
  "usage.limits": "Usage limits",
  "usage.open": "Open usage",
  "usage.period.day": "Show usage by day",
  "usage.period.month": "Show usage by month",
  "usage.period.quarter": "Show usage by quarter",
  "usage.period.week": "Show usage by week",
  "usage.tokens": "Usage by tokens",
  "view.reopenClosed": "Reopen closed tab",
  "workspace.focusNext": "Focus next workspace",
  "workspace.focusPrevious": "Focus previous workspace",
};

/** What each context says when it holds, and when it does not. */
export const CONTEXT_PHRASES: Record<string, { is: string; not: string }> = {
  composerFocus: { is: "the composer is focused", not: "the composer isn’t focused" },
  editableFocus: { is: "a text field is focused", not: "no text field is focused" },
  previewFocus: { is: "the preview is focused", not: "the preview isn’t focused" },
  rightPanelOpen: { is: "the right panel is open", not: "the right panel is closed" },
  terminalFocus: { is: "the terminal is focused", not: "the terminal isn’t focused" },
  draftRoute: { is: "a new thread is being drafted", not: "a thread is open" },
  turnRunning: { is: "a turn is running", not: "no turn is running" },
  usagePageOpen: { is: "the Usage page is open", not: "the Usage page isn’t open" },
};

export const commandLabel = (command: string): string => COMMAND_LABELS[command] ?? command;

/**
 * A rule's context as a plain phrase: "When the right panel is open and the terminal is focused", and "Anywhere" for
 * none. A context built with anything beyond `&&` and `!` keeps its raw text rather than guess.
 */
export function whenPhrase(when: string | null): string {
  if (!when || when.trim() === "") return "Anywhere";
  const parts = when.split("&&").map((part) => part.trim());
  const phrases = parts.map((part) => {
    const negated = part.startsWith("!");
    const entry = CONTEXT_PHRASES[negated ? part.slice(1).trim() : part];
    return entry ? (negated ? entry.not : entry.is) : null;
  });
  if (phrases.some((phrase) => phrase === null)) return when;
  return `When ${phrases.join(" and ")}`;
}

/** The clause for a sentence: "when the terminal is focused", or "" when the rule applies anywhere. */
export function whenClause(when: string | null): string {
  const phrase = whenPhrase(when);
  if (phrase === "Anywhere") return "";
  return phrase.startsWith("When ") ? `when ${phrase.slice(5)}` : `when ${phrase}`;
}
