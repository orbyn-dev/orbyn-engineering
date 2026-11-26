import { expect, test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { projectSettingsUrl, startProject, type Service } from "./service";

/**
 * F70: the Settings › Projects › Repositories scan and `.code-workspace` import dialogs, through a real `orbynd` with
 * `ORBYN_MULTI_REPO=1` and a real browser.
 *
 * The scan route is Person-class and writes nothing: a real folder's Git children are listed, and one result is added by the
 * dialog's own Add (one `add-folder` write) and trusted through the app's trust prompt (one `trust` write). Then a real
 * `.code-workspace` file the test writes is imported: its folders are listed, the folder outside the file's own folder is
 * refused and has no Add, and one folder from it is added and trusted. Each step reads the service's repository list back, so
 * the browser's own writes are the only ones that change it. Nothing is routed, slowed, stubbed or skipped.
 */

test.setTimeout(90_000);

const WINDOW = { width: 1500, height: 1000 };

let service: Service;
let projectId = "";
/** A real folder of two Git repositories, scanned as a plain folder: no `.code-workspace` file sits at its top. */
let scanRoot = "";
let scanOne = "";
let scanTwo = "";
/** The folder of a real `.code-workspace` file: one Git folder inside it, one Git folder of its parent (outside), one missing. */
let importRoot = "";
let importRepo = "";
let outsideRepo = "";
let workspaceFile = "";

/** A real Git repository with one commit on main, inside `parent`. */
function makeRepo(parent: string, name: string): string {
  const path = join(parent, name);
  mkdirSync(path);
  writeFileSync(join(path, "README.md"), `# ${name}\n`);
  for (const args of [["init", "-q", "-b", "main"], ["add", "."], ["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", "First"]]) {
    execFileSync("git", args, { cwd: path, stdio: "ignore" });
  }
  return path;
}

interface RepoRow {
  id: string;
  name: string;
  path: string;
  branch: string | null;
  trusted: boolean;
}
interface RepoScan {
  path: string;
  code_workspace_file: string | null;
  folders: { name: string; path: string; is_git: boolean; source: string; in_project: boolean; refused: string | null }[];
}
interface Write {
  kind: "scan" | "add-folder" | "trust";
  pathname: string;
  body: Record<string, unknown> | null;
}

/**
 * The dialog's own writes, in order, observed on the real requests the browser sends: the scan (which writes nothing) and, per
 * added row, one `add-folder` and then one `trust`. A listen, not a route or a stub.
 */
function watchWrites(page: Page): Write[] {
  const writes: Write[] = [];
  page.on("request", (request) => {
    if (request.method() !== "POST") return;
    const pathname = new URL(request.url()).pathname;
    if (!/^\/api\/projects\/[^/]+\/repos\/(scan|add-folder|[^/]+\/trust)$/.test(pathname)) return;
    const kind = pathname.endsWith("/repos/scan") ? "scan" : pathname.endsWith("/repos/add-folder") ? "add-folder" : "trust";
    const body = request.postData();
    writes.push({ kind, pathname, body: body === null ? null : (JSON.parse(body) as Record<string, unknown>) });
  });
  return writes;
}

const repositories = (page: Page) => page.getByRole("region", { name: "Repositories" });
const serviceRepos = async (): Promise<RepoRow[]> => (await service.api<{ repositories: RepoRow[] }>("GET", `/api/projects/${projectId}/repos`)).repositories;

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ multiRepo: true, fakeEngines: true }));
  const root = dirname(service.dataDir);
  scanRoot = join(root, "scan-root");
  mkdirSync(scanRoot);
  scanOne = makeRepo(scanRoot, "scan-one");
  scanTwo = makeRepo(scanRoot, "scan-two");
  importRoot = join(root, "import-root");
  mkdirSync(importRoot);
  importRepo = makeRepo(importRoot, "import-repo");
  outsideRepo = makeRepo(root, "import-outside");
  workspaceFile = join(importRoot, "evidence.code-workspace");
  writeFileSync(workspaceFile, JSON.stringify({ folders: [{ path: "import-repo" }, { path: outsideRepo }, { path: "missing-folder" }] }));
});
test.afterAll(() => service.stop());
test.beforeEach(async ({ page }) => {
  await page.setViewportSize(WINDOW);
});

test("Settings scans a real folder, adds one listed row with one write and trusts it", async ({ page }) => {
  const writes = watchWrites(page);
  await page.goto(await projectSettingsUrl(service, projectId));
  const section = repositories(page);
  await expect(section).toBeVisible();
  await expect(section.getByRole("listitem")).toHaveCount(1);
  expect((await serviceRepos()).map((row) => row.name)).toEqual(["my-repo"]);

  // The dialog reads no folder and writes nothing until the person asks for a scan.
  await section.getByRole("button", { name: "Add repository" }).click();
  await page.getByRole("menuitem", { name: "Scan a folder…" }).click();
  const dialog = page.getByRole("dialog", { name: "Scan a folder" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("region", { name: "Scan results" })).toHaveCount(0);
  expect(writes).toEqual([]);

  await dialog.getByLabel("Folder on this computer").fill(scanRoot);
  const scanning = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/projects/${projectId}/repos/scan`);
  await dialog.getByRole("button", { name: "Scan", exact: true }).click();
  const scan = (await (await scanning).json()) as RepoScan;
  expect(scan.path).toBe(scanRoot);
  expect(writes.map((write) => write.kind)).toEqual(["scan"]);
  expect(writes[0]?.body).toEqual({ path: scanRoot });

  // The result rows are the real folder's: name, folder and Git, with an Add per row.
  const results = dialog.getByRole("region", { name: "Scan results" });
  await expect(results).toBeVisible();
  await expect(results).toContainText("2 folders found.");
  await expect(results.getByRole("listitem")).toHaveCount(2);
  const one = results.getByRole("listitem").filter({ hasText: "scan-one" });
  const two = results.getByRole("listitem").filter({ hasText: "scan-two" });
  await expect(one.locator(".repos-name")).toHaveText("scan-one");
  await expect(one.locator(".repos-path")).toHaveText(scanOne);
  await expect(one.getByText("Git", { exact: true })).toBeVisible();
  await expect(one.getByRole("button", { name: "Add scan-one" })).toBeEnabled();
  await expect(two.locator(".repos-path")).toHaveText(scanTwo);
  await expect(two.getByRole("button", { name: "Add scan-two" })).toBeEnabled();
  // The scan wrote nothing: the project still holds its own folder only.
  expect((await serviceRepos()).map((row) => row.name)).toEqual(["my-repo"]);

  // Add is one write for the row that was clicked. The service answers the row untrusted, as add-folder does.
  const adding = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/projects/${projectId}/repos/add-folder`);
  await one.getByRole("button", { name: "Add scan-one" }).click();
  const added = (await (await adding).json()) as RepoRow;
  expect(added.name).toBe("scan-one");
  expect(added.path).toBe(scanOne);
  expect(added.branch).toBe("main");
  expect(added.trusted).toBe(false);
  await expect(one.getByText("Added, not trusted", { exact: true })).toBeVisible();
  await expect(one.getByRole("button", { name: "Add scan-one" })).toHaveCount(0);
  expect(writes.map((write) => write.kind)).toEqual(["scan", "add-folder"]);
  expect(writes[1]?.body).toEqual({ path: scanOne });
  expect((await serviceRepos()).map((row) => [row.name, row.trusted])).toEqual([["my-repo", true], ["scan-one", false]]);

  // Trust is the app's own prompt, one more write; then the row reads trusted and the service agrees.
  await one.getByRole("button", { name: "Trust…" }).click();
  const prompt = page.getByRole("dialog", { name: "Trust scan-one?" });
  await expect(prompt).toBeVisible();
  await expect(prompt).toContainText("Agents can read files, run commands and edit code in this folder.");
  await expect(prompt).toContainText("Trusting this folder does not trust the project's other repositories.");
  const trusting = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/projects/${projectId}/repos/${added.id}/trust`);
  await prompt.getByRole("button", { name: "Trust folder", exact: true }).click();
  expect(((await (await trusting).json()) as RepoRow).trusted).toBe(true);
  await expect(prompt).toHaveCount(0);
  await expect(one.getByText("Added, trusted", { exact: true })).toBeVisible();
  expect(writes.map((write) => write.kind)).toEqual(["scan", "add-folder", "trust"]);
  expect(writes[2]?.pathname).toBe(`/api/projects/${projectId}/repos/${added.id}/trust`);
  expect(writes[2]?.body).toBeNull();
  expect((await serviceRepos()).map((row) => [row.name, row.trusted])).toEqual([["my-repo", true], ["scan-one", true]]);

  // Closing the dialog, the section lists the added repository with the branch the service answered and its trust state.
  await dialog.getByRole("button", { name: "Done", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(section.getByRole("listitem")).toHaveCount(2);
  const row = section.getByRole("listitem").filter({ hasText: "scan-one" });
  await expect(row.locator(".repos-name")).toHaveText("scan-one");
  await expect(row.locator(".repos-path")).toHaveText(scanOne);
  await expect(row.locator(".repos-branch")).toHaveText("main");
  await expect(row.getByText("Trusted", { exact: true })).toBeVisible();
});

test("Settings imports a real .code-workspace file, refuses a folder outside it, and adds one listed row with one write then trusts it", async ({ page }) => {
  const writes = watchWrites(page);
  await page.goto(await projectSettingsUrl(service, projectId));
  const section = repositories(page);
  await expect(section).toBeVisible();
  const before = await serviceRepos();
  expect(before.map((row) => row.name)).not.toContain("import-repo");

  await section.getByRole("button", { name: "Add repository" }).click();
  await page.getByRole("menuitem", { name: "Import a .code-workspace file…" }).click();
  const dialog = page.getByRole("dialog", { name: "Import a .code-workspace file" });
  await expect(dialog).toBeVisible();
  expect(writes).toEqual([]);

  await dialog.getByLabel(".code-workspace file").fill(workspaceFile);
  const reading = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/projects/${projectId}/repos/scan`);
  await dialog.getByRole("button", { name: "Read file", exact: true }).click();
  const scan = (await (await reading).json()) as RepoScan;
  expect(scan.code_workspace_file).toBe("evidence.code-workspace");
  expect(writes.map((write) => write.kind)).toEqual(["scan"]);
  expect(writes[0]?.body).toEqual({ path: workspaceFile });

  // The rows are the file's real folders: the one inside is addable, the one outside is refused in words and has no Add.
  const results = dialog.getByRole("region", { name: "Scan results" });
  await expect(results).toBeVisible();
  await expect(results).toContainText("From evidence.code-workspace.");
  await expect(results).toContainText("3 folders found.");
  await expect(results.getByRole("listitem")).toHaveCount(3);
  const inside = results.getByRole("listitem").filter({ hasText: "import-repo" });
  const outside = results.getByRole("listitem").filter({ hasText: "import-outside" });
  const missing = results.getByRole("listitem").filter({ hasText: "missing-folder" });
  await expect(inside.locator(".repos-path")).toHaveText(importRepo);
  await expect(inside.getByText("Git", { exact: true })).toBeVisible();
  await expect(inside.getByRole("button", { name: "Add import-repo" })).toBeEnabled();
  await expect(outside).toContainText("Outside the workspace file's folder.");
  await expect(outside.getByRole("button", { name: /^Add / })).toHaveCount(0);
  await expect(missing).toContainText("This folder does not exist.");
  await expect(missing.getByRole("button", { name: /^Add / })).toHaveCount(0);
  // Reading a workspace file wrote nothing.
  expect((await serviceRepos()).map((row) => row.name)).toEqual(before.map((row) => row.name));

  const adding = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/projects/${projectId}/repos/add-folder`);
  await inside.getByRole("button", { name: "Add import-repo" }).click();
  const added = (await (await adding).json()) as RepoRow;
  expect(added.name).toBe("import-repo");
  expect(added.path).toBe(importRepo);
  expect(added.branch).toBe("main");
  expect(added.trusted).toBe(false);
  await expect(inside.getByText("Added, not trusted", { exact: true })).toBeVisible();
  expect(writes.map((write) => write.kind)).toEqual(["scan", "add-folder"]);
  expect(writes[1]?.body).toEqual({ path: importRepo });
  expect((await serviceRepos()).map((row) => [row.name, row.trusted])).toEqual([...before.map((row): [string, boolean] => [row.name, row.trusted]), ["import-repo", false]]);

  await inside.getByRole("button", { name: "Trust…" }).click();
  const prompt = page.getByRole("dialog", { name: "Trust import-repo?" });
  await expect(prompt).toBeVisible();
  const trusting = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/projects/${projectId}/repos/${added.id}/trust`);
  await prompt.getByRole("button", { name: "Trust folder", exact: true }).click();
  expect(((await (await trusting).json()) as RepoRow).trusted).toBe(true);
  await expect(prompt).toHaveCount(0);
  await expect(inside.getByText("Added, trusted", { exact: true })).toBeVisible();
  expect(writes.map((write) => write.kind)).toEqual(["scan", "add-folder", "trust"]);
  expect((await serviceRepos()).map((row) => [row.name, row.trusted])).toContainEqual(["import-repo", true]);

  await dialog.getByRole("button", { name: "Done", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(section.getByRole("listitem")).toHaveCount(before.length + 1);
  const row = section.getByRole("listitem").filter({ hasText: "import-repo" });
  await expect(row.locator(".repos-path")).toHaveText(importRepo);
  await expect(row.locator(".repos-branch")).toHaveText("main");
  await expect(row.getByText("Trusted", { exact: true })).toBeVisible();
});
