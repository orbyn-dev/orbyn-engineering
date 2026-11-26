import { expect, test } from "@playwright/test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { libraryReady, startService, type Service } from "./service";

/**
 * The real service's "Add existing folder" flow, driven through its browse list:
 * the listing rows and the parent row the service answers over GET /api/fs/browse,
 * and the shortcut add of the active row. The typed-path add is first-run.spec.ts;
 * this spec also covers the refusal when the same folder is added twice.
 */
const WINDOW = { width: 1280, height: 792 };

let service: Service;

test.beforeAll(async () => {
  service = await startService();
  await libraryReady(service);
});

test.afterAll(() => service.stop());

test("an existing folder is added from the browse list, through its rows and the shortcut add", async ({ page }) => {
  await page.setViewportSize(WINDOW);
  await page.goto(service.page);
  await page.getByRole("button", { name: "Skip for now" }).click();
  // The shipped empty home is the Welcome page; its Projects section adds a folder.
  await expect(page.getByRole("heading", { name: "Welcome to Orbyn Engineering", level: 1 })).toBeVisible();
  const projects = page.getByRole("region", { name: "Projects" });
  // The Welcome page's Add a folder opens the add-project palette on its browse list.
  const browsed = page.waitForResponse((response) => new URL(response.url()).pathname === "/api/fs/browse");
  await projects.getByRole("button", { name: "Add a folder" }).click();
  await browsed;
  const palette = page.getByRole("dialog", { name: "Command palette" });
  const field = palette.getByLabel("Folder path");
  await expect(field).toHaveValue("~/");

  // The listing is the real service's browse of the home folder: the home listing includes my-folder (plain) and my-repo (Git).
  const folder = palette.getByRole("button", { name: "~/my-folder" });
  const repo = palette.getByRole("button", { name: "~/my-repo" });
  await expect(repo).toBeVisible();
  await expect(repo.locator(".add-project-hint")).toHaveText("Git");
  await expect(folder).toBeVisible();
  await expect(folder.locator(".add-project-hint")).toHaveText("Folder");

  // Clicking a listing row descends into it; my-repo holds no folder of its own,
  // so its listing is the parent row alone.
  await repo.click();
  await expect(field).toHaveValue("~/my-repo/");
  await expect(palette.getByRole("button", { name: /^~\// })).toHaveCount(0);
  // The parent row climbs back up, so the listing returns to home.
  await palette.getByRole("button", { name: "..", exact: true }).click();
  await expect(field).toHaveValue("~/");
  await expect(folder).toBeVisible();

  // The shortcut add: the active row is added where it is, with Cmd/Ctrl+Enter.
  await folder.hover();
  await expect(folder).toHaveAttribute("data-active", "true");
  await field.press("ControlOrMeta+Enter");
  // An untrusted project lands on the trust step, as the typed-path add does.
  await expect(page.getByRole("button", { name: "Trust folder" })).toBeVisible();
  await page.getByRole("button", { name: "Trust folder" }).click();
  await expect(page.locator(".hero-project")).toHaveText("my-folder");
  // The folder stays where it is: no repository was made of it.
  expect(existsSync(join(service.folder, ".git"))).toBe(false);
});

test("adding the same folder twice is refused", async ({ page }) => {
  // The first test may not have run: the folder is a project either way.
  await service.api("POST", "/api/projects/add", { path: service.folder }).catch(() => undefined);
  await page.setViewportSize(WINDOW);
  await page.goto(service.page);
  // With a project already added, the sidebar's Add project opens the same palette.
  await page.getByRole("navigation", { name: "Workspace" }).getByRole("button", { name: "Add project" }).click();
  const palette = page.getByRole("dialog", { name: "Command palette" });
  const field = palette.getByLabel("Folder path");
  await expect(field).toHaveValue("~/");

  // The typed path names the folder that is already a project.
  await field.fill("~/my-folder");
  await field.press("Enter");
  await expect(palette.getByRole("alert")).toHaveText("This folder is already a project.");

  // The browse list refuses the same folder through the shortcut add too.
  await field.fill("~/");
  await expect(palette.getByRole("button", { name: "~/my-folder" })).toBeVisible();
  await palette.getByRole("button", { name: "~/my-folder" }).hover();
  await expect(palette.getByRole("button", { name: "~/my-folder" })).toHaveAttribute("data-active", "true");
  await field.click();
  await field.press("ControlOrMeta+Enter");
  await expect(palette.getByRole("alert")).toHaveText("This folder is already a project.");
  // One add succeeded and one refusal never made a second project of the folder.
  const projects = await service.api<{ id: string; name: string }[]>("GET", "/api/projects");
  expect(projects.filter((project) => project.name === "my-folder")).toHaveLength(1);
});
