import { expect, type Locator, type Page } from "@playwright/test";

const chat = (page: Page) => page.getByRole("log", { name: "Thread messages" });
export const chatTop = (page: Page) => chat(page).evaluate((element) => element.scrollTop);
export const mainTop = (page: Page) => page.locator("main").first().evaluate((element) => element.scrollTop);

/** Wheels over a target to both ends and past them, as a trackpad overscroll would. */
export async function wheelBothEnds(page: Page, target: Locator) {
  await target.hover();
  for (const delta of [4000, 4000, 4000, -4000, -4000, -4000, 4000, 4000]) {
    await page.mouse.wheel(0, delta);
    await page.waitForTimeout(60);
  }
}

/** Resolves once the chat's height and position hold still for 400 ms. */
export async function chatSettled(page: Page) {
  const state = () => chat(page).evaluate((element) => `${element.scrollHeight}/${element.scrollTop}`);
  await expect(async () => {
    const before = await state();
    await page.waitForTimeout(400);
    expect(await state()).toBe(before);
  }).toPass();
}

export async function expectChatUntouched(page: Page, target: Locator) {
  // Entering a newly opened panel can finish layout and chat-anchor adjustment.
  // Measure wheel isolation only after that target and the chat have settled.
  await target.hover();
  await chatSettled(page);
  const before = await chatTop(page);
  await wheelBothEnds(page, target);
  expect(await chatTop(page)).toBe(before);
  expect(await mainTop(page)).toBe(0);
}
