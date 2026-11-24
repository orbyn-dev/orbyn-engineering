import { expect, it } from "vitest";
import { cloneProblem } from "./AddProjectPalette";

it("takes every Git source the Add project flow clones from", () => {
  for (const address of [
    "/Users/me/repo.git",
    "C:\\work\\repo.git",
    "//server/share/repo.git",
    "./local/repo",
    "http://127.0.0.1:1234/x.git",
    "https://example.com/team/repo.git",
    "file:///tmp/x.git",
    "ssh://git@example.com/team/repo.git",
    "git@github.com:team/repo.git",
    " orbyn-dev/orbyn-notes ",
  ]) {
    expect(cloneProblem(address), address).toBeNull();
  }
});

it("takes a local path that holds spaces, and refuses an address that does", () => {
  for (const path of [
    "C:\\Users\\me\\My Projects\\repo",
    "~/My Projects/repo",
    "/Users/me/My Projects/repo",
    "./My Projects/repo",
  ]) {
    expect(cloneProblem(path), path).toBeNull();
  }
  // The issue's repro: a sentence where an address belongs, and a relative path that is not one of
  // the folder shapes the dialog takes.
  for (const address of ["bad url with spaces", "My Projects/repo"]) {
    expect(cloneProblem(address), address).toBe("The address cannot hold spaces or control characters.");
  }
});

it("refuses only what cannot be a clone source, and says why", () => {
  expect(cloneProblem("")).toBe("Enter a Git URL.");
  expect(cloneProblem("   ")).toBe("Enter a Git URL.");
  expect(cloneProblem("bad\u0000url")).toBe("The address cannot hold spaces or control characters.");
  expect(cloneProblem("https://")).toBe("Enter the address to clone from.");
});
