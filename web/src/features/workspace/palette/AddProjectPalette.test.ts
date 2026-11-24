import { expect, it } from "vitest";
import { cloneAddress, cloneProblem } from "./AddProjectPalette";

it("turns an owner/name into a GitHub address and leaves every other address as typed", () => {
  expect(cloneAddress(" orbyn-dev/orbyn-notes ")).toBe("https://github.com/orbyn-dev/orbyn-notes.git");
  expect(cloneAddress("https://example.com/team/repo.git")).toBe("https://example.com/team/repo.git");
  expect(cloneAddress("git@github.com:team/repo.git")).toBe("git@github.com:team/repo.git");
  expect(cloneAddress("./local/repo")).toBe("./local/repo");
});

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

it("refuses only what cannot be a clone source, and says why", () => {
  expect(cloneProblem("")).toBe("Enter a Git URL.");
  expect(cloneProblem("   ")).toBe("Enter a Git URL.");
  // The issue's repro: a sentence where an address belongs.
  expect(cloneProblem("bad url with spaces")).toBe("The address cannot hold spaces or control characters.");
  expect(cloneProblem("https://")).toBe("Enter the address to clone from.");
});
