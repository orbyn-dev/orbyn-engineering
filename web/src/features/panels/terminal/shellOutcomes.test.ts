import { describe, expect, it, vi } from "vitest";
import { clearReportedCommands, publishFinished, reportedCommandsOf, subscribeReportedCommands } from "./shellOutcomes";

describe("what a shell reported", () => {
  it("keeps only what it has not published yet, oldest first, and says when something is new", () => {
    const first = { seq: 0, command: "ls", exitCode: 0 };
    const second = { seq: 1, command: "npm test", exitCode: 1 };
    publishFinished("a:main", [first]);
    const listener = vi.fn();
    const stop = subscribeReportedCommands("a:main", listener);
    // The whole list each time: nothing new means nothing changes and nobody is told.
    publishFinished("a:main", [first]);
    expect(reportedCommandsOf("a:main")).toEqual([first]);
    expect(listener).not.toHaveBeenCalled();
    publishFinished("a:main", [first, second]);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(reportedCommandsOf("a:main")).toEqual([first, second]);
    stop();
  });

  it("keeps each terminal to itself, and a closed one leaves nothing behind", () => {
    publishFinished("iso-a:main", [{ seq: 0, command: "ls", exitCode: 0 }]);
    publishFinished("iso-b:main", [{ seq: 0, command: "pwd", exitCode: 0 }]);
    expect(reportedCommandsOf("iso-a:main").map((one) => one.command)).toEqual(["ls"]);
    expect(reportedCommandsOf("iso-c:main")).toEqual([]);
    clearReportedCommands("iso-a:main");
    expect(reportedCommandsOf("iso-a:main")).toEqual([]);
    expect(reportedCommandsOf("iso-b:main")).toHaveLength(1);
  });

  it("holds the newest 200 and drops the oldest", () => {
    const all = Array.from({ length: 260 }, (_, seq) => ({ seq, command: `cmd ${seq}`, exitCode: 0 }));
    publishFinished("long:main", all);
    const kept = reportedCommandsOf("long:main");
    expect(kept).toHaveLength(200);
    expect(kept[0]?.command).toBe("cmd 60");
    expect(kept.at(-1)?.command).toBe("cmd 259");
  });
});
