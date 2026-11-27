import { expect, test, type Page, type Response } from "@playwright/test";
import type { Lesson } from "../../src/generated/Lesson";
import type { LessonDeleted } from "../../src/generated/LessonDeleted";
import type { LessonDetail } from "../../src/generated/LessonDetail";
import type { LessonList } from "../../src/generated/LessonList";
import type { LessonSettingsView } from "../../src/generated/LessonSettingsView";
import { startProject, type Service } from "./service";

/**
 * Crew lessons against the real service (KC5), which the mock specs beside it cannot reach. The Preview flag is the
 * service's own startup choice (`ORBYN_CREW_PREVIEW=1`): with it off every lessons route answers 404 and every screen
 * draws nothing, so a run whose service reports it off fails loudly in `beforeAll` rather than skipping.
 *
 * Covered, one real response status per UI write:
 * - Empty list: the real `GET /api/crew/lessons?repo=...` answers 200, and the page says "Nothing learned yet".
 * - Add by hand: `POST /api/crew/lessons` (201) through Add a lesson; the card says "Saved.", shows the text, and
 *   shows the service's "Added by you" source. The list row draws `SOURCE_WORD`, which says "Your correction" for a
 *   person source (not the card's `sourceWordOf`), and counts "1 of 50".
 * - Edit: `PATCH /api/crew/lessons/{id}` (200); the new text and the "Updated" row show.
 * - Off/on: `POST .../disable` and `POST .../enable` (200 each); the Off sentence, then the on/next-brief sentence
 *   the service's own `in_next_brief` earns.
 * - Promote and move back: `POST .../promote` and `POST .../demote` (200 each) through their confirms; the card's
 *   Applies to row says "All repositories", then "This repository" again.
 * - Delete and Undo: `DELETE /api/crew/lessons/{id}?rev=N` (200, with the service's `undo_until`), the list's
 *   "Lesson deleted" toast, then `POST .../undo` (200) and the lesson is back in the list.
 * - The switch: `PUT /api/crew/lessons/settings` (200) off and on again; the label reads "Off" then "On".
 * - The path refusal, in a second test: the text names a path, so the service's read says `names_path`, the card
 *   refuses from that flag without sending, and the real service refuses a direct promote with 400 and its sentence.
 *
 * What is not covered, honestly:
 * - No engine runs. Nothing here starts a crew task, so every lesson is a person's: there are no fingerprint, escape
 *   or revert lessons, and no brief actually carries one. The brief's cap and chooser are seen only through the
 *   cards' own flags and the list's counts.
 * - The 10 s Undo window is not waited out: sleeps are banned here, so only Undo inside the window is driven. A second
 *   delete followed by its expiry is not tested.
 * - The card's Delete has no confirm step in `LessonCard`: its button sends the DELETE itself. The Undo toast is the
 *   only confirmation, so no confirm dialog is clicked for a delete.
 * - Skill proposals, the Hidden filter and the over-cap path need data this service start never creates; their
 *   component tests own them.
 */

/** The desktop window less its native title bar, as the other real specs use it. */
const WINDOW = { width: 1280, height: 792 };
/** A fresh dev server compiles the page on its first load. */
const PAGE_READY = 30_000;
const READ_TIMEOUT = 30_000;
/** The Undo toast must be clicked inside the service's 10 s window; this only guards its arrival. */
const TOAST_READY = 5_000;

const LESSON_TEXT = "Run the web tests before you open a pull request.";
const EDITED_TEXT = "Run the web tests and the lint before you open a pull request.";
const PATH_TEXT = "Check src/auth/login.ts first.";

test.describe.configure({ mode: "serial" });

let service: Service;
let projectId: string;
let projectName: string;

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ crewPreview: true }));
  const info = await service.api<{ crew_preview?: boolean }>("GET", "/api/info");
  // A lost crewPreview flag must fail the run, not skip every lessons route.
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

const lessonPath = (id: string) => `/api/crew/lessons/${encodeURIComponent(id)}`;
const listUrl = () => `${service.page}#/settings/crew/lessons?repo=${encodeURIComponent(projectId)}`;
const addUrl = () => `${service.page}#/settings/crew/lessons/add?repo=${encodeURIComponent(projectId)}`;

test("a person's lesson is added, listed, edited, turned off and on, promoted, moved back, deleted and undone, and the switch flips", async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize(WINDOW);

  // 1. The repository starts with no lessons: the real read answers 200 and the page draws its empty state.
  const empty = read(page, "/api/crew/lessons");
  await page.goto(listUrl());
  const emptyResponse = await answered(empty, "GET /api/crew/lessons");
  const emptyList = (await emptyResponse.json()) as LessonList;
  expect(emptyList.repo).toBe(projectId);
  expect(emptyList.use_lessons).toBe(true);
  expect(emptyList.lessons).toEqual([]);
  expect(emptyList.counts).toEqual({ active: 0, disabled: 0, hidden: 0 });
  await expect(page.getByText("Nothing learned yet", { exact: true })).toBeVisible({ timeout: PAGE_READY });

  // 2. Add by hand. The add page reads the list again before it draws the form; Save is the real POST.
  const addRead = read(page, "/api/crew/lessons");
  await page.getByRole("button", { name: "Add a lesson", exact: true }).click();
  await answered(addRead, "GET /api/crew/lessons on the add page");
  const field = page.getByRole("textbox", { name: "Lesson text" });
  await expect(field).toBeVisible({ timeout: PAGE_READY });
  await field.fill(LESSON_TEXT);
  const adding = write(page, "POST", "/api/crew/lessons");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const addResponse = await answered(adding, "POST /api/crew/lessons", [200, 201]);
  const created = (await addResponse.json()) as Lesson;
  expect(created.text).toBe(LESSON_TEXT);
  expect(created.scope).toBe("repo");
  expect(created.repo).toBe(projectId);
  expect(created.source).toBe("person");
  expect(created.names_path).toBe(false);
  // The card the Add page opens says "Saved." once, with the lesson and the service's own hand-added source.
  await expect(page.getByRole("heading", { name: /^Crew › Lesson/ })).toBeVisible({ timeout: PAGE_READY });
  await expect(page.getByText("Saved.", { exact: true })).toBeVisible();
  await expect(page.getByText(LESSON_TEXT, { exact: true })).toBeVisible();
  await expect(page.getByText("Added by you", { exact: true })).toBeVisible();

  // 3. Back on the list: the row is in this repository's group, at 1 of the contract's 50. The list draws
  // SOURCE_WORD ("Your correction" for a person), not the card's sourceWordOf ("Added by you").
  const listed = read(page, "/api/crew/lessons");
  await page.getByRole("link", { name: "Back to Lessons", exact: true }).click();
  const listResponse = await answered(listed, "GET /api/crew/lessons after Add");
  const list = (await listResponse.json()) as LessonList;
  expect(list.lessons.map((lesson) => lesson.id)).toEqual([created.id]);
  await expect(page.getByRole("heading", { name: /^This repository · / })).toBeVisible({ timeout: PAGE_READY });
  await expect(page.getByText("This repository", { exact: true })).toBeVisible();
  await expect(page.getByText("1 of 50", { exact: true })).toBeVisible();
  await expect(page.getByText(LESSON_TEXT, { exact: true })).toBeVisible();
  await expect(page.getByText("Your correction", { exact: true })).toBeVisible();

  // 4. Edit the text: the card's PATCH answers 200, and the card draws the new text and its Updated row.
  const detailRead = read(page, lessonPath(created.id));
  // The card reads its own list once its detail is ready; settling it here keeps it from being mistaken for a later
  // delete or Undo read.
  const cardListRead = read(page, "/api/crew/lessons");
  await page.getByRole("button", { name: "Open", exact: true }).click();
  const detailResponse = await answered(detailRead, `GET ${lessonPath(created.id)}`);
  const detail = (await detailResponse.json()) as LessonDetail;
  expect(detail.lesson.id).toBe(created.id);
  const beforeEdit = detail.lesson;
  await expect(page.getByRole("heading", { name: /^Crew › Lesson/ })).toBeVisible({ timeout: PAGE_READY });
  await answered(cardListRead, "GET /api/crew/lessons on the card");
  await page.getByRole("button", { name: "Edit", exact: true }).click();
  const editField = page.getByRole("textbox", { name: "Lesson text" });
  await expect(editField).toHaveValue(beforeEdit.text);
  await editField.fill(EDITED_TEXT);
  const editing = write(page, "PATCH", lessonPath(created.id));
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const editResponse = await answered(editing, `PATCH ${lessonPath(created.id)}`);
  const edited = (await editResponse.json()) as Lesson;
  expect(edited.text).toBe(EDITED_TEXT);
  expect(edited.rev).toBe(beforeEdit.rev + 1);
  await expect(page.getByText(EDITED_TEXT, { exact: true })).toBeVisible();
  await expect(page.getByText("Updated", { exact: true })).toBeVisible();

  // 5. Off, then on. The service decides the on sentence from `in_next_brief`: one active repository lesson with the
  // switch on is inside the brief's cap, so it should say "On: goes into briefs". If a real service start's own rule
  // keeps it out, the card draws the exact not-in-the-next-brief sentence for this state instead, and that is asserted.
  const disabling = write(page, "POST", `${lessonPath(created.id)}/disable`);
  await page.getByRole("button", { name: "Turn off", exact: true }).click();
  const disableResponse = await answered(disabling, `POST ${lessonPath(created.id)}/disable`);
  const disabled = (await disableResponse.json()) as Lesson;
  expect(disabled.state).toBe("disabled");
  await expect(page.getByText("Off: stays here and does not go into a brief", { exact: true })).toBeVisible();

  const enabling = write(page, "POST", `${lessonPath(created.id)}/enable`);
  await page.getByRole("button", { name: "Turn on", exact: true }).click();
  const enableResponse = await answered(enabling, `POST ${lessonPath(created.id)}/enable`);
  const enabled = (await enableResponse.json()) as Lesson;
  expect(enabled.state).toBe("active");
  const onSentence = enabled.in_next_brief
    ? "On: goes into briefs"
    : enabled.hidden_by_cap
      ? "On, but over the cap: readable under Hidden, never in a brief"
      : "On, not in the next brief: this repository is over the 5-lesson limit for a brief.";
  await expect(page.getByText(onSentence, { exact: true })).toBeVisible();

  // 6. Promote through its confirm, then move back through its own. The lesson text names no path.
  await page.getByRole("button", { name: "Promote to all repositories", exact: true }).click();
  const promoteDialog = page.getByRole("dialog", { name: "Promote to all repositories?" });
  await expect(promoteDialog).toBeVisible();
  const promoting = write(page, "POST", `${lessonPath(created.id)}/promote`);
  await promoteDialog.getByRole("button", { name: "Promote lesson", exact: true }).click();
  const promoteResponse = await answered(promoting, `POST ${lessonPath(created.id)}/promote`);
  const promoted = (await promoteResponse.json()) as Lesson;
  expect(promoted.scope).toBe("global");
  expect(promoted.repo).toBeNull();
  await expect(promoteDialog).toHaveCount(0);
  await expect(page.getByText("All repositories", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Move back to one repository", exact: true }).click();
  const demoteDialog = page.getByRole("dialog", { name: "Move back to one repository?" });
  await expect(demoteDialog).toBeVisible();
  const demoting = write(page, "POST", `${lessonPath(created.id)}/demote`);
  await demoteDialog.getByRole("button", { name: "Move lesson back", exact: true }).click();
  const demoteResponse = await answered(demoting, `POST ${lessonPath(created.id)}/demote`);
  const demoted = (await demoteResponse.json()) as Lesson;
  expect(demoted.scope).toBe("repo");
  expect(demoted.repo).toBe(projectId);
  await expect(demoteDialog).toHaveCount(0);
  await expect(page.getByText("This repository", { exact: true })).toBeVisible();

  // 7. Delete. The card's Delete sends the DELETE itself (no confirm step); the list's Undo toast is the only
  // confirmation. The service's answer carries the instant the 10 s window shuts; this waits for neither it nor the
  // window to pass. The list's own read after Delete is armed before the click so the settle is not raced.
  const removedList = read(page, "/api/crew/lessons");
  const removing = page.waitForResponse(
    (response) =>
      response.request().method() === "DELETE" && new URL(response.url()).pathname === lessonPath(created.id),
    { timeout: READ_TIMEOUT },
  );
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  const deleteResponse = await answered(removing, `DELETE ${lessonPath(created.id)}`);
  const deleted = (await deleteResponse.json()) as LessonDeleted;
  expect(deleted.id).toBe(created.id);
  const undoUntil = Date.parse(deleted.undo_until);
  expect(Number.isNaN(undoUntil), `undo_until is a time: ${deleted.undo_until}`).toBe(false);
  expect(undoUntil).toBeGreaterThan(Date.now());
  expect(undoUntil).toBeLessThanOrEqual(Date.now() + 11_000);
  const removed = (await (await answered(removedList, "GET /api/crew/lessons after Delete")).json()) as LessonList;
  expect(removed.lessons).toEqual([]);
  await expect(page.getByText("Nothing learned yet", { exact: true })).toBeVisible();
  const toast = page.getByRole("status", { name: /^Lesson deleted\. Undo for \d+ seconds$/ });
  await expect(toast).toBeVisible({ timeout: TOAST_READY });
  await expect(toast.getByRole("button", { name: "Undo", exact: true })).toBeVisible();

  // Undo in the open window: the POST and the list read it triggers, both answered by the real service.
  const undoing = write(page, "POST", `${lessonPath(created.id)}/undo`);
  const undoneRead = read(page, "/api/crew/lessons");
  await toast.getByRole("button", { name: "Undo", exact: true }).click();
  const undoResponse = await answered(undoing, `POST ${lessonPath(created.id)}/undo`);
  const undone = (await undoResponse.json()) as Lesson;
  expect(undone.id).toBe(created.id);
  expect(undone.state).toBe("active");
  const undoneList = (await (await answered(undoneRead, "GET /api/crew/lessons after Undo")).json()) as LessonList;
  expect(undoneList.lessons.map((lesson) => lesson.id)).toContain(created.id);
  await expect(page.getByText(EDITED_TEXT, { exact: true })).toBeVisible();

  // 8. The switch: off, then on. Each click sends the real PUT; the label follows the service's answer.
  const control = page.getByRole("switch", { name: `Use lessons in ${projectName}`, exact: true });
  await expect(control).toBeVisible();
  await expect(control).toBeChecked();

  const turningOff = write(page, "PUT", "/api/crew/lessons/settings");
  await control.click();
  const offResponse = await answered(turningOff, "PUT /api/crew/lessons/settings off");
  const off = (await offResponse.json()) as LessonSettingsView;
  expect(off).toEqual({ repo: projectId, use_lessons: false });
  await expect(control).not.toBeChecked();
  await expect(page.getByText("Off", { exact: true })).toBeVisible();

  const turningOn = write(page, "PUT", "/api/crew/lessons/settings");
  await control.click();
  const onResponse = await answered(turningOn, "PUT /api/crew/lessons/settings on");
  const on = (await onResponse.json()) as LessonSettingsView;
  expect(on).toEqual({ repo: projectId, use_lessons: true });
  await expect(control).toBeChecked();
  await expect(page.getByText("On", { exact: true })).toBeVisible();
});

test("refuses to promote a lesson that names a path", async ({ page }) => {
  test.setTimeout(90_000);
  await page.setViewportSize(WINDOW);

  const addRead = read(page, "/api/crew/lessons");
  await page.goto(addUrl());
  await answered(addRead, "GET /api/crew/lessons on the add page");
  const field = page.getByRole("textbox", { name: "Lesson text" });
  await expect(field).toBeVisible({ timeout: PAGE_READY });
  await field.fill(PATH_TEXT);
  const adding = write(page, "POST", "/api/crew/lessons");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const addResponse = await answered(adding, "POST /api/crew/lessons (path)", [200, 201]);
  const created = (await addResponse.json()) as Lesson;
  expect(created.text).toBe(PATH_TEXT);
  // The service's own path rule, not the page's: this is the flag the card reads before it decides to send.
  expect(created.names_path, "the service flags the text as naming a path").toBe(true);
  await expect(page.getByText("Saved.", { exact: true })).toBeVisible({ timeout: PAGE_READY });
  await expect(page.getByText(PATH_TEXT, { exact: true })).toBeVisible();

  const detail = await service.api<LessonDetail>(
    "GET",
    `${lessonPath(created.id)}?repo=${encodeURIComponent(projectId)}`,
  );
  expect(detail.lesson.names_path).toBe(true);
  expect(detail.lesson.scope).toBe("repo");

  // The card refuses from `names_path` before it sends, so this click causes no page response to wait for. The real
  // service's identical refusal is asserted directly, with the same 400 status and the contract's own sentence.
  let refusal = "";
  try {
    await service.api("POST", `${lessonPath(created.id)}/promote`, { rev: detail.lesson.rev });
  } catch (error) {
    refusal = String(error);
  }
  expect(refusal, "the real service refused the promotion").not.toBe("");
  expect(refusal).toContain("400");
  expect(refusal).toContain(
    "Edit this lesson first: it names a path, and a global lesson names path classes only.",
  );

  let promoted = false;
  page.on("request", (request) => {
    if (request.method() === "POST" && new URL(request.url()).pathname === `${lessonPath(created.id)}/promote`) {
      promoted = true;
    }
  });
  await page.getByRole("button", { name: "Promote to all repositories", exact: true }).click();
  await expect(page.getByText("Can’t promote this lesson", { exact: true })).toBeVisible();
  await expect(
    page.getByText("It names a path, so it can’t apply to all repositories. Edit the text first.", { exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("dialog", { name: "Promote to all repositories?" })).toHaveCount(0);
  expect(promoted, "the card refuses from the read's names_path without sending a promote").toBe(false);
});
