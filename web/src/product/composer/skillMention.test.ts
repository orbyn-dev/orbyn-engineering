import { describe, expect, it } from "vitest";
import { draggedRepository } from "./skillMention";

describe("a dragged file's repository", () => {
  it("reads the client-only id and name another repository's file carries", () => {
    expect(draggedRepository(JSON.stringify({ repoId: "planner", repoName: "orbyn-planner" }))).toEqual({
      repoId: "planner",
      repoName: "orbyn-planner",
    });
  });

  it("reads nothing from the thread's own payload, or from a broken one", () => {
    for (const raw of [
      "",
      "src/main.rs",
      "{",
      "null",
      "[]",
      JSON.stringify({ repoId: "planner" }),
      JSON.stringify({ repoName: "orbyn-planner" }),
      JSON.stringify({ repoId: "", repoName: "orbyn-planner" }),
      JSON.stringify({ repoId: "planner", repoName: "" }),
      JSON.stringify({ repoId: 1, repoName: "orbyn-planner" }),
    ]) {
      expect(draggedRepository(raw), raw).toBeNull();
    }
  });
});
