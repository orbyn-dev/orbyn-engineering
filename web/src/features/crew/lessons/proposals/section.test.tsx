import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useUi } from "../../../../product/state/ui";
import { setViewportWidth } from "../../../../test-support";
import { forgetSkillAdded, markSkillAdded, takeSkillAdded } from "./marks";
import { moment } from "./model";
import { ProposalsSection } from "./ProposalsSection";
import {
  SKILL_TEXT,
  bodyOf,
  lesson,
  lessons,
  proposal,
  stubFetch,
  writes,
  type Answer,
  type Call,
  type Write,
} from "./support";

let calls: Call[] = [];
let answers: { preview?: Answer; projects?: Answer; proposals?: Answer; lessons?: Answer; write?: (one: Write) => Answer | Promise<Answer> };

const notFound = { status: 404, body: { code: "not_found", message: "No such route." } } as const;
const open = () => render(<ProposalsSection project="p-app" name="orbyn-engineering" />);
const card = () => screen.findByRole("region", { name: "Proposed skill redirect-checks" });
const dialog = () => screen.findByRole("dialog", { hidden: true });

beforeEach(() => {
  calls = [];
  answers = { proposals: { status: 200, body: { proposals: [proposal()] } } };
  stubFetch(answers, calls);
  forgetSkillAdded();
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

  it("says the proposals could not be read, with Try again that reads them again, and never an empty box", async () => {
    answers.proposals = { status: 500, body: { code: "unreadable", message: "The proposals on this device can’t be read." } };
    const user = userEvent.setup();
    open();
    const banner = await screen.findByRole("alert");
    expect(within(banner).getByText("Can’t read the skill proposals")).toBeInTheDocument();
    expect(within(banner).getByText(/unreadable: The proposals on this device can’t be read\./)).toBeInTheDocument();
    expect(screen.queryByText(/No proposals/)).toBeNull();
    answers.proposals = { status: 200, body: { proposals: [proposal()] } };
    await user.click(within(banner).getByRole("button", { name: "Try again" }));
    expect(await card()).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("says so when only the lessons read failed, and Try again reads it again", async () => {
    answers.lessons = { status: 500, body: { code: "unreadable", message: "The lessons on this device can’t be read." } };
    const user = userEvent.setup();
    open();
    const banner = await screen.findByRole("alert");
    expect(within(banner).getByText("Can’t read the lessons")).toBeInTheDocument();
    answers.lessons = { status: 200, body: lessons() };
    await user.click(within(banner).getByRole("button", { name: "Try again" }));
    expect(await card()).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
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
      within(region).getByText("3 active lessons with the same source, check kind and path class", { selector: "b.cx-ppbasedline" }),
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
    const note = within(region)
      .getByText("Based on 2 active lessons now: needs 3.", { selector: "b.cx-pplead" })
      .parentElement as HTMLElement;
    expect(note).toHaveTextContent(
      /Based on 2 active lessons now: needs 3\. A lesson it was based on was deleted or turned off, so the pattern no longer has three active ones\./,
    );
    expect(within(region).getByText("2 active lessons now")).toBeInTheDocument();
    expect(within(region).getByText("(3 when it was proposed)")).toBeInTheDocument();
    expect(within(region).getByRole("button", { name: "Approve" })).toBeDisabled();
    expect(within(region).getByRole("button", { name: "Review" })).toBeEnabled();
    expect(within(region).getByRole("button", { name: "Dismiss" })).toBeEnabled();
    // Only the two lessons still active are listed: the turned-off one is not part of the pattern any more.
    expect(within(region).getByRole("button", { name: "Hide the 2 lessons" })).toBeInTheDocument();
    expect(within(region).getAllByRole("button", { name: "Open this lesson" })).toHaveLength(2);
    expect(within(region).queryByText(/a login link kept a returnTo from another host/)).toBeNull();
  });

  it("shows each unfolded row's evidence ids: task links, an escape link and a revert as plain text", async () => {
    answers.lessons = {
      status: 200,
      body: lessons({
        lessons: [
          lesson({
            id: "les-returnto",
            evidence: [{ kind: "task", id: "5b2d8a3" }, { kind: "task", id: "8c1d4f6" }],
          }),
          lesson({ id: "les-navcleanup", source: "escape", evidence: [{ kind: "escape", id: "9be4410" }] }),
          lesson({ id: "les-loginlink", source: "revert", evidence: [{ kind: "merge", id: "3b9f2c1" }] }),
        ],
      }),
    };
    open();
    const region = await card();
    const task = within(region).getByRole("link", { name: "Task 5b2d8a3" });
    expect(task).toHaveAttribute("href", "#/threads/5b2d8a3");
    expect(within(region).getByRole("link", { name: "Task 8c1d4f6" })).toHaveAttribute("href", "#/threads/8c1d4f6");
    expect(within(region).getByRole("link", { name: "Escape 9be4410" })).toHaveAttribute(
      "href",
      "#/settings/crew/escapes",
    );
    expect(within(region).getByText("Revert 3b9f2c1")).toBeInTheDocument();
    expect(within(region).queryByRole("link", { name: "Revert 3b9f2c1" })).toBeNull();
    // The ids come first, then the middot and the moment the lesson was learned.
    const row = within(region).getByRole("link", { name: "Task 5b2d8a3" }).closest(".cx-pprow") as HTMLElement;
    expect(row.querySelector("small")).toHaveTextContent(/^Task 5b2d8a3, Task 8c1d4f6 · Learned /);
  });

  it("names a lesson's source as the design draws it: a confirmed escape", async () => {
    answers.lessons = {
      status: 200,
      body: lessons({
        lessons: lessons().lessons.map((one) =>
          one.id === "les-navcleanup" ? { ...one, source: "escape" } : one,
        ),
      }),
    };
    open();
    const region = await card();
    expect(within(region).getByText("Confirmed escape")).toBeInTheDocument();
    expect(within(region).queryByText("Escape")).toBeNull();
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

  it("folds the lessons and previews 6 lines at a narrow width, while the wide card stays unfolded at 8", async () => {
    setViewportWidth(640);
    const narrow = open();
    const folded = await card();
    expect(within(folded).getByRole("button", { name: "Show the 3 lessons" })).toHaveAttribute("aria-expanded", "false");
    expect(within(folded).getByText("Proposed text (SKILL.md), first 6 of 14 lines")).toBeInTheDocument();
    expect(within(folded).queryAllByRole("button", { name: "Open this lesson" })).toHaveLength(0);
    narrow.unmount();
    setViewportWidth(1200);
    open();
    const wide = await card();
    expect(within(wide).getByRole("button", { name: "Hide the 3 lessons" })).toHaveAttribute("aria-expanded", "true");
    expect(within(wide).getByText("Proposed text (SKILL.md), first 8 of 14 lines")).toBeInTheDocument();
    expect(within(wide).getAllByRole("button", { name: "Open this lesson" })).toHaveLength(3);
  });

  it("draws the added line the review page hands over, once", async () => {
    answers.proposals = { status: 200, body: { proposals: [proposal({ state: "approved", skill: { name: "redirect-checks" }, rev: 2 })] } };
    markSkillAdded("redirect-checks", 3);
    open();
    expect(await screen.findByText("Added to skills-local.")).toBeInTheDocument();
    expect(screen.getByText("redirect-checks", { selector: "a" })).toHaveAttribute("href", "#/skills/redirect-checks");
    expect(screen.getByText(/The lessons it came from stay here and stop going into briefs while the skill exists\./)).toBeInTheDocument();
    // The line is drawn once: the mark is consumed, and a second mount does not draw it again.
    expect(takeSkillAdded()).toBeNull();
  });
  it("draws no lesson toggle for a stale proposal with no source lesson left", async () => {
    answers.lessons = {
      status: 200,
      body: lessons({
        counts: { active: 0, disabled: 3, hidden: 0 },
        lessons: lessons().lessons.map((one) => ({ ...one, state: "disabled", in_next_brief: false })),
      }),
    };
    answers.proposals = { status: 200, body: { proposals: [proposal({ can_approve: false })] } };
    open();
    const region = await card();
    expect(within(region).getByText("Based on 0 active lessons now: needs 3.", { selector: "b.cx-pplead" })).toBeInTheDocument();
    // Nothing is left to fold out, so the toggle is not drawn at all.
    expect(within(region).queryByText(/^(Hide|Show) the /)).toBeNull();
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
    expect(screen.getByText(/The lessons it came from stay here and stop going into briefs while the skill exists\./)).toBeInTheDocument();
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

  it("says what is true when Approve is off although three active lessons remain", async () => {
    const four = [...lessons().lessons, lesson({ id: "les-fourth", text: "A fourth repeated failure.", created_at: "2026-11-07T17:00:00Z" })];
    answers.lessons = {
      status: 200,
      body: lessons({
        counts: { active: 3, disabled: 1, hidden: 0 },
        lessons: four.map((one) => (one.id === "les-fourth" ? { ...one, state: "disabled", in_next_brief: false } : one)),
      }),
    };
    answers.proposals = {
      status: 200,
      body: { proposals: [proposal({ can_approve: false, lessons: [...proposal().lessons, { id: "les-fourth", evidence: [] }] })] },
    };
    open();
    const region = await card();
    const note = within(region)
      .getByText("Based on 3 active lessons now, 4 when it was proposed.", { selector: "b.cx-pplead" })
      .parentElement as HTMLElement;
    expect(note).not.toHaveTextContent(/needs 3/);
    expect(note).not.toHaveTextContent(/no longer has three/);
    expect(within(region).getByRole("button", { name: "Approve" })).toBeDisabled();
  });

  it("sends the proposal's new revision when the person retries after a refused write", async () => {
    const reread = proposal({
      rev: 2,
      text: SKILL_TEXT.replace("Check where a redirect or returnTo link goes", "Check twice where a redirect goes"),
    });
    let first = true;
    answers.write = ({ method }) => {
      if (method !== "POST") return notFound;
      if (first) {
        first = false;
        // Someone else changed the proposal: the service refuses the old revision, and a new read carries the new one.
        answers.proposals = { status: 200, body: { proposals: [reread] } };
        return { status: 409, body: { code: "stale", message: "This proposal changed since you opened it." } };
      }
      return { status: 200, body: proposal({ state: "approved", skill: { name: "redirect-checks" }, rev: 3 }) };
    };
    const user = userEvent.setup();
    open();
    await card();
    await user.click(screen.getByRole("button", { name: "Approve" }));
    const box = await dialog();
    await user.click(within(box).getByRole("button", { name: "Add to skills-local" }));
    expect(await within(box).findByRole("alert")).toHaveTextContent("This proposal changed since you opened it.");
    // The card behind the dialog shows the text of the new read: the confirm now holds that one.
    await screen.findByText(/Check twice where a redirect goes/, { selector: ".cx-ppdesc" });
    await user.click(within(box).getByRole("button", { name: "Add to skills-local" }));
    expect(await screen.findByText("Added to skills-local.")).toBeInTheDocument();
    expect(writes(calls, "POST").map(({ init }) => bodyOf(init).rev)).toEqual([1, 2]);
  });
});
