import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useUi } from "../../../../product/state/ui";
import { useWorkspace } from "../../../../product/state/workspace";
import { setViewportWidth } from "../../../../test-support";
import { forgetSkillAdded, takeSkillAdded } from "./marks";
import { SkillProposalPage } from "./SkillProposalPage";
import { SKILL_TEXT, bodyOf, lessons, project, proposal, stubFetch, writes, type Answer, type Call, type Write } from "./support";

let calls: Call[] = [];
let answers: { preview?: Answer; projects?: Answer; proposals?: Answer; lessons?: Answer; write?: (one: Write) => Answer | Promise<Answer> };

const notFound = { status: 404, body: { code: "not_found", message: "No such route." } } as const;
const open = () => render(<SkillProposalPage id="prop-redirect-checks" repo="p-app" />);
const loaded = () => screen.findByRole("textbox", { name: "Skill text" });

beforeEach(() => {
  calls = [];
  answers = { proposals: { status: 200, body: { proposals: [proposal()] } } };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
  forgetSkillAdded();
  window.location.hash = "";
});

describe("the proposed skill's review page", () => {
  it("draws nothing and reads no proposal while Crew Preview is off", async () => {
    answers.preview = notFound;
    const { container } = open();
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/preview"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(calls.some(({ url }) => url.includes("/api/crew/skill-proposals"))).toBe(false);
  });

  it("draws nothing when the service has no skill-proposal routes or the proposal is not open", async () => {
    answers.proposals = notFound;
    const { container } = open();
    // The page first draws its loading state; only the answer takes it away, so wait for that, then assert it is empty.
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/skill-proposals?"))).toBe(true));
    await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
    expect(container).toBeEmptyDOMElement();
  });

  it("draws nothing when the address names a repository the person cannot see", async () => {
    const { container } = render(<SkillProposalPage id="prop-redirect-checks" repo="p-elsewhere" />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/preview"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(calls.some(({ url }) => url.includes("/api/crew/skill-proposals"))).toBe(false);
  });

  it("draws nothing for a proposal that is already answered", async () => {
    answers.proposals = { status: 200, body: { proposals: [proposal({ state: "approved", skill: { name: "redirect-checks" } })] } };
    const { container } = open();
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/skill-proposals?"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
  });

  it("draws the whole text to edit, the proposal's facts and the two acts", async () => {
    open();
    expect(await screen.findByRole("textbox", { name: "Skill text" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^Crew › Proposed skill/ })).toBeInTheDocument();
    expect(screen.getByText(/^Nothing is installed or enabled until you press Approve\./)).toBeInTheDocument();
    expect(screen.getByText(/Until then skills-local is unchanged: the skill is not listed and not used\./)).toBeInTheDocument();
    expect(screen.getByText("The proposal")).toBeInTheDocument();
    expect(screen.getByText("redirect-checks", { selector: "b" })).toBeInTheDocument();
    expect(
      screen.getByText("Check where a redirect or returnTo link goes before changing code that follows it."),
    ).toBeInTheDocument();
    expect(screen.getByText("Based on")).toBeInTheDocument();
    expect(screen.getByText("3 lessons")).toBeInTheDocument();
    expect(screen.getByText("Will be saved in")).toBeInTheDocument();
    const field = screen.getByRole("textbox", { name: "Skill text" });
    expect(field).toHaveValue(SKILL_TEXT);
    expect(field).toHaveAttribute("aria-multiline", "true");
    expect(field).not.toHaveAttribute("readonly");
    expect(screen.getByText("Skill text (SKILL.md). You can edit it.")).toBeInTheDocument();
    expect(screen.getByText(`14 lines · ${SKILL_TEXT.length} characters`)).toBeInTheDocument();
    expect(screen.getByText(/The text is filled in from a fixed template, not written by a model\./)).toBeInTheDocument();
    expect(screen.getByText(/Approve saves the skill in skills-local on this device/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Back to Lessons" })).toHaveAttribute(
      "href",
      "#/settings/crew/lessons?repo=p-app",
    );
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Approve" })).toBeInTheDocument();
    expect(writes(calls, "POST")).toHaveLength(0);
  });

  it("stacks the facts and keeps Approve and Cancel on one row at a narrow width, and leaves the wide page unchanged", async () => {
    setViewportWidth(640);
    const narrow = open();
    await loaded();
    const facts = narrow.container.querySelector<HTMLElement>(".cx-ppfkv") as HTMLElement;
    expect(facts.style.gridTemplateColumns).toBe("minmax(0, 1fr)");
    const cancel = screen.getByRole("button", { name: "Cancel" });
    const approve = screen.getByRole("button", { name: "Approve" });
    expect(cancel.parentElement).toBe(approve.parentElement);
    expect(cancel.parentElement).toHaveClass("cx-acts");
    expect((cancel.parentElement as HTMLElement).style.flexWrap).toBe("nowrap");
    narrow.unmount();
    setViewportWidth(1200);
    const wide = open();
    await loaded();
    expect((wide.container.querySelector(".cx-ppfkv") as HTMLElement).style.gridTemplateColumns).toBe("120px minmax(0, 1fr)");
  });

  it("approves the text the person edited, then leaves for the Lessons page", async () => {
    answers.write = ({ method, url }) =>
      method === "POST" && url === "/api/crew/skill-proposals/prop-redirect-checks/approve"
        ? {
            status: 200,
            body: proposal({ state: "approved", skill: { name: "redirect-checks" }, rev: 2, text: `${SKILL_TEXT}\n\nKeep the check first.` }),
          }
        : notFound;
    const user = userEvent.setup();
    open();
    const field = await loaded();
    await user.type(field, "\nKeep the check first.");
    await user.click(screen.getByRole("button", { name: "Approve" }));
    await waitFor(() => expect(window.location.hash).toBe("#/settings/crew/lessons?repo=p-app"));
    expect(writes(calls, "POST")).toHaveLength(1);
    expect(bodyOf(writes(calls, "POST")[0]?.init)).toEqual({ text: `${SKILL_TEXT}\nKeep the check first.`, rev: 1 });
    // The Lessons page says "Added to skills-local" once, from this mark: the skill's name and its lesson count.
    expect(takeSkillAdded()).toEqual({ name: "redirect-checks", lessons: 3 });
  });

  it("keeps the edited text and relays the service's own secret refusal, never a placeholder", async () => {
    // Distinct words on purpose: if the page hardcodes a sentence, this test cannot pass.
    const refused = "Not saved: a token was found on line 4. Edit it and save again.";
    answers.write = () => ({ status: 400, body: { code: "secret_refused", message: refused } });
    const user = userEvent.setup();
    open();
    const field = await loaded();
    await user.type(field, "\n4. Use the staging token sk-live-xxxxxxxxxxxx.");
    await user.click(screen.getByRole("button", { name: "Approve" }));
    const refusal = await screen.findByRole("alert");
    expect(refusal).toHaveTextContent(refused);
    expect(refusal).not.toHaveTextContent("Not added:");
    const box = screen.getByRole("textbox", { name: "Skill text" });
    expect(box).toHaveValue(`${SKILL_TEXT}\n4. Use the staging token sk-live-xxxxxxxxxxxx.`);
    expect(box).toHaveClass("cx-invalid");
    // The scan refusal has no Try again, so the footer is the review foot, not the refused-write sentence.
    expect(screen.getByText(/Approve saves the skill in skills-local on this device/)).toBeInTheDocument();
    expect(screen.queryByText("Nothing was written. Approve or Try again sends the same text once more.")).toBeNull();
    expect(window.location.hash).toBe("");
  });

  it("offers no Approve for a proposal whose pattern lost a lesson, and says why with the lessons it still rests on", async () => {
    answers.lessons = {
      status: 200,
      body: lessons({
        counts: { active: 2, disabled: 1, hidden: 0 },
        lessons: lessons().lessons.map((one) => (one.id === "les-loginlink" ? { ...one, state: "disabled", in_next_brief: false } : one)),
      }),
    };
    answers.proposals = { status: 200, body: { proposals: [proposal({ can_approve: false })] } };
    open();
    const field = await loaded();
    expect(field).not.toHaveAttribute("readonly");
    expect(screen.getByRole("button", { name: "Approve" })).toBeDisabled();
    const note = screen.getByText(/^This proposal is stale:/).parentElement as HTMLElement;
    await waitFor(() =>
      expect(note).toHaveTextContent(
        "This proposal is stale: it now rests on 2 lessons and needs 3. Go back to dismiss it.",
      ),
    );
    expect(screen.getByText("(2 lessons now, 3 when it was proposed)")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    expect(screen.queryByText(/Approve saves the skill in skills-local/)).toBeNull();
    expect(writes(calls, "POST")).toHaveLength(0);
  });

  it("says a lesson was deleted or turned off while the lessons read has not answered", async () => {
    answers.lessons = "never";
    answers.proposals = { status: 200, body: { proposals: [proposal({ can_approve: false })] } };
    open();
    await loaded();
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/lessons?"))).toBe(true));
    const note = screen.getByText(/^This proposal is stale:/).parentElement as HTMLElement;
    expect(note).toHaveTextContent(
      "This proposal is stale: a lesson it was based on was deleted or turned off. Go back to dismiss it.",
    );
    expect(note).not.toHaveTextContent(/now rests on/);
    expect(screen.getByText("3 lessons")).toBeInTheDocument();
  });
  it("keeps the text when the write was refused, and Try again sends it again", async () => {
    answers.write = () => ({
      status: 500,
      body: { code: "unwritable", message: "skills-local refused the write: redirect-checks could not be created (permission denied)." },
    });
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Approve" }));
    expect(await screen.findByText("Can’t add the skill")).toBeInTheDocument();
    expect(screen.getByText(/skills-local refused the write: redirect-checks could not be created \(permission denied\)\./)).toBeInTheDocument();
    expect(screen.getByText(/Nothing was added\. Your text is still here and the proposal is still open\./)).toBeInTheDocument();
    expect(screen.getByText("Nothing was written. Approve or Try again sends the same text once more.")).toBeInTheDocument();
    expect(screen.queryByText(/Approve saves the skill in skills-local/)).toBeNull();
    expect(screen.getByRole("textbox", { name: "Skill text" })).toHaveValue(SKILL_TEXT);
    answers.write = () => ({ status: 200, body: proposal({ state: "approved", skill: { name: "redirect-checks" }, rev: 2 }) });
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(writes(calls, "POST")).toHaveLength(2));
  });

  it("says it is adding the skill and takes no second edit until the write answers", async () => {
    answers.write = () => "never";
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Approve" }));
    expect(await screen.findByText("Adding the skill. It is not listed or used until the write has finished.")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Skill text" })).toHaveAttribute("readonly");
    expect(screen.getByText("Adding redirect-checks to skills-local.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Adding…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(screen.getByText(/The text was filled in from a fixed template, not written by a model\./)).toBeInTheDocument();
  });

  it("goes back to the Lessons page, telling why, when the proposal moved on", async () => {
    answers.write = () => ({ status: 409, body: { code: "stale", message: "This proposal changed since you opened it." } });
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Approve" }));
    await waitFor(() => expect(window.location.hash).toBe("#/settings/crew/lessons?repo=p-app"));
    expect(useUi.getState().toasts.map((one) => one.text)).toContain("This proposal changed since you opened it.");
    expect(writes(calls, "POST")).toHaveLength(1);
  });

  it("cancels back to the Lessons page and sends nothing", async () => {
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(window.location.hash).toBe("#/settings/crew/lessons?repo=p-app");
    expect(writes(calls, "POST")).toHaveLength(0);
  });

  it("says the outcome is unknown, not that nothing was added, when the connection failed mid-write", async () => {
    answers.write = () => "network";
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Approve" }));
    expect(await screen.findByText("Can’t add the skill")).toBeInTheDocument();
    expect(screen.getByText(/Whether the skill was added is not known/)).toBeInTheDocument();
    expect(screen.queryByText(/Nothing was added\./)).toBeNull();
    expect(screen.queryByText(/Nothing was written\./)).toBeNull();
    // The proposal is read again, to see where the write left it.
    await waitFor(() => expect(calls.filter(({ url }) => url.includes("/api/crew/skill-proposals?"))).toHaveLength(2));
  });

  it("does not move the page when the person left while the write was still out", async () => {
    let answer: (one: Answer) => void = () => undefined;
    answers.write = () => new Promise<Answer>((resolve) => (answer = resolve));
    const user = userEvent.setup();
    const { unmount } = open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Approve" }));
    await screen.findByText("Adding the skill. It is not listed or used until the write has finished.");
    // The person follows Back to Lessons, then opens another proposal's page; this review is gone.
    unmount();
    window.location.hash = "#/settings/crew/lessons/proposal?repo=p-app&id=prop-other";
    answer({ status: 200, body: proposal({ state: "approved", skill: { name: "redirect-checks" }, rev: 2 }) });
    // The write still counts: its mark is kept for the Lessons page, but the finished request does not navigate.
    await waitFor(() => expect(takeSkillAdded()).toEqual({ name: "redirect-checks", lessons: 3 }));
    expect(window.location.hash).toBe("#/settings/crew/lessons/proposal?repo=p-app&id=prop-other");
  });
});
