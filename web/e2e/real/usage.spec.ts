import { expect, test, type Page } from "@playwright/test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { openDetails, startProject, type Service } from "./service";

/** The usage page over a synthetic history, and limits from the protocol double. */

let service: Service;
let projectId = "";

const history = (model: string) =>
  `${JSON.stringify({
    timestamp: new Date().toISOString(),
    message: { id: `m-${model}`, model, usage: { input_tokens: 1_000_000, output_tokens: 500_000, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 } },
  })}\n`;

/** The app's slow reads (web/src/api/lanes.ts): two run at once, and the rest queue behind them. */
const SLOW_READ =
  /^\/api\/(?:limits|usage|pulls|health\/dependencies)(?:\/|$)|^\/api\/(?:engines|projects|threads)\/[^/]+\/(?:models|tools|commands|limits|pulls)(?:\/|$)/;

/**
 * Watches the page's slow reads and resolves once none is left and none has changed for 300 ms.
 * The details card's Limits row shares their two slots: opened while the thread page's own reads
 * (commands, models) still hold them, its first read is aborted before it starts and the shared
 * limits store keeps that abort as a failed check, so no Limits row draws. Wait for the page to
 * finish reading what it opened with first, as a person reading the thread does.
 */
function slowReadsSettled(page: Page): () => Promise<void> {
  const inFlight = new Set<unknown>();
  let changedAt = Date.now();
  const isSlow = (request: { url(): string }) => {
    try {
      return SLOW_READ.test(new URL(request.url()).pathname);
    } catch {
      return false;
    }
  };
  page.on("request", (request) => {
    if (!isSlow(request)) return;
    inFlight.add(request);
    changedAt = Date.now();
  });
  const done = (request: unknown) => {
    if (inFlight.delete(request)) changedAt = Date.now();
  };
  page.on("requestfinished", done);
  page.on("requestfailed", done);
  return async () => {
    await expect
      .poll(() => inFlight.size === 0 && Date.now() - changedAt > 300, {
        message: "the thread page's slow reads settle before the details card opens",
        timeout: 20_000,
      })
      .toBe(true);
  };
}

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ fakeEngines: true }));
  const folder = join(service.dataDir, "../claude/projects/p");
  mkdirSync(folder, { recursive: true });
  writeFileSync(join(folder, "s.jsonl"), history("house-model") + history("other-model"));
});
test.afterAll(() => service.stop());

test("a model takes a price, follows another model and refreshes; keys move the page", async ({ page }) => {
  await page.goto(`${service.page}#/usage`);
  const usage = page.getByRole("region", { name: "Usage" });
  await usage.getByRole("button", { name: "Refresh usage" }).click();
  await usage.getByRole("button", { name: "Set price for house-model" }).click();
  const dialog = page.getByRole("dialog", { name: "Price for house-model" });
  await dialog.getByLabel("Input", { exact: true }).fill("2");
  await dialog.getByLabel("Output").fill("10");
  await dialog.getByLabel("Cached input").fill("0");
  await dialog.getByLabel("Cache write").fill("0");
  await dialog.getByRole("button", { name: "Save price" }).click();
  await expect(dialog).toBeHidden();
  await page.keyboard.press("c");
  await expect(usage.getByRole("radio", { name: "Cost" })).toBeChecked();
  await expect(usage.getByRole("row", { name: /house-model/ })).toContainText("$7");
  await expect(usage.getByRole("row", { name: /other-model/ })).toContainText("Unpriced");

  await usage.getByRole("button", { name: "Set price for other-model" }).click();
  const alias = page.getByRole("dialog", { name: "Price for other-model" });
  await alias.getByLabel("Counts as").fill("house-model");
  await alias.getByRole("button", { name: "Save price" }).click();
  await expect(alias).toBeHidden();
  await expect(usage.getByText("Unpriced")).toBeHidden();

  await page.keyboard.press("t");
  await expect(usage.getByRole("radio", { name: "Tokens" })).toBeChecked();
  await page.keyboard.press("l");
  await expect(usage.getByRole("tab", { name: "Limits" })).toHaveAttribute("aria-selected", "true");
  // Only Codex reports a window; Claude Code reads Not reported, with no bar of its own.
  await expect(usage.getByRole("heading", { name: /^Codex/ })).toBeVisible();
  await expect(usage.getByRole("meter").first()).toBeVisible();
  await expect(usage.locator(".limit-pct").first()).toContainText("% used");
  await expect(usage.getByText("Not reported").first()).toBeVisible();
});

test("thread details show what's left, and checks stay five minutes apart", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  const engines = await service.api<{ id: string }[]>("GET", "/api/engines");
  const engine = engines.find((entry) => entry.id === "codex")?.id;
  const create = (title: string) =>
    service.api<{ id: string }>("POST", "/api/threads", { project_id: projectId, engine, title, model: null, worktree: null, skill_profile: null });
  const first = await create("One");
  await create("Two");
  let checks = 0;
  page.on("request", (request) => {
    if (/\/api\/threads\/[^/]+\/limits/.test(request.url())) checks += 1;
  });
  const settled = slowReadsSettled(page);
  // The thread page opens with two slow reads of its own: the composer's slash commands and the
  // engine's models. Wait for both, then for the lane to fall quiet, before the details card reads.
  const pageReads = Promise.all([
    page.waitForResponse((response) => /\/api\/threads\/[^/]+\/commands$/.test(new URL(response.url()).pathname)),
    page.waitForResponse((response) => /\/api\/engines\/[^/]+\/models$/.test(new URL(response.url()).pathname)),
  ]);
  await page.goto(`${service.page}#/threads/${first.id}`);
  await pageReads;
  await settled();
  await openDetails(page);
  // The shipped copy, over the protocol double's own report: the 5-hour window is 12% used and the
  // weekly one 40%, so what is left reads as these two windows.
  await expect(page.getByText("5 hours 88% left")).toBeVisible();
  await expect(page.getByText("7 days 60% left")).toBeVisible();
  await page.locator("li[data-thread-item]").filter({ hasText: "Two" }).click();
  await expect(page.getByRole("heading", { name: "Two" })).toBeVisible();
  await openDetails(page);
  await expect(page.getByText("5 hours 88% left")).toBeVisible();
  await expect(page.getByText("7 days 60% left")).toBeVisible();
  expect(checks).toBe(1);
});

test("the provider menu opens above the usage dialog and goes back to all providers", async ({ page }) => {
  await page.goto(`${service.page}#/usage`);
  const usage = page.getByRole("region", { name: "Usage" });
  const picker = usage.getByRole("button", { name: "All providers" });
  await picker.click();
  // A click lands only on what is on top, so this fails while the menu sits under the dialog.
  await page.getByRole("menuitem", { name: "Codex" }).click();
  await expect(usage.getByRole("button", { name: "Codex" })).toBeVisible();
  await usage.getByRole("button", { name: "Codex" }).click();
  await page.getByRole("menuitem", { name: "All providers" }).click();
  await expect(picker).toBeVisible();
});
