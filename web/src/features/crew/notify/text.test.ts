import { describe, expect, it } from "vitest";
import type { NeedsYouItem } from "../../../generated/NeedsYouItem";
import type { NeedsYouKind } from "../../../generated/NeedsYouKind";
import { cut120, groupNoticeOf, NEEDS_YOU_INBOX, noticeOf, QUESTION_HIDDEN, REVIEW_QUEUE } from "./text";

const item = (kind: NeedsYouKind, over: Partial<NeedsYouItem> = {}): NeedsYouItem => ({
  id: "task-1",
  kind,
  project_id: "p-app",
  crew_id: "crew-app",
  since: "2026-11-07T14:02:00Z",
  dismissible: false,
  question_hidden: false,
  ...over,
});

describe("a Crew notification's words", () => {
  it("asks a task's question and points at the inbox when the item names no thread", () => {
    const notice = noticeOf(item("task_needs_person", { question: "Which target wins when both next and returnTo are set?" }));
    expect(notice).toEqual({
      title: "Asked you a question",
      lines: ["Which target wins when both next and returnTo are set?"],
      target: NEEDS_YOU_INBOX,
    });
  });

  it("never shows a question that is hidden or absent", () => {
    expect(noticeOf(item("task_needs_person", { question: "token=secret", question_hidden: true })).lines).toEqual([QUESTION_HIDDEN]);
    expect(noticeOf(item("task_needs_person", { question: null })).lines).toEqual([QUESTION_HIDDEN]);
    expect(noticeOf(item("task_needs_person", { question: "   " })).lines).toEqual([QUESTION_HIDDEN]);
  });

  it("cuts a line at 120 characters on a word and ends it with an ellipsis", () => {
    const long = "word ".repeat(30);
    const line = cut120(long);
    expect(line).toBe(`${Array.from({ length: 24 }, () => "word").join(" ")}…`);
    expect(line.length).toBeLessThanOrEqual(121);
    expect(cut120("short line")).toBe("short line");
  });

  it("collapses control characters and newlines into one line", () => {
    expect(cut120("Line one\nLine two\tthree\u0000four")).toBe("Line one Line two three four");
  });

  it("says what each other kind waits on", () => {
    expect(noticeOf(item("could_not_start", { reason: "Not started: 3.1 GB free, Crew needs 5 GB." }))).toEqual({
      title: "Could not start",
      lines: ["Not started: 3.1 GB free, Crew needs 5 GB."],
      target: NEEDS_YOU_INBOX,
    });
    expect(noticeOf(item("merge_waiting", { reason: "The merge waits for you." }))).toEqual({
      title: "The merge waits for you",
      lines: ["The merge waits for you."],
      target: NEEDS_YOU_INBOX,
    });
    expect(noticeOf(item("merge_gate_waiting", { reason: "A merge waits for your release." }))).toEqual({
      title: "A merge waits for your release",
      lines: ["A merge waits for your release."],
      target: NEEDS_YOU_INBOX,
    });
    expect(noticeOf(item("command_approval", { reason: "A command waits for your approval." }))).toEqual({
      title: "A command waits for your approval",
      lines: ["A command waits for your approval."],
      target: NEEDS_YOU_INBOX,
    });
    expect(noticeOf(item("background_review", { reason: "It used its whole turn budget." }))).toEqual({
      title: "The review waits for you",
      lines: ["It used its whole turn budget."],
      target: REVIEW_QUEUE,
    });
    expect(noticeOf(item("merge_waiting", { reason: null })).lines).toEqual([]);
  });

  it("names a watch only when its own fields carry the name", () => {
    expect(noticeOf(item("watch_failing", { name: "Redirect tests", reason: "The check exited 1." })).title).toBe("Watch failing: Redirect tests");
    expect(noticeOf(item("watch_failing", { reason: "Watch failing: Redirect tests · exited 1" })).title).toBe("Watch failing: Redirect tests");
    expect(noticeOf(item("watch_failing", { reason: "The watch’s command is failing." })).title).toBe("Watch failing");
    expect(noticeOf(item("watch_could_not_run", { reason: "3 checks in a row could not run: timed out." }))).toEqual({
      title: "Watch could not run",
      lines: ["3 checks in a row could not run: timed out."],
      target: NEEDS_YOU_INBOX,
    });
  });

  it("says a stop was a budget only when its reason says budget", () => {
    expect(noticeOf(item("tripwire_stop", { reason: "opencode reached its budget." })).title).toBe("Stopped · budget");
    expect(noticeOf(item("tripwire_stop", { reason: "Injected instructions in a page." })).title).toBe("Stopped");
    expect(noticeOf(item("worker_stopped_twice", { reason: "Its worker stopped again." }))).toEqual({
      title: "Stopped: its worker stopped twice",
      lines: ["Its worker stopped again."],
      target: NEEDS_YOU_INBOX,
    });
  });

  it("draws a task whose worker could not be stopped and a manual check", () => {
    expect(noticeOf(item("could_not_stop", { name: "Add tests for the redirect", reason: "Stop it again." }))).toEqual({
      title: "Could not stop",
      lines: ["Add tests for the redirect", "Stop it again."],
      target: NEEDS_YOU_INBOX,
    });
    expect(noticeOf(item("could_not_stop", { name: "", reason: "Stop it again." })).lines).toEqual(["Stop it again."]);
    expect(noticeOf(item("manual_check", { reason: "A person must read this check." }))).toEqual({
      title: "Needs a manual check",
      lines: ["A person must read this check."],
      target: NEEDS_YOU_INBOX,
    });
    expect(noticeOf(item("something_else" as NeedsYouKind, { reason: "Look at this." }))).toEqual({
      title: "Crew needs you",
      lines: ["Look at this."],
      target: NEEDS_YOU_INBOX,
    });
  });
});

describe("a grouped Crew notification", () => {
  it("counts five mixed items in one fixed order and carries no text of theirs", () => {
    const notice = groupNoticeOf([
      item("task_needs_person", { id: "q1", question: "Which target wins?" }),
      item("task_needs_person", { id: "q2", question: "Which target should win?" }),
      item("could_not_start", { id: "r1", reason: "Not started: 3.1 GB free." }),
      item("background_review", { id: "b1", reason: "It used its whole turn budget." }),
      item("watch_failing", { id: "w1", name: "Redirect tests", reason: "The check exited 1." }),
    ]);
    expect(notice).toEqual({
      title: "Crew: 5 things need you",
      lines: ["2 questions · 1 refused start · 1 review · 1 watch"],
    });
    expect(notice.lines[0]).not.toContain("Which target");
  });

  it("adds the kinds that share a noun and puts unknown kinds under items", () => {
    const notice = groupNoticeOf([
      item("tripwire_stop", { id: "s1", reason: "A trip wire stopped the task." }),
      item("worker_stopped_twice", { id: "s2", reason: "Its worker stopped twice." }),
      item("could_not_stop", { id: "s3", reason: "Stop it again." }),
      item("something_else" as NeedsYouKind, { id: "u1", reason: "Look at this." }),
    ]);
    expect(notice.lines).toEqual(["3 stops · 1 item"]);
  });
});
