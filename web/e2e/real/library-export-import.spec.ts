import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { openPanelTab, startProject, type Service } from "./service";

/**
 * The Library panel's Export and Import against a real `orbynd`: Export saves the service's own
 * document as a downloaded JSON file, and Import reads that file back through a real file chooser,
 * shows the service's dry-run report, writes nothing until Import is chosen, then reloads Personal.
 * The unit tests answer with the mock; this run reads the real export and import routes.
 */

const EXPORT_NOTICE = "Exported the library to a JSON file. It holds every kind; an env set goes by key name only, never a value.";
const DRY_RUN_NOTE = "Checked by the service. Nothing is written until you choose Import.";
const NO_EXPORT = "This service has no library export. Update Orbyn, then try again.";
const NO_IMPORT = "This version of the Orbyn service has no library import.";
const SEEDED = {
  workflow: { name: "Start dev server", description: "Runs the dev server", command: "npm run dev", params: [], env_set_ids: [] },
  prompt: { name: "Explain a diff", description: "Walks through a patch", body: "Explain this diff one file at a time.", params: [] },
  envSet: { name: "work-laptop", description: "Laptop variables", keys: ["API_BASE", "API_TOKEN"] },
};

interface ExportedWorkflow { name: string; description: string; command: string; params: unknown[]; env_set_names: string[] }
interface ExportedPrompt { name: string; description: string; body: string; params: unknown[] }
interface ExportedEnvSet { name: string; description: string; keys: string[] }
interface LibraryDocument {
  format: string;
  version: number;
  exported_at: string;
  profiles: unknown[];
  workflows: ExportedWorkflow[];
  prompts: ExportedPrompt[];
  env_sets: ExportedEnvSet[];
}

/** A service with one thread to show the panel in, and one item of each kind to export. */
let service: Service;
let threadId = "";

const counts = async () => ({
  workflows: (await service.api<unknown[]>("GET", "/api/library/workflows")).length,
  prompts: (await service.api<unknown[]>("GET", "/api/library/prompts")).length,
  envSets: (await service.api<unknown[]>("GET", "/api/library/env-sets")).length,
});

/** The thread page, its side panel and the Library tab: the panel offers the tab once the summary answered. */
async function openLibrary(page: Page) {
  const probed = page.waitForResponse((response) => response.request().method() === "GET" && new URL(response.url()).pathname === "/api/library");
  await page.goto(`${service.page}#/threads/${threadId}`);
  const panel = await openPanelTab(page);
  await probed;
  const tab = panel.getByRole("tab", { name: "Library", exact: true });
  if ((await tab.count()) === 0) {
    await panel.getByRole("button", { name: "Open a tab", exact: true }).click();
    await page.getByRole("menuitem", { name: "Library" }).click();
  }
  await expect(tab).toBeVisible();
  if ((await tab.getAttribute("aria-selected")) !== "true") await tab.click();
  await expect(tab).toHaveAttribute("aria-selected", "true");
  // The whole panel region: the tabs, the toolbar's actions, the notice under them and the rows.
  const library = panel.getByRole("region", { name: "Library" });
  await expect(library).toBeVisible({ timeout: 15_000 });
  await expect(library.getByRole("tablist", { name: "Library" })).toBeVisible();
  return library;
}

test.beforeAll(async () => {
  const started = await startProject({ fakeEngines: true });
  service = started.service;
  await service.api("POST", "/api/library/workflows", SEEDED.workflow);
  await service.api("POST", "/api/library/prompts", SEEDED.prompt);
  await service.api("POST", "/api/library/env-sets", SEEDED.envSet);
  const thread = await service.api<{ id: string }>("POST", "/api/threads", { project_id: started.projectId, engine: "codex", title: "Library export and import" });
  threadId = thread.id;
});

test.afterAll(() => service.stop());

test.beforeEach(({ page }) => page.setViewportSize({ width: 1500, height: 900 }));

test("Export saves the service's document and Import adds the same file back to Personal", async ({ page }) => {
  const library = await openLibrary(page);

  // Export: the service builds the document, the browser saves it, and nothing is written.
  const exported = page.waitForResponse((response) => response.request().method() === "GET" && new URL(response.url()).pathname === "/api/library/export");
  const downloading = page.waitForEvent("download");
  await library.getByRole("button", { name: "Export", exact: true }).click();
  expect((await exported).ok()).toBe(true);
  const download = await downloading;
  expect(download.suggestedFilename()).toMatch(/^orbyn-library-\d{4}-\d{2}-\d{2}\.json$/);
  const savedPath = await download.path();
  expect(savedPath).not.toBeNull();
  const text = readFileSync(savedPath!, "utf8");
  const document = JSON.parse(text) as LibraryDocument;

  // The downloaded file is the service's own document: every kind, sets by name and key names only.
  expect(document.format).toBe("orbyn-library");
  expect(document.version).toBe(1);
  expect(document.exported_at).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  expect(document.profiles).toEqual([]);
  expect(document.workflows).toEqual([{ name: "Start dev server", description: "Runs the dev server", command: "npm run dev", params: [], env_set_names: [] }]);
  expect(document.prompts).toEqual([{ name: "Explain a diff", description: "Walks through a patch", body: "Explain this diff one file at a time.", params: [] }]);
  expect(document.env_sets).toEqual([{ name: "work-laptop", description: "Laptop variables", keys: ["API_BASE", "API_TOKEN"] }]);
  expect(text).not.toContain('"values"');
  await expect(library.getByText(EXPORT_NOTICE)).toBeVisible();
  expect(await counts()).toEqual({ workflows: 1, prompts: 1, envSets: 1 });

  // Import: the real file chooser hands the saved file back, and the service dry-runs it.
  const dryRun = page.waitForRequest((request) => request.method() === "POST" && new URL(request.url()).pathname === "/api/library/import");
  const choosing = page.waitForEvent("filechooser");
  await library.getByRole("button", { name: "Import", exact: true }).click();
  await (await choosing).setFiles({ name: download.suggestedFilename(), mimeType: "application/json", buffer: Buffer.from(text) });
  expect((await dryRun).postDataJSON()).toMatchObject({ dry_run: true, format: "orbyn-library", version: 1 });

  // The dry run draws the service's report: every entry clashes, so each would be renamed.
  const dialog = page.getByRole("dialog", { name: "Import library" });
  await expect(dialog.getByText(DRY_RUN_NOTE)).toBeVisible();
  const table = dialog.getByRole("table", { name: "Import by kind" });
  await expect(table.locator("tbody tr").filter({ hasText: "Workflows" }).locator("td")).toHaveText(["0", "1", "0"]);
  await expect(table.locator("tbody tr").filter({ hasText: "Prompts" }).locator("td")).toHaveText(["0", "1", "0"]);
  await expect(table.locator("tbody tr").filter({ hasText: "Env sets" }).locator("td")).toHaveText(["0", "1", "0"]);
  await expect(table.locator("tfoot td")).toHaveText(["0", "3", "0"]);
  // A dry run writes nothing: the service still holds the one seeded item of each kind.
  expect(await counts()).toEqual({ workflows: 1, prompts: 1, envSets: 1 });

  // Import applies the same document: the service creates renamed copies and the caller reloads.
  const applying = page.waitForRequest((request) => request.method() === "POST" && new URL(request.url()).pathname === "/api/library/import");
  await dialog.getByRole("button", { name: "Import 3 items" }).click();
  expect((await applying).postDataJSON()).toMatchObject({ dry_run: false });
  await expect(dialog.getByText("Imported 3 items to Personal.")).toBeVisible();
  await dialog.getByRole("button", { name: "Close", exact: true }).click();
  await expect(dialog).toHaveCount(0);

  // The service holds both copies, and the new one is marked as imported.
  const workflows = await service.api<{ name: string; source: string }[]>("GET", "/api/library/workflows");
  expect(workflows.map((one) => one.name).sort()).toEqual(["Start dev server", "Start dev server (2)"]);
  expect(workflows.find((one) => one.name === "Start dev server (2)")?.source).toBe("import");

  // The Personal list reflects the import: six rows, and the renamed copies are rows of their own.
  await library.getByRole("tab", { name: /^Personal/ }).click();
  await expect(library.locator("button.lib-row")).toHaveCount(6);
  await expect(library.getByRole("button", { name: /^Start dev server \(2\)/ })).toContainText("Imported");
  await expect(library.getByRole("button", { name: /^Explain a diff \(2\)/ })).toBeVisible();
  await expect(library.getByRole("button", { name: /^work-laptop \(2\)/ })).toBeVisible();
});

test("a service too old for the routes says so instead of failing", async ({ page }) => {
  // A 404 on either route is an older service: the panel names it, and no file is offered again for import.
  await page.route((url) => url.pathname === "/api/library/export", (route) => route.fulfill({ status: 404, contentType: "application/json", body: JSON.stringify({ code: "not_found", message: "No such route." }) }));
  const library = await openLibrary(page);
  await library.getByRole("button", { name: "Export", exact: true }).click();
  await expect(library.getByText(NO_EXPORT)).toBeVisible();

  await page.route((url) => url.pathname === "/api/library/import", (route) => route.fulfill({ status: 404, contentType: "application/json", body: JSON.stringify({ code: "not_found", message: "No such route." }) }));
  const choosing = page.waitForEvent("filechooser");
  await library.getByRole("button", { name: "Import", exact: true }).click();
  await (await choosing).setFiles({ name: "orbyn-library-old.json", mimeType: "application/json", buffer: Buffer.from(JSON.stringify({ format: "orbyn-library", version: 1, exported_at: "2026-11-22T00:00:00Z", profiles: [], workflows: [], prompts: [], env_sets: [] })) });
  const dialog = page.getByRole("dialog", { name: "Import library" });
  await expect(dialog.getByText(NO_IMPORT)).toBeVisible();
  // Another file cannot fix a missing route, so the sheet does not offer one.
  await expect(dialog.getByRole("button", { name: /Choose another file/ })).toHaveCount(0);
  await dialog.getByRole("button", { name: "Close", exact: true }).click();
});
