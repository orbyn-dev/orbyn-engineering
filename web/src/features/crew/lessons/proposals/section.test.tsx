import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useUi } from "../../../../product/state/ui";
import { moment } from "./model";
import { ProposalsSection } from "./ProposalsSection";
import {
  bodyOf,
  lessons,
  proposal,
  stubFetch,
  writes,
  type Answer,
  type Call,
  type Write,
} from "./support";

let calls: Call[] = [];
let answers: { preview?: Answer; projects?: Answer; proposals?: Answer; lessons?: Answer; write?: (one: Write) => Answer };

const notFound = { status: 404, body: { code: "not_found", message: "No such route." } } as const;
const open = () => render(<ProposalsSection project="p-app" name="orbyn-engineering" />);
const card = () => screen.findByRole("region", { name: "Proposed skill redirect-checks" });
const dialog = () => screen.findByRole("dialog", { hidden: true });

beforeEach(() => {
  calls = [];
  answers = { proposals: { status: 200, body: { proposals: [proposal()] } } };
  stubFetch(answers, calls);
  window.location.hash = "";
});

describe("the Skill proposals section of the Lessons page", () => {
  it("draws nothing at all when the service has no skill-proposal routes", async () => {
    answers.proposals = notFound;
    const { container } = open();
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/skill-proposals?"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(screen.queryByText("Skill proposals")).toBeNull();
  });

  it("draws one proposal: its name and line, what it is based on, its text and the person's three acts", async () => {
    open();
    const region = await card();
    expect(within(region).getByText("Proposed skill")).toBeInTheDocument();
    expect(within(region).getByText("Proposed", { selector: ".cx-apchip" })).toBeInTheDocument();
    expect(within(region).getByText(`Proposed ${moment(proposal().created_at)}`)).toBeInTheDocument();
    expect(within(region).getByText("redirect-checks", { selector: ".cx-ppname" })).toBeInTheDocument();
    expect(
      within(region).getByText("Check where a redirect or returnTo link goes before changing code that follows it."),
    ).toBeInTheDocument();
    expect(
      within(region).getByText("3 active lessons with the same source, check kind and path class"),
    ).toBeInTheDocument();
    expect(within(region).getByText("Applies to")).toBeInTheDocument();
    expect(within(region).getByText(/All your projects/)).toBeInTheDocument();
    expect(within(region).getByText(/Would be saved in/)).toBeInTheDocument();
    expect(within(region).getAllByText(/skills-local/).length).toBeGreaterThan(0);
    expect(within(region).getByText("(device-local)")).toBeInTheDocument();
    expect(within(region).getByText("Proposed text (SKILL.md), first 8 of 14 lines")).toBeInTheDocument();
    expect(within(region).getByRole("textbox", { name: "Proposed skill text" })).toHaveTextContent("# Redirect checks");
    expect(within(region).getByText(/Nothing is added until you approve\./)).toBeInTheDocument();
    expect(within(region).getByText(/filled in from a fixed template, not written by a model/)).toBeInTheDocument();
    expect(within(region).getByText(/Crew is meant to scan the text for secrets/)).toBeInTheDocument();
    for (const name of ["Review", "Approve", "Dismiss"]) expect(within(region).getByRole("button", { name })).toBeInTheDocument();
    expect(screen.getByText("Skill proposals")).toBeInTheDocument();
    expect(screen.getByText("1")).toBeInTheDocument();
    // The three source lessons, from the lessons read, with their own words and the day they were learned.
    expect(within(region).getByText("Checks like a unit test failed on web/nav before: a redirect followed a target outside the app.")).toBeInTheDocument();
    expect(within(region).getAllByRole("button", { name: "Open this lesson" })).toHaveLength(3);
    expect(writes(calls, "POST")).toHaveLength(0);
  });

  it("says there are no proposals yet, in the design's words", async () => {
    answers.proposals = { status: 200, body: { proposals: [] } };
    open();
    expect(await screen.findByText("No proposals")).toBeInTheDocument();
    expect(
      screen.getByText(/Crew proposes a skill when 3 or more active lessons of one repository share a source, a check kind and a path class\./),
    ).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: /Proposed skill/ })).toBeNull();
    expect(writes(calls, "POST")).toHaveLength(0);
  });

  it("draws a proposal whose pattern lost a lesson, with Approve off", async () => {
    answers.lessons = {
      status: 200,
      body: lessons({
        counts: { active: 2, disabled: 1, hidden: 0 },
        lessons: lessons().lessons.map((one) => (one.id === "les-loginlink" ? { ...one, state: "disabled", in_next_brief: false } : one)),
      }),
    };
    answers.proposals = { status: 200, body: { proposals: [proposal({ can_approve: false })] } };
    open();
    const region = await card();
    expect(
      within(region).getByText(
        /Based on 2 active lessons now: needs 3\. A lesson it was based on was deleted or turned off, so the pattern no longer has three active ones\./,
      ),
    ).toBeInTheDocument();
    expect(within(region).getByText("2 active lessons now")).toBeInTheDocument();
    expect(within(region).getByText("(3 when it was proposed)")).toBeInTheDocument();
    expect(within(region).getByRole("button", { name: "Approve" })).toBeDisabled();
    expect(within(region).getByRole("button", { name: "Review" })).toBeEnabled();
    expect(within(region).getByRole("button", { name: "Dismiss" })).toBeEnabled();
  });

  it("hides and shows the lessons it is based on", async () => {
    const user = userEvent.setup();
    open();
    const region = await card();
    expect(within(region).getByRole("button", { name: "Hide the 3 lessons" })).toHaveAttribute("aria-expanded", "true");
    await user.click(within(region).getByRole("button", { name: "Hide the 3 lessons" }));
    expect(within(region).getByRole("button", { name: "Show the 3 lessons" })).toHaveAttribute("aria-expanded", "false");
    expect(within(region).queryAllByRole("button", { name: "Open this lesson" })).toHaveLength(0);
  });

  it("approves through its confirm, sending the text as proposed once, and says the skill was added", async () => {
    answers.write = ({ method, url }) => {
      if (method === "POST" && url === "/api/crew/skill-proposals/prop-redirect-checks/approve") {
        const answer = proposal({ state: "approved", skill: { name: "redirect-checks" }, rev: 2 });
        answers.proposals = { status: 200, body: { proposals: [answer] } };
        return { status: 200, body: answer };
      }
      return notFound;
    };
    const user = userEvent.setup();
    open();
    await card();
    await user.click(screen.getByRole("button", { name: "Approve" }));
    const box = await dialog();
    expect(within(box).getByText("Add redirect-checks to skills-local?")).toBeInTheDocument();
    expect(within(box).getByText(/You have not opened the full text yet\./)).toBeInTheDocument();
    expect(within(box).getByText("As proposed, 14 lines.")).toBeInTheDocument();
    await user.click(within(box).getByRole("button", { name: "Add to skills-local" }));
    expect(await screen.findByText("Added to skills-local.")).toBeInTheDocument();
    expect(screen.getByText("redirect-checks", { selector: "a" })).toHaveAttribute("href", "#/skills/redirect-checks");
    expect(screen.getByText(/is listed on the Skills page as Personal\./)).toBeInTheDocument();
    expect(screen.getByText(/The 3 lessons it came from stay here and stop going into briefs while the skill exists\./)).toBeInTheDocument();
    expect(writes(calls, "POST")).toHaveLength(1);
    expect(bodyOf(writes(calls, "POST")[0]?.init)).toEqual({ text: proposal().text, rev: 1 });
    // The read is made again, and the answered proposal is no longer open.
    expect(await screen.findByText("No proposals")).toBeInTheDocument();
    expect(calls.filter(({ url }) => url.includes("/api/crew/skill-proposals?"))).toHaveLength(2);
  });

  it("dismisses through its confirm and says so, without claiming the pattern is muted", async () => {
    answers.write = ({ method, url }) => {
      if (method === "POST" && url === "/api/crew/skill-proposals/prop-redirect-checks/dismiss") {
        const answer = proposal({ state: "dismissed", rev: 2 });
        answers.proposals = { status: 200, body: { proposals: [answer] } };
        return { status: 200, body: answer };
      }
      return notFound;
    };
    const user = userEvent.setup();
    open();
    await card();
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    const box = await dialog();
    expect(within(box).getByText("Dismiss this proposal?")).toBeInTheDocument();
    expect(within(box).getByText(/Nothing was added to skills-local\./)).toBeInTheDocument();
    expect(within(box).queryByRole("checkbox")).toBeNull();
    await user.click(within(box).getByRole("button", { name: "Dismiss" }));
    await waitFor(() => expect(useUi.getState().toasts.map((one) => one.text)).toContain("Proposal dismissed."));
    expect(bodyOf(writes(calls, "POST")[0]?.init)).toEqual({ rev: 1 });
    expect(await screen.findByText("No proposals")).toBeInTheDocument();
  });

  it("keeps the confirm open with the service's own words when the write was refused", async () => {
    answers.write = () => ({ status: 500, body: { code: "unwritable", message: "skills-local is read only." } });
    const user = userEvent.setup();
    open();
    await card();
    await user.click(screen.getByRole("button", { name: "Approve" }));
    const box = await dialog();
    await user.click(within(box).getByRole("button", { name: "Add to skills-local" }));
    expect(await within(box).findByRole("alert")).toHaveTextContent("skills-local is read only.");
    expect(screen.queryByText("Added to skills-local.")).toBeNull();
    expect(screen.getByRole("region", { name: "Proposed skill redirect-checks" })).toBeInTheDocument();
  });

  it("writes nothing from a read, and the person's own click is the only write", async () => {
    open();
    await card();
    expect(writes(calls, "POST")).toHaveLength(0);
    expect(writes(calls, "PATCH")).toHaveLength(0);
    expect(writes(calls, "PUT")).toHaveLength(0);
    expect(writes(calls, "DELETE")).toHaveLength(0);
  });
});
