// The words of a Crew device notification (KC10). Pure: the same item always gives the same title, lines and target.
// Only the item's own fields are read. A line is one line, control characters gone, at most 120 characters with a
// trailing ellipsis when it was cut; a group carries counts and kinds only, never a title, a command or a question.
import type { NeedsYouItem } from "../../../generated/NeedsYouItem";
import type { NeedsYouKind } from "../../../generated/NeedsYouKind";
import type { Route } from "../../../product/state/route";

/** The Needs you inbox: where a notice goes when its item carries no thread of its own. */
export const NEEDS_YOU_INBOX: Route = { name: "crew", tab: "inbox" };

/** The Review queue the inbox's review rows open. */
export const REVIEW_QUEUE: Route = { name: "crew", tab: "approvals" };

/** The longest line a system card is given. */
export const NOTICE_LINE_MAX = 120;

export interface Notice {
  title: string;
  lines: string[];
  target: Route;
}

export interface GroupNotice {
  title: string;
  lines: string[];
}

/** The question line when the secret scan refused it, or when the service sent none. */
export const QUESTION_HIDDEN = "The question is not shown here. Open the thread to read it.";

const CONTROL = /\p{Cc}/gu;

/** One clean line: controls and newlines become a space, runs of space fold, and the ends are trimmed. */
function oneLine(text: string): string {
  return text.replace(CONTROL, " ").replace(/\s+/g, " ").trim();
}

/**
 * One line, cut at 120 characters. A cut lands on the last space of the kept text's last 20 characters when there is
 * one, so a word is not split, and ends with "…"; a line that already fits is unchanged.
 */
export function cut120(text: string): string {
  const line = oneLine(text);
  if (line.length <= NOTICE_LINE_MAX) return line;
  const head = line.slice(0, NOTICE_LINE_MAX);
  const space = head.slice(-20).lastIndexOf(" ");
  const kept = space >= 0 ? head.slice(0, head.length - 20 + space) : head;
  return `${kept.trimEnd()}…`;
}

/** The item's reason as one notification line; absent when the service sent none. */
const reasonLines = (item: NeedsYouItem): string[] => {
  const reason = cut120(item.reason ?? "");
  return reason === "" ? [] : [reason];
};

/** A question, or the one sentence that says it is not shown here. An empty question reads the same as an absent one. */
const questionLines = (item: NeedsYouItem): string[] => {
  if (item.question_hidden) return [QUESTION_HIDDEN];
  const question = cut120(item.question ?? "");
  return question === "" ? [QUESTION_HIDDEN] : [question];
};

/** A watch's name when its own field carries it, or when the reason opens with "Watch failing: <name>". */
function carriedWatchName(item: NeedsYouItem): string | null {
  const own = item.name?.trim();
  if (own) return cut120(own);
  const carried = /watch failing:\s*([^·\n]+)/i.exec(item.reason ?? "");
  const name = carried?.[1]?.trim() ?? "";
  return name === "" ? null : cut120(name);
}

/** The title of a stop whose reason says it was a budget. */
const budgetStop = (item: NeedsYouItem): boolean => /budget/i.test(item.reason ?? "");

/** One notice for one item. */
export function noticeOf(item: NeedsYouItem): Notice {
  switch (item.kind) {
    case "task_needs_person":
      return { title: "Asked you a question", lines: questionLines(item), target: NEEDS_YOU_INBOX };
    case "could_not_start":
      return { title: "Could not start", lines: reasonLines(item), target: NEEDS_YOU_INBOX };
    case "merge_waiting":
      return { title: "The merge waits for you", lines: reasonLines(item), target: NEEDS_YOU_INBOX };
    case "merge_gate_waiting":
      return { title: "A merge waits for your release", lines: reasonLines(item), target: NEEDS_YOU_INBOX };
    case "command_approval":
      return { title: "A command waits for your approval", lines: reasonLines(item), target: NEEDS_YOU_INBOX };
    case "background_review":
      return { title: "The review waits for you", lines: reasonLines(item), target: REVIEW_QUEUE };
    case "watch_failing": {
      const name = carriedWatchName(item);
      return { title: name === null ? "Watch failing" : `Watch failing: ${name}`, lines: reasonLines(item), target: NEEDS_YOU_INBOX };
    }
    case "watch_could_not_run":
      return { title: "Watch could not run", lines: reasonLines(item), target: NEEDS_YOU_INBOX };
    case "tripwire_stop":
      return { title: budgetStop(item) ? "Stopped · budget" : "Stopped", lines: reasonLines(item), target: NEEDS_YOU_INBOX };
    case "worker_stopped_twice":
      return { title: "Stopped: its worker stopped twice", lines: reasonLines(item), target: NEEDS_YOU_INBOX };
    case "could_not_stop": {
      const name = cut120(item.name ?? "");
      const reason = cut120(item.reason ?? "");
      return { title: "Could not stop", lines: [name, reason].filter((line) => line !== ""), target: NEEDS_YOU_INBOX };
    }
    case "manual_check":
      return { title: "Needs a manual check", lines: reasonLines(item), target: NEEDS_YOU_INBOX };
    default:
      return { title: "Crew needs you", lines: reasonLines(item), target: NEEDS_YOU_INBOX };
  }
}

type Noun = readonly [one: string, many: string];

const NOUNS: Record<NeedsYouKind, Noun> = {
  task_needs_person: ["question", "questions"],
  could_not_start: ["refused start", "refused starts"],
  manual_check: ["item", "items"],
  merge_waiting: ["merge", "merges"],
  merge_gate_waiting: ["merge", "merges"],
  command_approval: ["command", "commands"],
  background_review: ["review", "reviews"],
  watch_failing: ["watch", "watches"],
  watch_could_not_run: ["watch", "watches"],
  tripwire_stop: ["stop", "stops"],
  worker_stopped_twice: ["stop", "stops"],
  could_not_stop: ["stop", "stops"],
};

/** The one order a group's counts are written in: the kinds as the service lists them, manual checks and unknowns last. */
const KIND_ORDER: readonly NeedsYouKind[] = [
  "task_needs_person",
  "could_not_start",
  "merge_waiting",
  "merge_gate_waiting",
  "command_approval",
  "background_review",
  "watch_failing",
  "watch_could_not_run",
  "tripwire_stop",
  "worker_stopped_twice",
  "could_not_stop",
  "manual_check",
];

const ITEM: Noun = ["item", "items"];
const KNOWN = new Set<string>(KIND_ORDER);

/** The counts line of a group: nouns of the same kind add up, in one fixed kind order, separated by " · ". */
function countsLine(items: NeedsYouItem[]): string {
  const counted = new Map<string, { noun: Noun; count: number }>();
  const add = (noun: Noun) => {
    const key = `${noun[0]}|${noun[1]}`;
    const known = counted.get(key);
    if (known) known.count += 1;
    else counted.set(key, { noun, count: 1 });
  };
  for (const kind of KIND_ORDER) {
    for (const item of items) {
      if (item.kind !== kind) continue;
      add(NOUNS[kind]);
    }
  }
  for (const item of items) if (!KNOWN.has(item.kind)) add(ITEM);
  return [...counted.values()]
    .map(({ noun, count }) => `${count} ${count === 1 ? noun[0] : noun[1]}`)
    .join(" · ");
}

/** One card for several items: the count in the title and the kinds in one line, never any text of the items. */
export function groupNoticeOf(items: NeedsYouItem[]): GroupNotice {
  const counts = cut120(countsLine(items));
  return { title: `Crew: ${items.length} things need you`, lines: counts === "" ? [] : [counts] };
}
