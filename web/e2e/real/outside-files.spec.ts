import { expect, test } from "@playwright/test";
import { mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startProject, type Service } from "./service";

let service: Service;
let projectId: string;
let outside: string;
test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ fakeEngines: true }));
  outside = realpathSync(mkdtempSync(join(tmpdir(), "orbyn-outside-")));
});
test.afterAll(async () => {
  await service?.stop();
  if (outside) rmSync(outside, { recursive: true, force: true });
});

test("a file outside the checkout opens read-only and its HTML stays isolated", async ({ page }) => {
  const page_ = join(outside, "report.html");
  const html = `<p id="seen">outside page</p><script>parent.document.title = "pwned"; document.body.dataset.ran = "yes";</script>`
    + `<img src="${service.url}/api/threads?probe=outside" alt="probe">`;
  writeFileSync(page_, html);
  writeFileSync(join(service.repo, "inside.txt"), "inside\n");
  const thread = await service.api<{ id: string }>("POST", "/api/threads", {
    project_id: projectId, engine: "codex", model: null, title: "Outside files", worktree: null, skill_profile: null,
  });
  // The page's probe must be stopped by its policy and never answered.
  const answered: string[] = [];
  const blocked: string[] = [];
  page.on("response", (response) => { if (response.url().includes("probe=outside")) answered.push(response.url()); });
  page.on("requestfailed", (request) => { if (request.url().includes("probe=outside")) blocked.push(request.failure()?.errorText ?? ""); });
  await page.goto(`${service.page}#/threads/${thread.id}`);
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();
  const title = await page.title();

  // A message link to a full path opens a file tab for it. The tab's id is its target, so the thread's saved layout
  // can name one: the same tab a click on the link makes.
  const openFullPath = async (path: string) => {
    const id = `file:${encodeURIComponent(JSON.stringify(["t", path]))}`;
    await page.evaluate(([thread, tab]) => {
      const prefs = JSON.parse(localStorage.getItem("orbyn.prefs.v1") ?? "{}");
      const builtin = ["diff", "browser", "activity", "side", "tools"];
      const layout = { panelTab: tab, panelTabOrder: [...builtin, tab], panelTabsClosed: builtin, terminalOpen: false, panelMaximized: false, panelWidth: null, sideChat: null };
      // The open thread's layout is the top-level fields; the others wait under `layouts`.
      Object.assign(prefs, layout, { panelOpen: true, layouts: { ...prefs.layouts, [thread!]: layout } });
      localStorage.setItem("orbyn.prefs.v1", JSON.stringify(prefs));
    }, [thread.id, id]);
    await page.reload();
  };
  await openFullPath(page_);

  const viewer = page.locator(".files-viewer.outside-file");
  await expect(viewer.getByText(/Outside checkout · Read-only/)).toBeVisible();
  for (const name of ["Edit", "Save"]) await expect(viewer.getByRole("radio", { name })).toHaveCount(0);
  await expect(viewer.getByRole("button", { name: "Save" })).toHaveCount(0);
  await expect(viewer.getByRole("link", { name: "Save file" })).toHaveCount(0);
  await expect(viewer.getByRole("button", { name: "Open in editor" })).toHaveCount(0);

  const frame = viewer.locator("iframe.file-frame");
  await expect(frame).toHaveAttribute("sandbox", "");
  await expect(page.frameLocator(".outside-file iframe.file-frame").locator("#seen")).toHaveText("outside page");
  // The page's script never ran, here or in the app, and it fetched nothing from the API.
  await expect(page.frameLocator(".outside-file iframe.file-frame").locator("body")).not.toHaveAttribute("data-ran", "yes");
  expect(await page.title()).toBe(title);
  await expect.poll(() => blocked).toEqual(["csp"]);
  expect(answered).toEqual([]);

  await viewer.getByRole("radio", { name: "Source" }).click();
  await expect(viewer.locator("pre.file-source")).toContainText("parent.document.title");
  expect(readFileSync(page_, "utf8")).toBe(html);

  // A path into Orbyn's data is refused by the service.
  const data = realpathSync(service.dataDir);
  const held = readdirSync(data).map((name) => join(data, name)).find((path) => statSync(path).isFile());
  expect(held).toBeTruthy();
  await openFullPath(held!);
  await expect(page.getByText(/own data isn't shown here/)).toBeVisible();
  // A full path inside the checkout is not an outside file: it opens as the checkout's own, in the editor with Save.
  await openFullPath(join(realpathSync(service.repo), "inside.txt"));
  await expect(page.getByRole("textbox", { name: "Edit inside.txt" })).toHaveValue("inside\n");
  await expect(page.getByRole("button", { name: "Save" })).toBeVisible();
});
