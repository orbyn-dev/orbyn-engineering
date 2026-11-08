import { expect, test } from "@playwright/test";
import { send, settled, startProject, threadIdOf, type Service } from "./service";

let service: Service;
let projectId = "";

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ scenes: true }));
});
test.afterAll(() => service.stop());

test("a watched command that ends opens its capped output at the first line, not where it was following", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 800 });
  await page.goto(`${service.page}#/new/${projectId}`);
  await send(page, "[scene:long-live]");
  await expect(page).toHaveURL(/#\/threads\//);
  // The running call is one row; open it and watch the output follow its end.
  await page.locator("button.tcr").first().click();
  const tail = page.locator(".co.tail");
  await expect(tail).toBeVisible();
  await expect.poll(() => tail.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
  // Stopping ends the call; its output is now capped.
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  const capped = page.locator(".co.cap");
  await expect(capped).toBeVisible({ timeout: 15_000 });
  expect(await capped.evaluate((element) => element.scrollTop)).toBe(0);
  await expect(capped).toContainText("line 1");
  await settled(service, threadIdOf(page));
});
