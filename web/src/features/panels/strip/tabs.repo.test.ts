import { expect, it } from "vitest";
import { fileTabId } from "../../../product/state/ui";
import { tabOf } from "./tabs";

// #1055: two files of the same name in two repositories are told apart by the repository's name; every other tab is named as before.
const names = (id: string) => (id === "planner" ? "orbyn-planner" : undefined);

it("names a file tab of another repository by its file and its repository", () => {
  expect(tabOf(fileTabId({ kind: "thread", path: "docs/README.md", repoId: "planner" }), names).label).toBe("README.md (orbyn-planner)");
});

it("leaves a primary, an ordinary-thread and a not-yet-named repository's tab as its file name alone", () => {
  expect(tabOf(fileTabId({ kind: "thread", path: "docs/README.md" }), names).label).toBe("README.md");
  expect(tabOf(fileTabId({ kind: "thread", path: "docs/README.md" })).label).toBe("README.md");
  expect(tabOf(fileTabId({ kind: "thread", path: "docs/README.md", repoId: "ghost" }), names).label).toBe("README.md");
  expect(tabOf(fileTabId({ kind: "thread", path: "docs/README.md", repoId: "planner" })).label).toBe("README.md");
});
