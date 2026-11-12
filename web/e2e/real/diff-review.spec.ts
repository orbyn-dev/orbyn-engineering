import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { openDetails, send, settled, startProject, type Service } from "./service";

let service: Service;
let projectId = "";

const commit = (message: string) => {
  execFileSync("git", ["add", "."], { cwd: service.repo });
  execFileSync("git", ["-c", "user.name=T", "-c", "user.email=t@example.com", "commit", "-qm", message], { cwd: service.repo });
};

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ fakeEngines: true }));
});
test.afterAll(async () => { await service?.stop(); });

const thread = (title: string) => service.api<{ id: string }>("POST", "/api/threads", {
  project_id: projectId, engine: "codex", model: null, title, worktree: null, skill_profile: null,
});

test("E10: a diff range comment shows inline, follows its chip and reaches the prompt", async ({ page }) => {
  writeFileSync(join(service.repo, "review.txt"), "alpha\nbravo\ncharlie\n");
  commit("Review base");
  writeFileSync(join(service.repo, "review.txt"), "alpha\nBRAVO\ncharlie\n");
  const { id } = await thread("Diff review");
  await page.setViewportSize({ width: 1500, height: 900 });
  await page.goto(`${service.page}#/threads/${id}`);
  await (await openDetails(page)).getByRole("button", { name: /^Changes/ }).click();
  const file = page.locator("[data-diff-file='review.txt']");
  await file.getByRole("button", { name: "Comment on line 1", exact: true }).click();
  await file.getByRole("button", { name: "Comment on line 2", exact: true }).click({ modifiers: ["Shift"] });
  await file.getByLabel("Review comment on review.txt line 1–2").fill("Why uppercase?");
  await file.getByRole("button", { name: "Comment", exact: true }).click();

  const note = file.getByRole("note", { name: "Review comment review.txt:1–2" });
  await expect(note).toContainText("Why uppercase?");
  const chip = page.getByRole("button", { name: "Review comment review.txt:1–2", exact: true });
  await expect(chip).toBeVisible();
  // Editing the chip edits the annotation: there is one comment, not two.
  await chip.click();
  const details = page.getByRole("dialog", { name: "Review comment review.txt:1–2", exact: true });
  await expect(details.locator("pre")).toHaveText("alpha\nBRAVO");
  await details.getByRole("textbox", { name: "Review comment", exact: true }).fill("Keep it lowercase");
  await details.getByRole("button", { name: "Save comment" }).click();
  await expect(note).toContainText("Keep it lowercase");

  // Removing the annotation removes the chip.
  await note.getByRole("button", { name: "Remove comment" }).click();
  await expect(chip).toHaveCount(0);

  await file.getByRole("button", { name: "Comment on line 2", exact: true }).click();
  await file.getByLabel("Review comment on review.txt line 2").fill("Sent from the diff");
  await file.getByRole("button", { name: "Comment", exact: true }).click();
  await send(page, "look at my comment");
  await settled(service, id);
  const messages = JSON.stringify(await service.api("GET", `/api/threads/${id}/messages?limit=10`));
  expect(messages).toContain("Review on `review.txt:2`");
  expect(messages).toContain("Sent from the diff");
  expect(messages).toContain("BRAVO");
  // A sent comment stays under its line, now read as sent.
  await expect(file.getByRole("note", { name: /Review comment/ })).toContainText("Sent to the agent");
});

test("E18: a large uncommitted diff lists every file, and find counts matches across all of them", async ({ page }) => {
  const dir = join(service.repo, "bulk");
  mkdirSync(dir, { recursive: true });
  for (let at = 0; at < 130; at += 1) writeFileSync(join(dir, `file-${String(at).padStart(3, "0")}.txt`), `marker ${at}\n`);
  const { id } = await thread("Large diff");
  await page.setViewportSize({ width: 1500, height: 900 });
  await page.goto(`${service.page}#/threads/${id}`);
  await (await openDetails(page)).getByRole("button", { name: /^Changes/ }).click();
  // One file's lines are drawn at a time, so the list never mounts 130 diffs: every file is a row.
  const rows = page.locator("[data-diff-row^='bulk/']");
  await expect(rows.first()).toBeVisible();
  // The list draws 50 at a time and ends with a row that shows more.
  await expect(rows).toHaveCount(50);
  await page.getByRole("button", { name: /^Show \d+ more files$/ }).click();
  await expect(rows).toHaveCount(100);
  await page.getByRole("button", { name: /^Show \d+ more files$/ }).click();
  await expect(rows).toHaveCount(130);
  await expect(page.locator("[data-diff-file]")).toHaveCount(1);
  await page.getByRole("group", { name: "Patch" }).focus();
  await page.keyboard.press("ControlOrMeta+f");
  await page.getByRole("searchbox", { name: "Find in diff" }).fill("marker");
  await expect(page.getByText(/^1 of 1[3-9]\d$/)).toBeVisible();
});
