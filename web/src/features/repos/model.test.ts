import { describe, expect, it } from "vitest";
import { repoFixtures } from "../../../mock/repos";
import { groupByRepo, canJoinThread, checkCloneSource, cloneText, defaultTicks, isFolderName, isPublishName, plain, repoState, startLabel } from "./model";

describe("repository state", () => {
  it("reads trusted, not trusted and not a Git folder from the row", () => {
    expect(repoFixtures().map(repoState)).toEqual(["trusted", "trusted", "untrusted", "not_git"]);
  });

  it("puts trust before Git: an untrusted folder is untrusted even with no Git (setup-git refuses it with 409), then Set up Git", () => {
    expect(repoState({ is_git: false, trusted: false })).toBe("untrusted");
    expect(repoState({ is_git: true, trusted: false })).toBe("untrusted");
    expect(repoState({ is_git: false, trusted: true })).toBe("not_git");
    expect(repoState({ is_git: true, trusted: true })).toBe("trusted");
  });

  it("lets only a trusted Git folder that is not cloning join a thread", () => {
    const [engineering, , engine, notes] = repoFixtures();
    expect(canJoinThread(engineering!)).toBe(true);
    expect(canJoinThread(engine!)).toBe(false);
    expect(canJoinThread(notes!)).toBe(false);
    expect(canJoinThread({ ...engineering!, clone: { state: "cloning" } })).toBe(false);
  });

  it("ticks every trusted repository up to the limit and none beyond", () => {
    expect(defaultTicks(repoFixtures(), 8)).toEqual(["orbyn-engineering", "orbyn-planner"]);
    expect(defaultTicks(repoFixtures(), 1)).toEqual(["orbyn-engineering"]);
    expect(defaultTicks(repoFixtures(), 0)).toEqual([]);
  });

  it("counts the button", () => {
    expect(startLabel(1)).toBe("Start in 1 repository");
    expect(startLabel(2)).toBe("Start in 2 repositories");
  });
});

describe("untrusted text", () => {
  it.each(["https://github.com/orbyn-dev/orbyn-docs.git", "git@github.com:orbyn-dev/orbyn-docs.git", "ssh://git@host/x/y.git"])("accepts %s", (text) => {
    expect(checkCloneSource(` ${text} `)).toEqual({ ok: true, source: text });
  });

  it.each([["", "Enter"], ["--upload-pack=touch /tmp/x", "space"], ["-oProxyCommand=x", "dash"], ["ext::sh -c x", "space"], ["file:///etc", "https or ssh"], ["fd::1", "https or ssh"], ["orbyn-dev/orbyn-docs", "https or ssh"], ["/etc/passwd", "https or ssh"], ["https://user:secret@host/x.git", "password"], ["ssh://u:p@host/x", "password"], ["https://h/x\ny", "control"], ["git@host:-x", "https or ssh"], ["a b/c", "space"], ["../up/dir", "https or ssh"], ["https://h/x\u0000y", "control"]])("refuses %j", (text, why) => {
    const checked = checkCloneSource(text);
    expect(checked.ok).toBe(false);
    if (!checked.ok) expect(checked.reason).toContain(why);
  });

  it("checks a publish name as owner/name", () => {
    expect(isPublishName("orbyn-dev/orbyn-planner")).toBe(true);
    expect(["", "name", "/x", "a/", "a/b/c", "-a/b"].some(isPublishName)).toBe(false);
  });

  it("accepts a folder name only without a dot-dot, a separator, a drive prefix or a leading dot", () => {
    expect(["orbyn-docs", "a_b.c", "A1"].every(isFolderName)).toBe(true);
    expect(["", "a/b", "a\\b", "..", "a..b", ".x", "C:x", "-x", "a b"].some(isFolderName)).toBe(false);
  });

  it("keeps a title to one line of text", () => {
    expect(plain("a\nb\u0000c")).toBe("a b c");
  });

  it("says a clone is running, never how far: the service's percent is always null, and a stray figure is not drawn", () => {
    const [row] = repoFixtures();
    expect(cloneText(row!)).toBeNull();
    expect(cloneText({ ...row!, clone: { state: "cloning" } })).toBe("Cloning");
    expect(cloneText({ ...row!, clone: { state: "cloning", percent: null } })).toBe("Cloning");
    expect(cloneText({ ...row!, clone: { state: "cloning", percent: 41.6 } })).toBe("Cloning");
    expect(cloneText({ ...row!, clone: { state: "failed", percent: null, message: "No access." } })).toBe("Clone failed. No access.");
  });
});

describe("grouping changed files by repository", () => {
  const repo = (id: string, files?: string[]) => ({ repo_id: id, name: id, branch: "fix/login", added: 1, deleted: 0, trusted: true, files });
  const file = (path: string) => ({ path });

  it("puts each file under the repository that reports it, in the repositories' order, and keeps unknown files last", () => {
    const groups = groupByRepo([file("calendar/feed.ts"), file("auth/redirect.ts"), file("README.md")], [repo("engineering", ["auth/redirect.ts"]), repo("planner", ["calendar/feed.ts"])]);
    expect(groups.map((group) => [group.repo?.repo_id ?? null, group.files.map((entry) => entry.path)])).toEqual([
      ["engineering", ["auth/redirect.ts"]], ["planner", ["calendar/feed.ts"]], [null, ["README.md"]],
    ]);
  });

  it("groups by the repo_id a file carries when it has one, so two repositories can each have a README.md", () => {
    const groups = groupByRepo([{ path: "README.md", repo_id: "planner" }, { path: "README.md", repo_id: "engineering" }, { path: "x.ts", repo_id: "gone" }, { path: "y.ts" }], [repo("engineering"), repo("planner")]);
    expect(groups.map((group) => [group.repo?.repo_id ?? null, group.files.map((entry) => entry.repo_id ?? null)])).toEqual([
      ["engineering", ["engineering"]], ["planner", ["planner"]], [null, ["gone", null]],
    ]);
  });

  it("leaves out a repository with no file in the list, for instance under a filter", () => {
    const groups = groupByRepo([file("auth/redirect.ts")], [repo("engineering", ["auth/redirect.ts"]), repo("planner", ["calendar/feed.ts"])]);
    expect(groups.map((group) => group.repo?.repo_id)).toEqual(["engineering"]);
  });

  it("returns no groups when the service does not say which repository a file belongs to", () => {
    expect(groupByRepo([file("a.ts")], [repo("engineering"), repo("planner")])).toEqual([]);
    expect(groupByRepo([file("a.ts")], [])).toEqual([]);
  });
});
