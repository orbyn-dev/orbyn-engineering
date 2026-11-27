import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { routeHref } from "../../../product/state/route";
import { useWorkspace } from "../../../product/state/workspace";
import { LiveLessons } from "./LiveLessons";
import { lesson, list, project, stubFetch, type Answer, type Call } from "./support";

let calls: Call[] = [];
let answers: { preview?: Answer; projects?: Answer; list?: Answer; hidden?: Answer; settings?: Answer };

/** One lesson the proposals approved into a skill, and one that stayed an ordinary lesson. */
const rows = () => [
  lesson({
    id: "les-skill",
    text: "A lesson that became a skill.",
    skill: "redirect-checks",
    in_next_brief: false,
    uses: 2,
    last_used_at: "2026-11-07T16:05:00Z",
  }),
  lesson({
    id: "les-plain",
    text: "A lesson that stayed a lesson.",
    skill: null,
  }),
];

beforeEach(() => {
  calls = [];
  answers = { list: { status: 200, body: list({ lessons: rows() }) } };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
});

describe("a lesson the proposals approved into a skill", () => {
  it("draws the Now a skill chip, the skill's link and that it is not used now", async () => {
    render(<LiveLessons view="list" initial="p-app" />);
    const link = await screen.findByRole("link", { name: "redirect-checks" });
    expect(link).toHaveAttribute("href", routeHref({ name: "skills", skill: "redirect-checks" }));
    const row = link.closest(".cx-lsrow") as HTMLElement;
    expect(row).toHaveTextContent("Now a skill");
    expect(row).toHaveTextContent("Used in 2 briefs · not used now, it is a skill");
  });

  it("draws no chip and no skill link on a lesson that did not become one", async () => {
    render(<LiveLessons view="list" initial="p-app" />);
    const plain = (await screen.findByText("A lesson that stayed a lesson.")).closest(".cx-lsrow") as HTMLElement;
    expect(plain).not.toHaveTextContent("Now a skill");
    expect(plain.querySelector(".cx-lsskill")).toBeNull();
    expect(screen.getAllByText("Now a skill")).toHaveLength(1);
  });
});
