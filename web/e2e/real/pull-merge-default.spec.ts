import { expect, test, type Page } from "@playwright/test";
import { startProject, type Service } from "./service";

/** The merge dialog's starting method through the real service: the global default and a project override. */

let service: Service;
let projectId = "";

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({
    fakeEngines: true,
    forge: [{ number: 1, title: "Merge default", headRefName: "feat", reviewDecision: "APPROVED", statusCheckRollup: [{ conclusion: "SUCCESS", status: "COMPLETED" }] }],
  }));
});
test.afterAll(() => service.stop());
test.beforeEach(({ page }) => page.setViewportSize({ width: 1500, height: 1000 }));

/** The setting as the service resolves it for this project, with the source that won. */
const setting = () =>
  service
    .api<{ key: string; value: unknown; source: string }[]>("GET", `/api/preferences?project_id=${projectId}`)
    .then((rows) => rows.find((row) => row.key === "pull_request_merge_method"));
const setGlobal = (value: unknown) =>
  service.api("PUT", "/api/preferences", { values: { pull_request_merge_method: value } });
const setProject = (value: unknown) =>
  service.api("PUT", `/api/projects/${projectId}/preferences`, { values: { pull_request_merge_method: value } });
/** The pull request page, read fresh so the setting is read again. */
const openPull = async (page: Page) => {
  await page.goto(`${service.page}#/pulls`);
  await page.getByRole("button", { name: /Merge default/ }).click();
  const article = page.getByRole("article", { name: "Merge default" });
  await expect(article).toBeVisible();
  return article;
};

test("the merge dialog starts on the project's merge method, and a project override wins over the global default", async ({ page }) => {
  await setGlobal("merge");
  let article = await openPull(page);
  await expect(article.getByRole("button", { name: /^Create a merge commit$/ })).toBeVisible();
  expect(await setting()).toMatchObject({ value: "merge", source: "global" });

  // The project's own override is what the settings page writes when a project is selected.
  await setProject("rebase");
  article = await openPull(page);
  await expect(article.getByRole("button", { name: /^Rebase and merge$/ })).toBeVisible();
  expect(await setting()).toMatchObject({ value: "rebase", source: "project" });
  // On the page the merge menu chose the method, so the dialog says which and shows no select.
  await article.getByRole("button", { name: "Merge", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Merge pull request" });
  await expect(dialog).toContainText("Rebase and merge #1 into main.");
  await expect(dialog.getByRole("combobox", { name: "Merge method" })).toHaveCount(0);
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(dialog).toBeHidden();

  // Clearing the override inherits the global default again.
  await setProject(null);
  article = await openPull(page);
  await expect(article.getByRole("button", { name: /^Create a merge commit$/ })).toBeVisible();
  expect(await setting()).toMatchObject({ value: "merge", source: "global" });

  // Ask is the setting's default; the dialog falls back to Squash and merge.
  await setGlobal("");
  article = await openPull(page);
  await expect(article.getByRole("button", { name: /^Squash and merge$/ })).toBeVisible();
  expect(await setting()).toMatchObject({ value: "", source: "global" });
});
