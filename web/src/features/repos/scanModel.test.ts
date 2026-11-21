import { expect, it } from "vitest";
import { canAddScanned, checkBranchName, cleanScan, cleanTaskFolder, SCAN_ENTRY_CAP } from "./scanModel";

const entry = (name: string, extra: Record<string, unknown> = {}) => ({ name, path: `/w/${name}`, is_git: true, source: "child", in_project: false, refused: null, ...extra });

it("keeps a scan's entries and reads every flag as a plain boolean", () => {
  const scan = cleanScan({ path: "/w", is_git: false, code_workspace_file: null, folders: [entry("a"), entry("b", { in_project: true, refused: "missing" })], truncated: false });
  expect(scan?.folders.map((one) => one.name)).toEqual(["a", "b"]);
  expect(scan?.folders[1]).toMatchObject({ in_project: true, refused: "missing" });
  expect(scan?.truncated).toBe(false);
});

it("caps the list at 64 entries and says it was cut, and caps and flattens text", () => {
  const many = Array.from({ length: 80 }, (_, index) => entry(`r${index}`));
  const scan = cleanScan({ path: "/w", is_git: false, code_workspace_file: null, folders: many, truncated: false });
  expect(scan?.folders).toHaveLength(SCAN_ENTRY_CAP);
  expect(scan?.truncated).toBe(true);
  const long = cleanScan({ path: "/w", folders: [entry("x".repeat(2000), { path: "/w/a\nb\u0000c" })] });
  expect(long?.folders[0]?.name).toHaveLength(512);
  expect(long?.folders[0]?.path).toBe("/w/a b c");
});

it("drops an entry with an unknown source or no name, and treats an unknown refusal as a refusal", () => {
  const scan = cleanScan({ path: "/w", folders: [entry("a", { source: "other" }), entry(""), { name: 4, path: "/x", source: "child" }, entry("ok", { refused: "something-new" })] });
  expect(scan?.folders.map((one) => one.name)).toEqual(["ok"]);
  expect(scan?.folders[0]?.refused).not.toBeNull();
  expect(canAddScanned(scan!.folders[0]!)).toBe(false);
});

it("reads own keys only, so an inherited or __proto__ key is never a field", () => {
  const polluted = JSON.parse('{"path":"/w","folders":[{"name":"a","path":"/a","source":"child","__proto__":{"refused":"outside"}}],"__proto__":{"truncated":true}}');
  const scan = cleanScan(polluted);
  expect(scan?.truncated).toBe(false);
  expect(scan?.folders[0]?.refused).toBeNull();
  expect(({} as Record<string, unknown>).truncated).toBeUndefined();
});

it("answers null for something that is not a scan", () => {
  expect(cleanScan(null)).toBeNull();
  expect(cleanScan({ ok: true })).toBeNull();
  expect(cleanScan({ path: "/w", folders: "x" })).toBeNull();
});

it("adds only an entry that is neither refused nor already in a project", () => {
  expect(canAddScanned(cleanScan({ path: "/", folders: [entry("a")] })!.folders[0]!)).toBe(true);
  expect(canAddScanned(cleanScan({ path: "/", folders: [entry("a", { in_project: true })] })!.folders[0]!)).toBe(false);
});

it("reads a task folder, caps it at 8 repositories and drops a malformed one", () => {
  const repos = Array.from({ length: 10 }, (_, index) => ({ repo_id: `r${index}`, name: `r${index}`, branch: "feat/x", added: 1, deleted: 0, trusted: true }));
  const folder = cleanTaskFolder({ root: "/t/1", branch: "feat/x", repos: [...repos, { name: 3 }], max_per_thread: 8 });
  expect(folder?.repos).toHaveLength(8);
  expect(cleanTaskFolder({ root: "", branch: "x", repos: [] })).toBeNull();
  expect(cleanTaskFolder("nope")).toBeNull();
});

it.each(["feat/login", "fix_1", "release/1.2", "a"])("accepts the branch name %s", (name) => {
  expect(checkBranchName(name)).toEqual({ ok: true, name });
});

it.each(["", "  ", "a b", "-x", "a..b", "a//b", "/a", "a/", "a.", "a.lock", "x/.hidden", "@", "a@{b", "a~1", "a^", "a:b", "a?b", "a*b", "a[b", "a\\b", "a\nb"])("refuses the branch name %j", (name) => {
  expect(checkBranchName(name).ok).toBe(false);
});
