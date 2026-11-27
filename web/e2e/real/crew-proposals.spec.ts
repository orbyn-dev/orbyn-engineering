import { expect, test, type Page, type Response } from "@playwright/test";
import type { ApiError } from "../../src/generated/ApiError";
import type { Lesson } from "../../src/generated/Lesson";
import type { LessonDetail } from "../../src/generated/LessonDetail";
import type { LessonList } from "../../src/generated/LessonList";
import type { SkillProposal } from "../../src/generated/SkillProposal";
import type { SkillProposalList } from "../../src/generated/SkillProposalList";
import { startProject, type Service } from "./service";

/**
 * Crew skill proposals against the real service (KC9), which the mock specs beside them cannot reach. The Preview flag
 * and the debug seed are the service start's own choices: `startProject({ crewPreview: true, seedProposals: true })`
 * turns on both, so a lost Preview flag fails loudly in `beforeAll` and a lost seed route fails at the first seed. One
 * serial service means every test sees what the one before left; Review and Approve drive the proposal test 2 seeds,
 * while Dismiss and Stale seed their own areas and so their own groups.
 *
 * Covered, one real response status per UI write:
 * - Empty: before any seed, the real `GET /api/crew/skill-proposals?repo=...` answers 200 with `{ proposals: [] }`, and
 *   the Lessons page draws no "Skill proposals" heading and no "No proposals" box.
 * - Seeding: the seed writes exactly `lessons` new rows per call, so a first call of 1 in one area and a second call of
 *   2 in another both answer `proposal_id: null` and the proposals read stays empty. A third lesson in the second area
 *   makes exactly one open proposal: its card says the name, "3 active lessons with the same source, check kind and
 *   path class", Review/Approve/Dismiss, and the heading counts 1. A reload keeps it, and the API list has one open
 *   proposal for the repository.
 * - Review: the card's Review opens the proposal's own page, which shows the proposal's text and its based-on lessons
 *   (the escape evidence ids); Back to Lessons returns to the card.
 * - Approve: Approve -> "Add to skills-local" -> the real POST answers 200; the page says "Added to skills-local." and
 *   links the skill at `#/skills/<name>`; the card and the "Skill proposals" heading go; the real lessons read marks
 *   every source lesson "Now a skill"; the Skills page lists the skill under "Added from lessons" as "Personal".
 * - Dismiss: a second area seeds three lessons; Dismiss -> "Dismiss" -> the real POST answers 200; the card and heading
 *   go, the toast says "Proposal dismissed.", the API list has no open proposal, and the source lessons still exist.
 * - Stale sources: a third area seeds three lessons; one source is turned off through the real lessons API behind the
 *   page's back. The page's read is stale, so its Approve is still enabled and no re-read happens before the confirm.
 *   The real POST answers 400 `invalid` with "A lesson this skill came from changed, so it cannot be approved." and the
 *   confirm stays open with that sentence; the page's own re-read then turns the card's Approve off. The contract's 409s
 *   are a stale revision, a proposal that is no longer open and a taken skill name; a changed source is the core's
 *   `Invalid`, which the proposals route maps to 400.
 *
 * What is not covered, honestly:
 * - No engine runs. Every proposal here comes from the debug seed, not from a crew task, so no real learner groups
 *   anything and no brief carries an approved skill. The fixed template's shape is asserted, not a model's writing.
 * - No "Don't propose again": the contract has no mute, so Dismiss only answers the proposal and the toast says so.
 * - No Check for updates: the section has no such control, and no read is polled for a newer proposal.
 * - No skill folder content checks: the spec asserts the service and the screens, never the bytes of the SKILL.md the
 *   approval writes under skills-local. Removing that folder and watching "Now a skill" disappear is left out too.
 */

/** The desktop window less its native title bar, as the other real specs use it. */
const WINDOW = { width: 1280, height: 792 };
/** A fresh dev server compiles the page on its first load. */
const PAGE_READY = 30_000;
const READ_TIMEOUT = 30_000;
/** The dismissal toast must be caught inside its 4 s life; this only guards its arrival. */
const TOAST_READY = 5_000;
/** The core's own sentence when a source lesson changed after the proposal was made. */
const CANNOT_APPROVE = "A lesson this skill came from changed, so it cannot be approved.";

/** The area whose one lesson is never a group, then the area test 2 completes and tests 3 and 4 answer. */
const FIRST_AREA = "src/one";
const REVIEW_AREA = "src/review";
const DISMISS_AREA = "src/dismiss";
const STALE_AREA = "src/stale";

test.describe.configure({ mode: "serial" });

let service: Service;
let projectId: string;
let projectName: string;
/** The proposal test 2 seeds; test 3 reviews it and test 4 approves it. */
let seeded: SkillProposal;

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ crewPreview: true, seedProposals: true }));
  const info = await service.api<{ crew_preview?: boolean }>("GET", "/api/info");
  // A lost crewPreview flag must fail the run, not skip every proposal route.
  expect(info.crew_preview, "the service reports Crew Preview on").toBe(true);
  const projects = await service.api<{ id: string; name: string }[]>("GET", "/api/projects");
  const project = projects.find((entry) => entry.id === projectId);
  expect(project, "the added project is in the real project list").toBeDefined();
  projectName = project!.name;
});

test.afterAll(async () => {
  await service?.stop();
});

/** The one real read for a pathname, started before the navigation that causes it. */
function read(page: Page, pathname: string) {
  return page.waitForResponse(
    (response) => response.request().method() === "GET" && new URL(response.url()).pathname === pathname,
    { timeout: READ_TIMEOUT },
  );
}

/** The real write for a method and pathname, started before the click that causes it. */
function write(page: Page, method: string, pathname: string) {
  return page.waitForResponse(
    (response) => response.request().method() === method && new URL(response.url()).pathname === pathname,
    { timeout: READ_TIMEOUT },
  );
}

/** The answer once it arrives; any status outside the expected set is the failure. */
async function answered(
  pending: Promise<Response>,
  label: string,
  expected: number | readonly number[] = 200,
): Promise<Response> {
  const response = await pending;
  const statuses = typeof expected === "number" ? [expected] : expected;
  expect(statuses, `${label} answered ${response.status()}`).toContain(response.status());
  return response;
}

const slug = (value: string) => value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
/** The template's name for a dev-seeded escape group: `crew-<source>-<area slug>`. */
const skillNameFor = (area: string) => `crew-escape-${slug(area)}`;

const lessonsUrl = () => `${service.page}#/settings/crew/lessons?repo=${encodeURIComponent(projectId)}`;
const skillsUrl = () => `${service.page}#/skills`;
const proposalsUrl = () => `/api/crew/skill-proposals?repo=${encodeURIComponent(projectId)}`;
const proposalPath = (id: string) => `/api/crew/skill-proposals/${encodeURIComponent(id)}`;
const lessonPath = (id: string) => `/api/crew/lessons/${encodeURIComponent(id)}`;

interface Seeded {
  lessons: string[];
  proposal_id: string | null;
}

/** The debug seam: writes `lessons` new escape lessons of `area` and answers the group's open proposal, if any. */
async function seedProposal(area: string, lessons = 3): Promise<Seeded> {
  const seeded = await service.api<Seeded>("POST", "/api/crew/dev/seed-proposal", {
    repo: projectId,
    lessons,
    area,
  });
  expect(seeded.lessons, `the seed wrote ${lessons} lessons`).toHaveLength(lessons);
  return seeded;
}

async function readProposals(): Promise<SkillProposalList> {
  return service.api<SkillProposalList>("GET", proposalsUrl());
}

/** Runs the navigation and returns the proposals body of the read the Lessons page makes once it lands. */
async function readLessonsPage(page: Page, change: () => Promise<unknown>): Promise<SkillProposalList> {
  const proposals = read(page, "/api/crew/skill-proposals");
  const lessons = read(page, "/api/crew/lessons");
  await change();
  const [proposalResponse] = await Promise.all([
    answered(proposals, "GET /api/crew/skill-proposals"),
    answered(lessons, "GET /api/crew/lessons"),
  ]);
  return (await proposalResponse.json()) as SkillProposalList;
}

const openLessons = (page: Page) => readLessonsPage(page, () => page.goto(lessonsUrl()));
const reloadLessons = (page: Page) => readLessonsPage(page, () => page.reload());

function proposalCard(page: Page, name: string) {
  return page.getByRole("region", { name: `Proposed skill ${name}`, exact: true });
}

function proposalsHeading(page: Page) {
  return page.getByRole("heading", { name: /^Skill proposals/ });
}

test("no skill proposals are read or drawn before anything is seeded", async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize(WINDOW);

  const empty = await openLessons(page);
  expect(empty.proposals).toEqual([]);
  await expect(page.getByText("Nothing learned yet", { exact: true })).toBeVisible({ timeout: PAGE_READY });
  await expect(proposalsHeading(page)).toHaveCount(0);
  await expect(page.getByText("No proposals", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("region", { name: /Proposed skill/ })).toHaveCount(0);
});

test("a third lesson in one group makes the only proposal, and it survives a reload", async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize(WINDOW);

  // Each seed call writes its own new rows. One lesson in a first area and two in a second both stay under three;
  // a same-area 1 then 2 would already be three, so the group that becomes the proposal is grown one lesson at a time.
  const one = await seedProposal(FIRST_AREA, 1);
  expect(one.proposal_id).toBeNull();
  const two = await seedProposal(REVIEW_AREA, 2);
  expect(two.proposal_id).toBeNull();

  const empty = await openLessons(page);
  expect(empty.proposals).toEqual([]);
  await expect(page.getByText(`Dev seed lesson 1 for ${REVIEW_AREA}.`, { exact: true }).first()).toBeVisible({
    timeout: PAGE_READY,
  });
  await expect(proposalsHeading(page)).toHaveCount(0);
  await expect(page.getByText("No proposals", { exact: true })).toHaveCount(0);

  const third = await seedProposal(REVIEW_AREA, 1);
  expect(third.proposal_id, "the third lesson makes the group's proposal").not.toBeNull();
  const name = skillNameFor(REVIEW_AREA);

  const listed = await reloadLessons(page);
  const open = listed.proposals.filter((proposal) => proposal.state === "open");
  expect(open, "the API list has exactly one open proposal").toHaveLength(1);
  expect(open[0]!.id).toBe(third.proposal_id);
  expect(open[0]!.can_approve).toBe(true);
  expect(open[0]!.lessons).toHaveLength(3);
  const sourceIds = open[0]!.lessons.map((lesson) => lesson.id);
  // The first area's lesson is another group; only the second area's three lessons are this proposal's sources.
  for (const id of [...two.lessons, ...third.lessons]) expect(sourceIds).toContain(id);
  for (const id of one.lessons) expect(sourceIds).not.toContain(id);
  seeded = open[0]!;

  const card = proposalCard(page, name);
  await expect(card).toBeVisible({ timeout: PAGE_READY });
  await expect(card.getByText(name, { exact: true })).toBeVisible();
  await expect(card.getByText("3 active lessons with the same source, check kind and path class", { exact: true })).toBeVisible();
  await expect(card.getByRole("button", { name: "Review", exact: true })).toBeVisible();
  await expect(card.getByRole("button", { name: "Approve", exact: true })).toBeEnabled();
  await expect(card.getByRole("button", { name: "Dismiss", exact: true })).toBeVisible();
  await expect(proposalsHeading(page)).toContainText("1");

  // Reload: the service still answers the same one open proposal, and the page draws it again.
  const again = await reloadLessons(page);
  expect(again.proposals.filter((proposal) => proposal.state === "open").map((proposal) => proposal.id)).toEqual([seeded.id]);
  await expect(card).toBeVisible({ timeout: PAGE_READY });
  await expect(proposalsHeading(page)).toContainText("1");

  const api = await readProposals();
  expect(api.proposals.filter((proposal) => proposal.state === "open")).toHaveLength(1);
  expect(api.proposals.find((proposal) => proposal.id === seeded.id)?.state).toBe("open");
});

test("Review opens the proposal's own page and Back returns to the card", async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize(WINDOW);
  if (!seeded) throw new Error("the seeding test must run first");
  const name = skillNameFor(REVIEW_AREA);

  const listed = await openLessons(page);
  expect(listed.proposals.find((proposal) => proposal.id === seeded.id)?.state).toBe("open");
  const card = proposalCard(page, name);
  await expect(card).toBeVisible({ timeout: PAGE_READY });

  const reviewed = await readLessonsPage(page, () => card.getByRole("button", { name: "Review", exact: true }).click());
  expect(reviewed.proposals.find((proposal) => proposal.id === seeded.id)?.state).toBe("open");
  await expect(page.getByRole("heading", { name: /^Crew › Proposed skill/ })).toBeVisible({ timeout: PAGE_READY });
  await expect(page.getByRole("textbox", { name: "Skill text" })).toHaveValue(seeded.text, { timeout: PAGE_READY });

  // The proposal's facts: its name, its source lessons' evidence ids, and the count of lessons it is based on.
  const facts = page.locator(".cx-ppfkv").first();
  await expect(facts.getByText(name, { exact: true })).toBeVisible();
  await expect(facts.getByText("3 lessons", { exact: true })).toBeVisible();
  await expect(facts.getByRole("link", { name: "Escape dev-seed-1" })).toHaveCount(2);
  await expect(facts.getByRole("link", { name: "Escape dev-seed-2" })).toHaveCount(1);
  await expect(facts.getByRole("link", { name: "Escape dev-seed-1" }).first()).toHaveAttribute(
    "href",
    "#/settings/crew/escapes",
  );
  await expect(page.getByRole("link", { name: "Back to Lessons" })).toHaveAttribute(
    "href",
    `#/settings/crew/lessons?repo=${encodeURIComponent(projectId)}`,
  );

  const back = await readLessonsPage(page, () =>
    page.getByRole("link", { name: "Back to Lessons", exact: true }).click(),
  );
  expect(back.proposals.find((proposal) => proposal.id === seeded.id)?.state).toBe("open");
  await expect(card).toBeVisible({ timeout: PAGE_READY });
  await expect(proposalsHeading(page)).toContainText("1");
});

test("Approve writes the skill, clears the section, marks its lessons and lists it as Personal", async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize(WINDOW);
  if (!seeded) throw new Error("the seeding test must run first");
  const name = skillNameFor(REVIEW_AREA);

  const listed = await openLessons(page);
  expect(listed.proposals.find((proposal) => proposal.id === seeded.id)?.can_approve).toBe(true);
  const card = proposalCard(page, name);
  await expect(card).toBeVisible({ timeout: PAGE_READY });
  await expect(proposalsHeading(page)).toContainText("1");

  await card.getByRole("button", { name: "Approve", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: `Add ${name} to skills-local?`, exact: true });
  await expect(dialog).toBeVisible();

  const approving = write(page, "POST", `${proposalPath(seeded.id)}/approve`);
  const proposalsAfter = read(page, "/api/crew/skill-proposals");
  const lessonsAfter = read(page, "/api/crew/lessons");
  await dialog.getByRole("button", { name: "Add to skills-local", exact: true }).click();
  const approved = (await (await answered(approving, `POST ${proposalPath(seeded.id)}/approve`)).json()) as SkillProposal;
  expect(approved.state).toBe("approved");
  expect(approved.skill?.name).toBe(name);
  expect(approved.rev).toBe(seeded.rev + 1);

  const banner = page.getByRole("status").filter({ hasText: "Added to skills-local." });
  await expect(banner).toBeVisible({ timeout: PAGE_READY });
  await expect(banner.getByText("Added to skills-local.", { exact: true })).toBeVisible();
  await expect(banner.getByRole("link", { name, exact: true })).toHaveAttribute("href", `#/skills/${name}`);
  await expect(banner).toContainText("is listed on the Skills page as Personal.");

  // The section's own re-read and the page's lessons re-read, both answered by the real service.
  const after = (await (await answered(proposalsAfter, "GET /api/crew/skill-proposals after Approve")).json()) as SkillProposalList;
  const lessons = (await (await answered(lessonsAfter, "GET /api/crew/lessons after Approve")).json()) as LessonList;
  expect(after.proposals.filter((proposal) => proposal.state === "open"), "no proposal stays open").toHaveLength(0);
  expect(after.proposals.find((proposal) => proposal.id === seeded.id)?.state).toBe("approved");
  expect(after.proposals.find((proposal) => proposal.id === seeded.id)?.skill?.name).toBe(name);
  for (const { id } of seeded.lessons) {
    const lesson = lessons.lessons.find((one) => one.id === id);
    expect(lesson, `lesson ${id} is still listed`).toBeDefined();
    expect(lesson!.skill, `lesson ${id} is now the skill`).toBe(name);
  }

  await expect(card).toHaveCount(0);
  await expect(proposalsHeading(page)).toHaveCount(0);
  await expect(page.getByText("Now a skill", { exact: true })).toHaveCount(3);
  await expect(page.locator(".cx-lsrow").getByRole("link", { name, exact: true })).toHaveCount(3);

  // The Skills page lists it in its own group, from the real proposals read it makes for every visible project.
  const skillProposals = read(page, "/api/crew/skill-proposals");
  await page.goto(skillsUrl());
  const skillList = (await (await answered(skillProposals, "GET /api/crew/skill-proposals on the Skills page")).json()) as SkillProposalList;
  expect(skillList.proposals.find((proposal) => proposal.id === seeded.id)?.state).toBe("approved");

  const added = page.getByRole("heading", { name: /^Added from lessons/ });
  await expect(added).toBeVisible({ timeout: PAGE_READY });
  await expect(added).toContainText("1");
  const group = added.locator("..");
  await expect(group.getByText(name, { exact: true })).toBeVisible();
  await expect(group.getByText("Personal", { exact: true })).toBeVisible();
  await expect(group.getByText(/^From 3 lessons in /)).toBeVisible();
  await expect(group.getByRole("button", { name: projectName, exact: true })).toBeVisible();
});

test("Dismiss takes the proposal away, keeps its lessons and says so", async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize(WINDOW);

  const dismissedSeed = await seedProposal(DISMISS_AREA, 3);
  expect(dismissedSeed.proposal_id, "three lessons make a proposal").not.toBeNull();
  const name = skillNameFor(DISMISS_AREA);
  const before = await readProposals();
  const target = before.proposals.find((proposal) => proposal.id === dismissedSeed.proposal_id);
  expect(target?.state).toBe("open");
  expect(target?.can_approve).toBe(true);
  expect(target!.lessons).toHaveLength(3);

  const listed = await openLessons(page);
  expect(listed.proposals.filter((proposal) => proposal.state === "open")).toHaveLength(1);
  const card = proposalCard(page, name);
  await expect(card).toBeVisible({ timeout: PAGE_READY });
  await expect(proposalsHeading(page)).toContainText("1");

  await card.getByRole("button", { name: "Dismiss", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Dismiss this proposal?", exact: true });
  await expect(dialog).toBeVisible();
  await expect(
    dialog.getByText("Nothing was added to skills-local. The lessons it was based on stay as they are.", { exact: true }),
  ).toBeVisible();
  await expect(dialog.getByText(`3 lessons · This repository (${projectName})`, { exact: true })).toBeVisible();

  const dismissing = write(page, "POST", `${proposalPath(target!.id)}/dismiss`);
  const proposalsAfter = read(page, "/api/crew/skill-proposals");
  await dialog.getByRole("button", { name: "Dismiss", exact: true }).click();
  const dismissed = (await (await answered(dismissing, `POST ${proposalPath(target!.id)}/dismiss`)).json()) as SkillProposal;
  expect(dismissed.state).toBe("dismissed");
  expect(dismissed.rev).toBe(target!.rev + 1);
  expect(dismissed.skill).toBeNull();
  await expect(page.locator(".toast", { hasText: "Proposal dismissed." })).toBeVisible({ timeout: TOAST_READY });

  const after = (await (await answered(proposalsAfter, "GET /api/crew/skill-proposals after Dismiss")).json()) as SkillProposalList;
  expect(after.proposals.find((proposal) => proposal.id === target!.id)?.state).toBe("dismissed");
  expect(after.proposals.filter((proposal) => proposal.state === "open"), "no open proposal is left").toHaveLength(0);
  await expect(card).toHaveCount(0);
  await expect(proposalsHeading(page)).toHaveCount(0);

  // Dismiss answers the proposal; the lessons it came from stay exactly where they were.
  const lessons = await service.api<LessonList>("GET", `/api/crew/lessons?repo=${encodeURIComponent(projectId)}&filter=all`);
  for (const id of dismissedSeed.lessons) {
    const lesson = lessons.lessons.find((one) => one.id === id);
    expect(lesson, `lesson ${id} still exists`).toBeDefined();
    expect(lesson!.state).toBe("active");
  }
});

test("a stale proposal keeps the service's sentence in the open confirm and turns Approve off", async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize(WINDOW);

  const staleSeed = await seedProposal(STALE_AREA, 3);
  expect(staleSeed.proposal_id, "three lessons make a proposal").not.toBeNull();
  const name = skillNameFor(STALE_AREA);
  const before = await readProposals();
  const target = before.proposals.find((proposal) => proposal.id === staleSeed.proposal_id);
  expect(target?.can_approve).toBe(true);
  expect(target!.lessons).toHaveLength(3);

  const listed = await openLessons(page);
  expect(listed.proposals.filter((proposal) => proposal.state === "open")).toHaveLength(1);
  const card = proposalCard(page, name);
  await expect(card).toBeVisible({ timeout: PAGE_READY });
  await expect(card.getByRole("button", { name: "Approve", exact: true })).toBeEnabled();
  await expect(proposalsHeading(page)).toContainText("1");

  // Change the world behind the page's back: one source lesson is turned off, so the proposal's sources are two.
  const source = target!.lessons[0]!.id;
  const detail = await service.api<LessonDetail>(
    "GET",
    `${lessonPath(source)}?repo=${encodeURIComponent(projectId)}`,
  );
  const disabled = await service.api<Lesson>("POST", `${lessonPath(source)}/disable`, { rev: detail.lesson.rev });
  expect(disabled.state).toBe("disabled");
  expect(disabled.in_next_brief).toBe(false);
  expect(disabled.rev).toBe(detail.lesson.rev + 1);
  const now = await readProposals();
  const staleNow = now.proposals.find((proposal) => proposal.id === target!.id);
  expect(staleNow?.state).toBe("open");
  expect(staleNow?.can_approve).toBe(false);

  // The page has not re-read, so its Approve is still enabled and the confirm opens; the real write is refused.
  await card.getByRole("button", { name: "Approve", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: `Add ${name} to skills-local?`, exact: true });
  await expect(dialog).toBeVisible();

  const approving = write(page, "POST", `${proposalPath(target!.id)}/approve`);
  const proposalsAfter = read(page, "/api/crew/skill-proposals");
  await dialog.getByRole("button", { name: "Add to skills-local", exact: true }).click();
  const refused = await answered(approving, `POST ${proposalPath(target!.id)}/approve on a stale source`, 400);
  const failure = (await refused.json()) as ApiError;
  expect(failure.code).toBe("invalid");
  expect(failure.message).toBe(CANNOT_APPROVE);
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("alert")).toHaveText(CANNOT_APPROVE);

  // The refused write makes the section read again: the proposal is still open, but Approve is now off.
  const reread = (await (await answered(proposalsAfter, "GET /api/crew/skill-proposals after the refusal")).json()) as SkillProposalList;
  const after = reread.proposals.find((proposal) => proposal.id === target!.id);
  expect(after?.state).toBe("open");
  expect(after?.can_approve).toBe(false);
  expect(after?.skill ?? null).toBeNull();

  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(card.getByRole("button", { name: "Approve", exact: true })).toBeDisabled();
  // Only the proposals read ran again. The page's lessons list still holds the lesson it loaded before the disable,
  // so the note counts all three as active; what matters here is the service's `can_approve: false` turning Approve off.
  await expect(card.locator("b.cx-pplead")).toHaveText("Based on 3 active lessons now, 3 when it was proposed.");
  await expect(card).toContainText("A lesson it was based on was deleted or turned off");
  await expect(card).toContainText("Approve is off.");
  await expect(proposalsHeading(page)).toContainText("1");
});
