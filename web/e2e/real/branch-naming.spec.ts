import { expect, test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { startProject, type Service } from "./service";

/** F25: a new worktree's branch name follows the naming mode over the real service and real Git. */

interface Listed {
  id: string;
  title: string;
  worktree: { path: string; branch: string } | null;
}

let service: Service;
let projectId = "";

const box = (page: Page) => page.getByRole("textbox", { name: /^Message/ });
const checkout = (page: Page) => page.locator(".composer-context");

/** The catalog defaults, so one test's naming mode cannot decide the next one's branch. */
const DEFAULTS = { branch_naming_mode: "static", branch_name_prefix: "feat", branch_name_instructions: "" } as const;

const preferences = (values: Record<string, unknown>) => service.api("PUT", "/api/preferences", { values });

/** A purpose atom, so each new worktree gets its own folder and branch base. */
const newPurpose = (lead: string) => `${lead}-${Date.now().toString(36)}`;

const newWorktreeThread = async (page: Page, purpose: string, message: string) => {
  await page.goto(`${service.page}#/new/${projectId}`);
  await expect(box(page)).toBeVisible();
  await checkout(page).getByRole("button", { name: /checkout|worktree/i }).click();
  await page.getByRole("menuitem", { name: "New worktree" }).click();
  await page.getByRole("textbox", { name: "Worktree purpose" }).fill(purpose);
  await box(page).fill(message);
  await box(page).press("Enter");
  await expect(page).toHaveURL(/#\/threads\//);
  const id = decodeURIComponent(page.url().split("/threads/")[1] ?? "");
  return service.api<Listed>("GET", `/api/threads/${id}`);
};

/** The service's own Git repository has the ref, so the name is a real branch, not a reply field. */
const localBranch = (branch: string) =>
  execFileSync("git", ["-C", service.repo, "rev-parse", "--verify", `refs/heads/${branch}`], { encoding: "utf8" }).trim();

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ scenes: true }));
});
test.afterAll(() => service.stop());
test.beforeEach(async ({ page }) => {
  await preferences(DEFAULTS);
  await page.setViewportSize({ width: 1400, height: 900 });
});

test("static naming keeps the prefix and the purpose", async ({ page }) => {
  await preferences({ branch_naming_mode: "static", branch_name_prefix: "team" });
  const purpose = newPurpose("static-one");
  const thread = await newWorktreeThread(page, purpose, "Static proof");
  expect(thread.worktree?.branch).toBe(`team/${purpose}`);
  expect(localBranch(`team/${purpose}`)).not.toBe("");
});

test("semantic naming slugs the draft's title and numbers a collision", async ({ page }) => {
  await preferences({ branch_naming_mode: "semantic" });
  const first = await newWorktreeThread(page, newPurpose("login-one"), "Ship the login flow");
  expect(first.worktree?.branch).toBe("feat/ship-the-login-flow");
  const second = await newWorktreeThread(page, newPurpose("login-two"), "Ship the login flow");
  expect(second.worktree?.branch).toBe("feat/ship-the-login-flow-2");
  expect(localBranch("feat/ship-the-login-flow")).not.toBe("");
  expect(localBranch("feat/ship-the-login-flow-2")).not.toBe("");
});

test("custom naming uses the name the engine writes from the instructions", async ({ page }) => {
  await preferences({
    branch_naming_mode: "custom",
    branch_name_instructions: "Use release/ and the issue number 42.",
  });
  const thread = await newWorktreeThread(page, newPurpose("custom-one"), "Rewrite the login");
  expect(thread.worktree?.branch).toBe("release/rewrite-login");
  expect(localBranch("release/rewrite-login")).not.toBe("");
});
