import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useUi } from "../../../../product/state/ui";
import { useWorkspace } from "../../../../product/state/workspace";
import { forgetSkillAdded } from "./marks";
import { SkillProposalPage } from "./SkillProposalPage";
import { SKILL_TEXT, bodyOf, project, proposal, stubFetch, writes, type Answer, type Call, type Write } from "./support";

let calls: Call[] = [];
let answers: { preview?: Answer; projects?: Answer; proposals?: Answer; lessons?: Answer; write?: (one: Write) => Answer };

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
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/skill-proposals?"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
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
  });

  it("keeps the edited text and says the scan refused it, in the design's exact sentence", async () => {
    answers.write = () => ({
      status: 400,
      body: { code: "secret_refused", message: "Not added: this looks like it holds a secret. Edit it and approve again." },
    });
    const user = userEvent.setup();
    open();
    const field = await loaded();
    await user.type(field, "\n4. Use the staging token sk-live-xxxxxxxxxxxx.");
    await user.click(screen.getByRole("button", { name: "Approve" }));
    const refusal = await screen.findByRole("alert");
    expect(refusal).toHaveTextContent("Not added: this looks like it holds a secret. Edit it and approve again.");
    expect(screen.getByRole("textbox", { name: "Skill text" })).toHaveValue(`${SKILL_TEXT}\n4. Use the staging token sk-live-xxxxxxxxxxxx.`);
    expect(window.location.hash).toBe("");
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
});
