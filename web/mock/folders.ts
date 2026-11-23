/**
 * The mock's folder browser (`GET /api/fs/browse`) for a test that needs its own folders, in either platform's style: one
 * listing per path, the home folder answering a bare ask, and an unknown folder listed empty. Without `set`, the sample
 * tree of `fixtures.fsTree` answers.
 */
import type { FsListing } from "../src/api/types.ts";
import { parentPath } from "../src/lib/fsPath.ts";

export function createFoldersMock() {
  let home = "";
  let tree: Map<string, FsListing> | null = null;
  const key = (path: string) => path.replace(/[\\/]+$/, "") || path;
  return {
    reset() {
      home = "";
      tree = null;
    },
    /** Each path lists the subfolders it holds, and its parent comes from its own text; a bare ask lists `homeFolder`. */
    set(homeFolder: string, folders: Record<string, string[]>) {
      home = homeFolder;
      tree = new Map(Object.entries(folders).map(([path, names]) => [key(path), {
        path, parent: parentPath(path),
        entries: names.map((name) => ({ name, path: path.endsWith("\\") || path.endsWith("/") ? `${path}${name}` : `${path}${path.includes("\\") ? "\\" : "/"}${name}`, is_git: false })),
      }]));
    },
    /** The listing for a path, or undefined while the sample tree answers. */
    list(path: string | null): FsListing | undefined {
      if (!tree) return undefined;
      const asked = path ?? home;
      return tree.get(key(asked)) ?? { path: asked, parent: null, entries: [] };
    },
  };
}
