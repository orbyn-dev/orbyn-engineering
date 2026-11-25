import { afterEach, expect, it, vi } from "vitest";
import { readRepoTree } from "./api";

afterEach(() => vi.unstubAllGlobals());

/** Serves the file list route's answer and records every URL asked for, without the origin. */
function stub(entries: unknown[]) {
  const urls: string[] = [];
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    urls.push(String(input).replace(/^.*?(?=\/api)/, ""));
    return new Response(JSON.stringify(entries), { status: 200 });
  }));
  return urls;
}

it("reads one repository's tree with its repo_id and shapes the entries as an explorer level", async () => {
  const urls = stub([
    { name: "src", path: "src", is_dir: true, size: 0, ignored: false },
    { name: "a.ts", path: "a.ts", is_dir: false, size: 12, ignored: false },
    { name: "dist", path: "dist", is_dir: true, size: 0, ignored: true },
  ]);
  const tree = await readRepoTree("t 1", "repo/2", "src");
  expect(urls).toEqual(["/api/threads/t%201/files?path=src&repo_id=repo%2F2"]);
  // The route lists names and no change letters, so the level says changes cannot be shown.
  expect(tree).toMatchObject({ path: "src", truncated: false, total: 3, total_at_least: false, changes: "unavailable", read_only: false });
  expect(tree.entries).toEqual([
    { name: "src", path: "src", kind: "directory", size: 0, ignored: false, change: null, has_changes: false },
    { name: "a.ts", path: "a.ts", kind: "file", size: 12, ignored: false, change: null, has_changes: false },
    { name: "dist", path: "dist", kind: "directory", size: 0, ignored: true, change: null, has_changes: false },
  ]);
});

it("leaves repo_id out for the primary repository, as every other call does", async () => {
  const urls = stub([]);
  const tree = await readRepoTree("t", undefined, "");
  expect(urls).toEqual(["/api/threads/t/files?path="]);
  expect(tree.entries).toEqual([]);
});
