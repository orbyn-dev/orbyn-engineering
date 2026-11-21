import { expect, test, type Page } from "@playwright/test";
import { chatTop, expectChatUntouched } from "./real/panel-scroll";

async function fixture(page: Page) {
  await page.setContent(`
    <style>
      body { margin: 0; }
      main { height: 300px; overflow: auto; }
      [role=log] { height: 200px; width: 300px; overflow: auto; }
      [data-target] { position: fixed; top: 20px; right: 20px; width: 200px; height: 100px; }
    </style>
    <main><div role="log" aria-label="Thread messages"><div style="height:3000px">Messages</div></div></main>
    <aside data-target>Panel</aside>
  `);
  await page.getByRole("log").evaluate((element) => { element.scrollTop = 600; });
  return page.locator("[data-target]");
}

test("target preparation completes before the wheel baseline is taken", async ({ page }) => {
  const target = await fixture(page);
  // Model a pending layout/anchor adjustment completed as the target is entered.
  // No wheel event changes the chat in this case.
  await target.evaluate((element) => element.addEventListener("mouseenter", () => {
    document.querySelector<HTMLElement>("[role=log]")!.scrollTop = 612;
  }, { once: true }));
  await expectChatUntouched(page, target);
  expect(await chatTop(page)).toBe(612);
});

test("a wheel that leaks into chat still fails the isolation check", async ({ page }) => {
  const target = await fixture(page);
  await target.evaluate((element) => element.addEventListener("wheel", () => {
    document.querySelector<HTMLElement>("[role=log]")!.scrollTop += 100;
  }));
  await expect(expectChatUntouched(page, target)).rejects.toThrow();
  expect(await chatTop(page)).toBeGreaterThan(600);
});
