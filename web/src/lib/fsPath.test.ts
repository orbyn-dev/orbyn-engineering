import { describe, expect, it } from "vitest";
import { parentPath } from "./fsPath";

describe("parentPath", () => {
  it.each([
    ["/Users/me/code", "/Users/me"],
    ["/Users/me", "/Users"],
    ["/Users", "/"],
    ["C:\\Users\\me\\code", "C:\\Users\\me"],
    ["C:\\Users\\me", "C:\\Users"],
    ["C:\\Users", "C:\\"],
    ["\\\\server\\share\\code", "\\\\server\\share"],
    ["/Users/me/code/", "/Users/me"],
    ["C:\\Users\\me\\code\\", "C:\\Users\\me"],
  ])("reads %s as %s", (path, expected) => expect(parentPath(path)).toBe(expected));

  it.each(["/", "C:\\", "C:/"])("stops at the root %s", (path) => expect(parentPath(path)).toBeNull());

  it("has no parent for a bare drive name, which is not a folder", () => expect(parentPath("C:")).toBeNull());
});
