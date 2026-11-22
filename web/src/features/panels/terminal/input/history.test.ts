import { beforeEach, describe, expect, it } from "vitest";
import { historyOf, maskSecrets, recordCommand, recordExitCode, searchHistory, suggestion, type HistoryEntry } from "./history";

const at = (command: string, n = 0): HistoryEntry => ({ command, ok: true, at: n });

describe("history", () => {
  beforeEach(() => {
    for (let i = 0; i < 1; i += 1) recordCommand("reset", "x");
  });
  it("searches newest first, once per command, case-insensitively", () => {
    const list = [at("git status"), at("npm test"), at("git status"), at("git log")];
    const found = searchHistory(list, "GIT");
    expect(found.map((row) => row.entry.command)).toEqual(["git log", "git status"]);
    expect(found[0]?.match).toEqual([0, 3]);
  });
  it("lists everything for an empty query, with no match range", () => {
    expect(searchHistory([at("a"), at("b")], "").map((row) => row.shown)).toEqual(["b", "a"]);
  });
  it("masks a secret in what is listed and in what a search can reach", () => {
    const list = [at("export OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwx"), at("echo hi")];
    expect(searchHistory(list, "")[1]?.shown).toBe("export OPENAI_API_KEY=••••••");
    expect(searchHistory(list, "sk-abc")).toEqual([]);
    expect(maskSecrets("curl -H 'x' AKIAIOSFODNN7EXAMPLE")).toBe("curl -H 'x' ••••••");
  });
  it("never records a secret or a blank line", () => {
    recordCommand("k", "  ");
    recordCommand("k", "export TOKEN=abcdef");
    recordCommand("k", "ls");
    expect(historyOf("k").map((entry) => entry.command)).toEqual(["ls"]);
  });
  it("suggests the rest of the newest earlier command", () => {
    const list = [at("git status"), at("git stash")];
    expect(suggestion(list, "git sta")).toBe("sh");
    expect(suggestion(list, "git status")).toBe("");
    expect(suggestion(list, "")).toBe("");
    expect(suggestion(list, "a\nb")).toBe("");
  });
});

describe("how a command ended", () => {
  it("gives a success and a failure to the oldest unknown line with that command, in the order they arrived", () => {
    recordCommand("ended:main", "npm test", 1);
    recordCommand("ended:main", "npm test", 2);
    expect(recordExitCode("ended:main", "npm test", 0)).toBe(true);
    expect(recordExitCode("ended:main", "npm test", 1)).toBe(true);
    expect(historyOf("ended:main").map((entry) => entry.ok)).toEqual([true, false]);
    // Both lines have their result: a third report of the same command has no line to give it to.
    expect(recordExitCode("ended:main", "npm test", 0)).toBe(false);
  });

  it("finds the line through the shell's own spacing and through a line break", () => {
    recordCommand("space:main", "cargo   test    --workspace", 1);
    recordCommand("space:main", "npm run\n  build", 2);
    expect(recordExitCode("space:main", "cargo test --workspace", 1)).toBe(true);
    expect(recordExitCode("space:main", "npm run build", 0)).toBe(true);
    expect(historyOf("space:main").map((entry) => entry.ok)).toEqual([false, true]);
  });

  it("treats any non-zero code as a failure, an interrupted one included", () => {
    recordCommand("code:main", "sleep 9", 1);
    recordExitCode("code:main", "sleep 9", 130);
    expect(historyOf("code:main")[0]?.ok).toBe(false);
  });

  it("leaves every line alone when no unknown line carries that command", () => {
    recordCommand("none:main", "ls", 1);
    expect(recordExitCode("none:main", "cargo test", 0)).toBe(false);
    expect(recordExitCode("other:main", "ls", 0)).toBe(false);
    expect(recordExitCode("none:main", "   ", 0)).toBe(false);
    expect(historyOf("none:main")[0]?.ok).toBeNull();
  });

  it("never gives the same line a second result", () => {
    recordCommand("once:main", "ls", 1);
    expect(recordExitCode("once:main", "ls", 1)).toBe(true);
    expect(recordExitCode("once:main", "ls", 0)).toBe(false);
    expect(historyOf("once:main").map((entry) => entry.ok)).toEqual([false]);
  });
});

describe("history across a project", () => {
  it("joins the terminals of a project, oldest first like one history, and leaves other projects out", async () => {
    const { historyAcross } = await import("./history");
    recordCommand("a:main", "git status", 1);
    recordCommand("b:main", "npm test", 2);
    recordCommand("a:main", "git status", 3);
    recordCommand("z:main", "other project", 4);
    expect(historyAcross(["a", "b"]).map((entry) => entry.command)).toEqual(["git status", "npm test", "git status"]);
  });
});

describe("history row time", () => {
  it("says how long ago it ran, and how it ended only when the shell reported one", async () => {
    const { rowDetail } = await import("./history");
    expect(rowDetail({ command: "x", ok: null, at: 0 }, 4 * 60_000)).toBe("4 min ago");
    expect(rowDetail({ command: "x", ok: false, at: 0 }, 4 * 60_000)).toBe("4 min ago · 1 failed");
    // The check mark carries a success, so the row says only when.
    expect(rowDetail({ command: "x", ok: true, at: 0 }, 4 * 60_000)).toBe("4 min ago");
  });
  it("shows a multi-line command as one row with a visible separator, and returns the real text", () => {
    const command = "for f in a b; do\n  echo $f\ndone";
    const [row] = searchHistory([at(command)], "");
    expect(row?.shown).toBe("for f in a b; do ⏎ echo $f ⏎ done");
    expect(row?.shown).not.toContain("\n");
    expect(row?.entry.command).toBe(command);
  });
  it("matches across the separator and masks a secret-looking word on any line", () => {
    const [row] = searchHistory([at("echo one\nexport API_KEY=hunter2hunter2\necho two")], "one");
    expect(row?.shown).toBe("echo one ⏎ export API_KEY=•••••• ⏎ echo two");
    expect(row?.match).toEqual([5, 8]);
    expect(searchHistory([at("echo one\nexport API_KEY=hunter2hunter2")], "hunter")).toEqual([]);
  });
  it("collapses blank lines and runs of whitespace around a break once", () => {
    expect(searchHistory([at("a\n\n  b\t\nc")], "")[0]?.shown).toBe("a ⏎ b ⏎ c");
  });
  it("keeps a recorded multi-line command whole and suggests only single lines", () => {
    recordCommand("m", "echo a\necho b");
    expect(historyOf("m").map((e) => e.command)).toEqual(["echo a\necho b"]);
    expect(suggestion(historyOf("m"), "echo a")).toBe("");
    expect(suggestion([at("echo a\necho b"), at("echo apple")], "echo a")).toBe("pple");
  });
});
