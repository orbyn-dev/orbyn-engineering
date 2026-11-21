import { describe, expect, it } from "vitest";
import { applyCompletion, completeAt, type CompleteContext, type PathEntry } from "./completions";

const tree: Record<string, PathEntry[]> = {
  "": [{ name: "src", kind: "directory" }, { name: "README.md", kind: "file" }, { name: ".env", kind: "file" }, { name: "scripts", kind: "directory" }],
  "src/": [{ name: "main.tsx", kind: "file" }, { name: "features", kind: "directory" }],
};
const context: CompleteContext = { history: ["deploy --now", "git status", "FOO=1 x"], listDir: (dir) => tree[dir] ?? null };
const labels = (line: string, cursor = line.length, explicit = false) => completeAt(line, cursor, context, explicit)?.items.map((item) => item.label);

describe("completeAt", () => {
  it("completes command names from our table and the person's history", () => {
    expect(labels("g")).toEqual(["git", "grep"]);
    expect(labels("de")).toEqual(["deploy"]);
  });
  it("opens nothing for an empty word until Tab", () => {
    expect(labels("")).toBeUndefined();
    expect(labels("", 0, true)?.length).toBeGreaterThan(3);
  });
  it("completes subcommands and flags", () => {
    expect(labels("git st")).toEqual(["status", "stash"]);
    expect(labels("ls -")).toEqual(["-l", "-a", "-h", "-R", "-t"]);
    expect(labels("cargo --r")).toEqual(["--release"]);
  });
  it("completes paths inside the worktree, folders with a slash, hidden only when asked", () => {
    expect(labels("cat s")).toEqual(["src/", "scripts/"]);
    expect(labels("cat src/")).toEqual(["main.tsx", "features/"]);
    expect(labels("cat .")).toEqual([".env"]);
    expect(completeAt("cat src/m", 9, context)?.items[0]?.insert).toBe("src/main.tsx");
  });
  it("leaves paths it cannot see alone", () => {
    expect(labels("cat ../x")).toBeUndefined();
    expect(labels("cat /etc/h")).toBeUndefined();
    expect(labels("cat nope/")).toBeUndefined();
  });
  it("starts again after a pipe", () => {
    expect(labels("ls | gre")).toEqual(["grep"]);
  });
  it("completes the word before the cursor, not the whole line", () => {
    const result = completeAt("git st --help", 6, context);
    expect(result).toMatchObject({ from: 4, to: 6 });
  });
  it("applies a completion and moves the cursor", () => {
    const result = completeAt("git st", 6, context)!;
    expect(applyCompletion("git st", result, result.items[0]!)).toEqual({ line: "git status ", cursor: 11 });
    const path = completeAt("cat s", 5, context)!;
    expect(applyCompletion("cat s", path, path.items[0]!)).toEqual({ line: "cat src/", cursor: 8 });
  });
});
