import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import type { Project } from "../../api/types";
import type { SkillPackStatus } from "../../generated/SkillPackStatus";
import { useWorkspace } from "../../product/state/workspace";
import { SkillsPage } from "./SkillsPage";
import { resetSkillsStore } from "./skillsStore";

/** The design's sample SKILL.md, as the proposal read carries it. */
const SKILL_TEXT = [
  "---",
  "name: redirect-checks",
  'description: "Check where a redirect or returnTo link goes before changing code that follows it."',
  "---",
  "",
  "# Redirect checks",
  "",
  "Use when a task changes how the app follows a redirect or a returnTo link.",
  "",
  "1. Keep the target inside the app.",
  "2. Drop a returnTo that points to another host.",
  "3. Before renaming a helper that builds returnTo links, look for old links that still use it.",
  "",
  "Filled in from a fixed template, using 3 lessons from orbyn-engineering.",
].join("\n");

const project = { id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project;

const proposal = (over: Record<string, unknown> = {}) => ({
  id: "prop-redirect-checks",
  repo: "p-app",
  state: "open",
  text: SKILL_TEXT,
  lessons: ["les-returnto", "les-navcleanup", "les-loginlink"],
  can_approve: true,
  skill: null,
  rev: 1,
  created_at: "2026-11-07T16:31:00Z",
  ...over,
});

const catalog: SkillPackStatus = {
  version: "123",
  digest: "123",
  signature_valid: true,
  source: "/library",
  skills: [
    { name: "architect", category: "Engineering practice", description: "Shape contracts", enabled: true },
    { name: "design-review", category: "Product design", description: "Inspect interfaces", enabled: true },
    { name: "tdd", category: "Engineering practice", description: "Write the test first", enabled: true },
  ],
};

type Answer = { status: number; body?: unknown } | "never";
let calls: string[] = [];
let answers: { preview?: Answer; proposals?: Answer };

function stubFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string) => {
      const url = String(input);
      calls.push(url);
      if (url.includes("/api/crew/preview")) return json(answers.preview ?? { status: 200, body: { enabled: true } });
      if (url.includes("/api/crew/skill-proposals?")) return json(answers.proposals ?? { status: 200, body: { proposals: [] } });
      return json({ status: 404, body: { code: "not_found", message: "No such route." } });
    }),
  );
}
const json = (answer: Answer) => {
  if (answer === "never") return new Promise<Response>(() => undefined) as unknown as Response;
  return new Response(answer.body === undefined ? null : JSON.stringify(answer.body), { status: answer.status });
};

beforeEach(() => {
  calls = [];
  answers = {};
  resetSkillsStore();
  vi.spyOn(api, "skills").mockResolvedValue(catalog);
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch();
  window.location.hash = "#/skills";
});
afterEach(() => {
  vi.restoreAllMocks();
});

it("draws no Crew group when the service has no skill-proposal routes", async () => {
  answers.proposals = { status: 404, body: { code: "not_found", message: "Not found" } };
  render(<SkillsPage />);
  expect(await screen.findByRole("button", { name: /architect/ })).toBeInTheDocument();
  await waitFor(() => expect(calls.some((url) => url.includes("/api/crew/skill-proposals?"))).toBe(true));
  expect(screen.queryByRole("heading", { name: /^Proposed/ })).toBeNull();
  expect(screen.queryByRole("heading", { name: /^Added from lessons/ })).toBeNull();
  expect(screen.queryByText(/Nothing added from lessons yet\./)).toBeNull();
  expect(screen.getByText("3 built in")).toBeInTheDocument();
});

it("draws no Crew group while Crew Preview is off", async () => {
  answers.preview = { status: 404, body: { code: "not_found", message: "Not found" } };
  answers.proposals = { status: 200, body: { proposals: [proposal()] } };
  render(<SkillsPage />);
  expect(await screen.findByRole("button", { name: /architect/ })).toBeInTheDocument();
  await waitFor(() => expect(calls.some((url) => url.includes("/api/crew/preview"))).toBe(true));
  expect(calls.some((url) => url.includes("/api/crew/skill-proposals?"))).toBe(false);
  expect(screen.queryByRole("heading", { name: /^Proposed/ })).toBeNull();
  expect(screen.getByText("3 built in")).toBeInTheDocument();
});

it("draws a proposal waiting for the person as a Proposed card, and Review opens its page", async () => {
  answers.proposals = { status: 200, body: { proposals: [proposal()] } };
  const user = userEvent.setup();
  render(<SkillsPage />);
  expect(await screen.findByRole("heading", { name: /^Proposed/ })).toBeInTheDocument();
  expect(screen.getByText("3 built in · 1 proposed")).toBeInTheDocument();
  const card = screen.getByRole("region", { name: "Proposed skill redirect-checks" });
  expect(within(card).getByText("redirect-checks")).toBeInTheDocument();
  expect(
    within(card).getByText("Check where a redirect or returnTo link goes before changing code that follows it."),
  ).toBeInTheDocument();
  expect(within(card).getByText("Proposed")).toBeInTheDocument();
  expect(within(card).getByText("From 3 lessons · Personal")).toBeInTheDocument();
  expect(
    screen.getByText(/Crew proposed this from 3 or more active lessons on this device\. Nothing is added until you review and approve it;/),
  ).toBeInTheDocument();
  await user.click(within(card).getByRole("button", { name: "Review" }));
  expect(window.location.hash).toBe("#/settings/crew/lessons/proposal?repo=p-app&id=prop-redirect-checks");
});

it("lists a skill approved from lessons under Added from lessons, as Personal", async () => {
  answers.proposals = {
    status: 200,
    body: { proposals: [proposal({ state: "approved", skill: { name: "redirect-checks" }, rev: 2 })] },
  };
  const user = userEvent.setup();
  render(<SkillsPage />);
  expect(await screen.findByRole("heading", { name: /^Added from lessons/ })).toBeInTheDocument();
  expect(screen.getByText("3 built in · 1 added from lessons")).toBeInTheDocument();
  const group = screen.getByRole("heading", { name: /^Added from lessons/ }).closest(".skills-group") as HTMLElement;
  expect(within(group).getByText("1")).toBeInTheDocument();
  expect(
    within(group).getByText(
      "Available to engines through Orbyn's skills tools. Applies to all your projects. Saved in skills-local on this device, not in your repository.",
    ),
  ).toBeInTheDocument();
  expect(within(group).getByText("redirect-checks")).toBeInTheDocument();
  expect(within(group).getByText("Personal")).toBeInTheDocument();
  expect(within(group).getByText(/From 3 lessons in/)).toBeInTheDocument();
  await user.click(within(group).getByRole("button", { name: "orbyn-engineering" }));
  expect(window.location.hash).toBe("#/settings/crew/lessons?repo=p-app");
  expect(screen.getByRole("heading", { name: /^Built in/ })).toBeInTheDocument();
});

it("says where an approved skill will be listed while none is, and draws no Added heading", async () => {
  answers.proposals = { status: 200, body: { proposals: [] } };
  render(<SkillsPage />);
  expect(await screen.findByRole("heading", { name: /^Built in/ })).toBeInTheDocument();
  expect(
    screen.getByText(
      "Nothing added from lessons yet. A skill you approve from a Crew proposal is listed here, above Built in, as Personal.",
    ),
  ).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: /^Added from lessons/ })).toBeNull();
  expect(screen.getByText("3 built in")).toBeInTheDocument();
});

it("draws one skill added from lessons on its own page", async () => {
  window.location.hash = "#/skills/redirect-checks";
  answers.proposals = {
    status: 200,
    body: { proposals: [proposal({ state: "approved", skill: { name: "redirect-checks" }, rev: 2 })] },
  };
  render(<SkillsPage skill="redirect-checks" />);
  expect(await screen.findByText("Personal · Added from 3 lessons · All your projects")).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "redirect-checks" })).toBeInTheDocument();
  expect(screen.getByText("Personal · Added from 3 lessons · All your projects")).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "What it does" })).toBeInTheDocument();
  expect(screen.getByText("Check where a redirect or returnTo link goes before changing code that follows it.")).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Where it came from" })).toBeInTheDocument();
  expect(screen.getByText("Added by you.")).toBeInTheDocument();
  expect(screen.getByText(/The lessons stay on the Lessons page, marked Now a skill\./)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Use in a new thread" })).toBeInTheDocument();
});

it("keeps a just-added skill's address while its read is still running, with a loading line, not a blank page", async () => {
  window.location.hash = "#/skills/redirect-checks";
  answers.proposals = "never";
  render(<SkillsPage skill="redirect-checks" />);
  await waitFor(() => expect(calls.some((url) => url.includes("/api/crew/skill-proposals?"))).toBe(true));
  // The address is not rewritten: the proposals read has not said whether the skill exists, and the page says it is waiting.
  expect(window.location.hash).toBe("#/skills/redirect-checks");
  expect(screen.getByRole("heading", { name: "redirect-checks" })).toBeInTheDocument();
  expect(screen.getByRole("status", { name: "Loading skills" })).toBeInTheDocument();
});

it("keeps a bookmark to a skill when the Crew read failed, since a failed read proves nothing", async () => {
  window.location.hash = "#/skills/redirect-checks";
  answers.proposals = { status: 500, body: { code: "internal", message: "The service failed." } };
  render(<SkillsPage skill="redirect-checks" />);
  await waitFor(() => expect(calls.some((url) => url.includes("/api/crew/skill-proposals?"))).toBe(true));
  // One turn of the event loop after the answer, so the read's failure has been drawn before the address is judged.
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(window.location.hash).toBe("#/skills/redirect-checks");
});

it("rewrites a bookmark to a skill that does not exist once the Crew read answered without it", async () => {
  window.location.hash = "#/skills/redirect-checks";
  render(<SkillsPage skill="redirect-checks" />);
  await waitFor(() => expect(window.location.hash).toBe("#/skills"));
});

it("draws the Crew groups when the built-in catalog is empty", async () => {
  vi.spyOn(api, "skills").mockResolvedValue({ ...catalog, skills: [] });
  answers.proposals = { status: 200, body: { proposals: [proposal()] } };
  render(<SkillsPage />);
  expect(await screen.findByRole("heading", { name: /^Proposed/ })).toBeInTheDocument();
  expect(screen.getByRole("region", { name: "Proposed skill redirect-checks" })).toBeInTheDocument();
  expect(screen.getByText("No skills in this workspace yet.")).toBeInTheDocument();
});

it("keeps the Crew groups off a filtered page", async () => {
  answers.proposals = { status: 200, body: { proposals: [proposal()] } };
  const user = userEvent.setup();
  render(<SkillsPage />);
  expect(await screen.findByRole("heading", { name: /^Proposed/ })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Product design" }));
  expect(screen.queryByRole("heading", { name: /^Proposed/ })).toBeNull();
  expect(screen.queryByRole("heading", { name: /^Built in/ })).toBeNull();
  expect(screen.getByText("3 built in")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "All" }));
  expect(await screen.findByRole("heading", { name: /^Proposed/ })).toBeInTheDocument();
});
it("names an added skill by the service's own skill name, not the text's front matter", async () => {
  answers.proposals = {
    status: 200,
    body: {
      proposals: [
        proposal({
          state: "approved",
          skill: { name: "redirect-checks-v2" },
          rev: 2,
        }),
      ],
    },
  };
  render(<SkillsPage />);
  const group = (await screen.findByRole("heading", { name: /^Added from lessons/ })).closest(".skills-group") as HTMLElement;
  expect(within(group).getByText("redirect-checks-v2")).toBeInTheDocument();
  expect(within(group).queryByText("redirect-checks")).toBeNull();
  // The one line still comes from the SKILL.md text the proposal carries.
  expect(
    within(group).getByText("Check where a redirect or returnTo link goes before changing code that follows it."),
  ).toBeInTheDocument();
});

it("lists no added skill when the proposal carries none", async () => {
  answers.proposals = { status: 200, body: { proposals: [proposal({ state: "approved", skill: null, rev: 2 })] } };
  render(<SkillsPage />);
  expect(await screen.findByRole("heading", { name: /^Built in/ })).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: /^Added from lessons/ })).toBeNull();
  expect(screen.getByText(/Nothing added from lessons yet\./)).toBeInTheDocument();
});

it("draws no added-skill line while a proposal waits", async () => {
  answers.proposals = { status: 200, body: { proposals: [proposal()] } };
  render(<SkillsPage />);
  expect(await screen.findByRole("heading", { name: /^Proposed/ })).toBeInTheDocument();
  expect(screen.queryByText(/Nothing added from lessons yet\./)).toBeNull();
});
