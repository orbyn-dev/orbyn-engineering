import { describe, expect, it } from "vitest";
import type { Lesson, LessonDetail, LessonList } from "../src/features/crew/lessons/types.ts";
import { createMockApi } from "./api.ts";

const api = (scenario: string) => {
  const mock = createMockApi();
  mock.reset(scenario as Parameters<ReturnType<typeof createMockApi>["reset"]>[0]);
  return mock;
};

type Mock = ReturnType<typeof api>;

const lessonsOf = (mock: Mock): Lesson[] => {
  const answer = mock.handle("GET", "/api/crew/lessons?repo=p-app&filter=all");
  expect(answer.status).toBe(200);
  return (answer.body as LessonList).lessons;
};

const lessonOf = (mock: Mock, id: string): Lesson => {
  const answer = mock.handle("GET", `/api/crew/lessons/${id}`);
  expect(answer.status).toBe(200);
  return (answer.body as LessonDetail).lesson;
};

describe("the lessons mock's failing writes", () => {
  it("refuses an add the scan says holds a key, in the contract's own sentence, and stores nothing", () => {
    const mock = api("crew-lessons-add-secret");
    const before = lessonsOf(mock).length;
    const refused = mock.handle("POST", "/api/crew/lessons", {
      scope: "repo",
      repo: "p-app",
      text: "Use sk-live-4f9a2b1c in the login test.",
    });
    expect(refused.status).toBe(400);
    expect(refused.body).toEqual({
      code: "secret_refused",
      // docs/crew-lessons.md, "Errors": the screen relays this sentence, never its own.
      message: "Not saved: this looks like it holds a secret. Edit it and save again.",
    });
    expect(lessonsOf(mock)).toHaveLength(before);

    // A text the scan passes is saved as usual: the refusal is the scan's, not the scenario refusing every add.
    const saved = mock.handle("POST", "/api/crew/lessons", {
      scope: "repo",
      repo: "p-app",
      text: "Run the web tests before you open a pull request.",
    });
    expect(saved.status).toBe(201);
    expect((saved.body as Lesson).source).toBe("person");
    expect(lessonsOf(mock)).toHaveLength(before + 1);
  });

  it("fails the add with a 500 of its own and stores nothing", () => {
    const mock = api("crew-lessons-add-fails");
    const before = lessonsOf(mock).length;
    const failed = mock.handle("POST", "/api/crew/lessons", {
      scope: "repo",
      repo: "p-app",
      text: "Run the web tests before you open a pull request.",
    });
    expect(failed.status).toBe(500);
    expect(failed.body).toEqual({ code: "write_failed", message: "The service could not save the lesson. Try again." });
    expect(lessonsOf(mock)).toHaveLength(before);
  });

  it("fails the card's edit save with a 500 of its own and leaves the lesson as it was", () => {
    const mock = api("crew-lessons-save-fails");
    const before = lessonOf(mock, "les-dates");
    const failed = mock.handle("PATCH", "/api/crew/lessons/les-dates", {
      text: "Date formatting goes through the one formatter, and the tests check it.",
      rev: before.rev,
    });
    expect(failed.status).toBe(500);
    expect(failed.body).toEqual({ code: "write_failed", message: "The service could not save the lesson. Try again." });
    const after = lessonOf(mock, "les-dates");
    expect(after.text).toBe(before.text);
    expect(after.rev).toBe(before.rev);
  });

  it("fails the delete with a 500 of its own, keeps the lesson and opens no Undo window", () => {
    const mock = api("crew-lessons-delete-fails");
    const before = lessonOf(mock, "les-returnto");
    const failed = mock.handle("DELETE", `/api/crew/lessons/les-returnto?rev=${before.rev}`);
    expect(failed.status).toBe(500);
    expect(failed.body).toEqual({ code: "write_failed", message: "The service could not delete the lesson. Try again." });
    expect(lessonsOf(mock).map((one) => one.id)).toContain("les-returnto");
    // The delete never happened, so the contract's Undo answers not_found rather than restoring it.
    const undo = mock.handle("POST", "/api/crew/lessons/les-returnto/undo");
    expect(undo.status).toBe(404);
  });
});
