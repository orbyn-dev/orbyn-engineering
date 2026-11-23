import { describe, expect, it } from "vitest";
import type { LessonList } from "../src/features/crew/lessons/types.ts";
import type { SkillProposal, SkillProposalList } from "../src/features/crew/lessons/proposals/types.ts";
import { createMockApi } from "./api.ts";

const api = (scenario: string) => {
  const mock = createMockApi();
  mock.reset(scenario as Parameters<ReturnType<typeof createMockApi>["reset"]>[0]);
  return mock;
};

const proposalsOf = (scenario: string): SkillProposal[] => {
  const answer = api(scenario).handle("GET", "/api/crew/skill-proposals?repo=p-app");
  expect(answer.status).toBe(200);
  return (answer.body as SkillProposalList).proposals;
};

describe("the skill-proposals mock", () => {
  it("serves the design's open proposal over the three repeated failures", () => {
    const [one] = proposalsOf("crew-proposals");
    expect(one?.state).toBe("open");
    expect(one?.can_approve).toBe(true);
    expect(one?.lessons).toEqual(["les-returnto", "les-navcleanup", "les-loginlink"]);
    expect(one?.text.split("\n")[1]).toBe("name: redirect-checks");
    expect(one?.text.split("\n").length).toBe(14);
  });

  it("serves no proposal for nothing proposed yet, and for any scenario that is not about proposals", () => {
    expect(proposalsOf("crew-proposals-none")).toEqual([]);
    expect(proposalsOf("crew")).toEqual([]);
  });

  it("serves a proposal whose source lesson was turned off, with Approve off", () => {
    const [one] = proposalsOf("crew-proposals-stale");
    expect(one?.can_approve).toBe(false);
    const lessons = api("crew-proposals-stale").handle("GET", "/api/crew/lessons?repo=p-app&filter=all").body as LessonList;
    expect(lessons.lessons.find((lesson) => lesson.id === "les-loginlink")?.state).toBe("disabled");
  });

  it("serves the approved skill and the lessons that now carry it", () => {
    const [one] = proposalsOf("crew-proposals-approved");
    expect(one?.state).toBe("approved");
    expect(one?.skill).toEqual({ name: "redirect-checks" });
    const lessons = api("crew-proposals-approved").handle("GET", "/api/crew/lessons?repo=p-app&filter=all").body as LessonList;
    expect(lessons.lessons.every((lesson) => lesson.skill?.name === "redirect-checks")).toBe(true);
    expect(lessons.lessons.every((lesson) => lesson.in_next_brief === false)).toBe(true);
  });

  it("approves once, marking the lessons as a skill, then refuses a second answer", () => {
    const mock = api("crew-proposals");
    const first = mock.handle("POST", "/api/crew/skill-proposals/prop-redirect-checks/approve", {
      text: "---\nname: redirect-checks\ndescription: \"A line.\"\n---\n\n# Redirect checks\n",
      rev: 1,
    });
    expect(first.status).toBe(200);
    expect((first.body as SkillProposal).state).toBe("approved");
    expect((first.body as SkillProposal).skill).toEqual({ name: "redirect-checks" });
    const lessons = mock.handle("GET", "/api/crew/lessons?repo=p-app&filter=all").body as LessonList;
    expect(lessons.lessons.filter((lesson) => lesson.skill !== null)).toHaveLength(3);
    expect(mock.handle("POST", "/api/crew/skill-proposals/prop-redirect-checks/approve", { text: "x", rev: 2 }).status).toBe(409);
    expect(mock.handle("POST", "/api/crew/skill-proposals/prop-redirect-checks/dismiss", { rev: 2 }).status).toBe(409);
  });

  it("refuses a wrong revision and a proposal that is no longer open, as the contract says", () => {
    const mock = api("crew-proposals");
    const stale = mock.handle("POST", "/api/crew/skill-proposals/prop-redirect-checks/approve", { text: "x", rev: 9 });
    expect(stale.status).toBe(409);
    expect((stale.body as { code: string }).code).toBe("stale");
    const dismissed = mock.handle("POST", "/api/crew/skill-proposals/prop-redirect-checks/dismiss", { rev: 1 });
    expect(dismissed.status).toBe(200);
    const again = mock.handle("POST", "/api/crew/skill-proposals/prop-redirect-checks/dismiss", { rev: 2 });
    expect(again.status).toBe(409);
    expect((again.body as { code: string }).code).toBe("not_open");
  });

  it("refuses a text that holds a secret, in the design's sentence", () => {
    const answer = api("crew-proposals-secret").handle("POST", "/api/crew/skill-proposals/prop-redirect-checks/approve", {
      text: "Use the staging token sk-live-xxxxxxxxxxxx.",
      rev: 1,
    });
    expect(answer.status).toBe(400);
    expect((answer.body as { code: string }).code).toBe("secret_refused");
    expect((answer.body as { message: string }).message).toBe(
      "Not added: this looks like it holds a secret. Edit it and approve again.",
    );
  });

  it("serves a refused write, as a skills-local permission problem is", () => {
    const answer = api("crew-proposals-approve-fails").handle("POST", "/api/crew/skill-proposals/prop-redirect-checks/approve", {
      text: "---\nname: redirect-checks\n---\n",
      rev: 1,
    });
    expect(answer.status).toBe(500);
    expect((answer.body as { message: string }).message).toContain("skills-local refused the write");
  });

  it("names the repository a read must carry", () => {
    expect(api("crew-proposals").handle("GET", "/api/crew/skill-proposals").status).toBe(400);
  });
});
