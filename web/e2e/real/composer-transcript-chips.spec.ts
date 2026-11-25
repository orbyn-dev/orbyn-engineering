import { expect, test, type Page } from "@playwright/test";
import { TRANSCRIPT_TYPE } from "../../src/lib/transcript";
import { settled, startProject, type Service } from "./service";

/**
 * B43, the composer's transcript drop, end to end on the real service: a transcript exported by the real export route is
 * dragged onto another thread's composer and posted to the real Person attach route, becoming a chip with the service's
 * title and message count; an over-2 MiB file is refused before it leaves the page, and a newer-format file is uploaded
 * and refused by the route, leaving no chip; and the kept transcript's id rides on the next message's run.
 */

let service: Service;
let projectId: string;

/** A thread with one finished turn, made through the real routes. */
async function finishedThread(title: string): Promise<{ id: string; doc: unknown }> {
  const thread = await service.api<{ id: string }>("POST", "/api/threads", {
    project_id: projectId, engine: "codex", model: null, title, worktree: null, skill_profile: null,
  });
  await service.api("POST", `/api/threads/${thread.id}/runs`, { prompt: "one" });
  await settled(service, thread.id);
  const doc = await service.api<unknown>("GET", `/api/threads/${thread.id}/export`);
  return { id: thread.id, doc };
}

/** Drops one real File on the composer, the way a browser hands a dragged file to it. */
async function dropFile(page: Page, file: { name: string; type: string; text: string }) {
  await page.locator(".composer-box").evaluate((box, held) => {
    const transfer = new DataTransfer();
    transfer.items.add(new File([held.text], held.name, { type: held.type }));
    for (const type of ["dragenter", "dragover", "drop"]) {
      box.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: transfer }));
    }
  }, file);
}

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ fakeEngines: true }));
});

test.afterAll(async () => {
  await service?.stop();
});

test("a dropped transcript is attached through the real route and rides the next message", async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize({ width: 1500, height: 1000 });
  const source = await finishedThread("Login redirect evidence");
  const target = await finishedThread("Compare the redirect");
  await page.goto(`${service.page}#/threads/${target.id}`);
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible({ timeout: 30_000 });

  const attached = page.waitForResponse(
    (response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/threads/${target.id}/transcripts`,
    { timeout: 30_000 },
  );
  await dropFile(page, { name: "login-redirect.orbyn.json", type: TRANSCRIPT_TYPE, text: JSON.stringify(source.doc) });
  const answer = await attached;
  expect(answer.status()).toBe(201);
  const body = (await answer.json()) as { attachment: { id: string; name: string }; message_count: number };
  expect(body.attachment.name).toMatch(/^Imported transcript - .+\.txt$/);
  expect(body.message_count).toBeGreaterThan(0);
  // The chip: the service's title and the count it read, not the file's name.
  const chipText = body.attachment.name.replace(/^Imported transcript - /, "").replace(/\.txt$/, "");
  const chips = page.getByRole("list", { name: "Transcripts" });
  await expect(chips.getByText(chipText, { exact: true })).toBeVisible();
  await expect(chips.getByText(`· ${body.message_count.toLocaleString()} ${body.message_count === 1 ? "message" : "messages"}`)).toBeVisible();

  // The kept transcript's id is one of the run's attachments.
  const started = page.waitForResponse(
    (response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/threads/${target.id}/runs`,
    { timeout: 30_000 },
  );
  await page.getByRole("textbox", { name: /^Message/ }).fill("compare");
  await page.keyboard.press("Enter");
  const run = JSON.parse((await started).request().postData() ?? "{}") as { attachment_ids?: string[] };
  expect(run.attachment_ids).toContain(body.attachment.id);
  await settled(service, target.id);
});

test("an over-2 MiB transcript and a newer format are refused: only the oversized file never leaves the page", async ({ page }) => {
  await page.setViewportSize({ width: 1500, height: 1000 });
  const target = await finishedThread("Refused transcripts");
  await page.goto(`${service.page}#/threads/${target.id}`);
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible({ timeout: 30_000 });
  const posts: string[] = [];
  page.on("request", (request) => {
    if (request.method() === "POST" && new URL(request.url()).pathname === `/api/threads/${target.id}/transcripts`) posts.push(request.url());
  });

  await dropFile(page, { name: "big.orbyn.json", type: TRANSCRIPT_TYPE, text: "x".repeat(2 * 1024 * 1024 + 1) });
  await expect(page.getByText("Transcripts can be up to 2 MiB.", { exact: true })).toBeVisible();

  const newer = { format: "orbyn-transcript", version: 2, exported_at: "2026-11-22T10:00:00Z", title: "A newer one", messages: [] };
  await dropFile(page, { name: "newer.orbyn.json", type: TRANSCRIPT_TYPE, text: JSON.stringify(newer) });
  await expect(page.getByText("Made by a newer Orbyn. Update Orbyn to read it.", { exact: true })).toBeVisible();
  // Refused means nothing was kept: no transcript chip, and the refused file is only in the strip, not attached to a message.
  await expect(page.getByRole("list", { name: "Transcripts" })).toHaveCount(0);
  // The service was asked only for the newer one; the oversized file never left the page.
  await expect.poll(() => posts.length).toBe(1);
});
