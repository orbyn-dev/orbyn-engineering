import { expect, test, type Locator, type Page } from "@playwright/test";
import { startProject, type Service } from "./service";

interface Pane {
  id: string;
  title: string;
  group: string;
  direction: string | null;
  alive: boolean;
  exit_code: number | null;
}

const panes = (service: Service, threadId: string) =>
  service.api<Pane[]>("GET", `/api/threads/${threadId}/terminals`);
const inGroup = async (service: Service, threadId: string, group: string) =>
  (await panes(service, threadId)).filter((pane) => pane.group === group);
const scrollback = (service: Service, threadId: string, id: string) =>
  service.api<{ text: string }>("GET", `/api/threads/${threadId}/terminals/${id}/scrollback`).then((read) => read.text);

/** Types until one of the tab's shells answers, since a shell still starting can lose what is typed. */
async function runInPane(page: Page, service: Service, threadId: string, group: string, input: Locator, command: string, output: string) {
  await expect(async () => {
    await input.focus();
    await page.keyboard.type(`${command}\n`);
    const texts = await Promise.all((await inGroup(service, threadId, group)).map((pane) => scrollback(service, threadId, pane.id)));
    expect(texts.join("\n")).toContain(output);
  }).toPass({ timeout: 25_000 });
}

/**
 * Parity row K42: the split runs from the web through a real orbynd and real PTYs, not the mock.
 * A tab of its own splits with mod+D into one tab of two live shells, each pane runs a command in
 * its own shell, and the tab's X ends both with one confirmation.
 */
test("a terminal tab splits, both shells run, and the tab closes", async ({ page }) => {
  test.setTimeout(90_000);
  page.setViewportSize({ width: 1400, height: 900 });
  const { service, projectId } = await startProject();
  try {
    const thread = await service.api<{ id: string }>("POST", "/api/threads", { project_id: projectId, engine: "codex", title: "Terminal split" });
    const opened = await service.api<Pane>("POST", `/api/threads/${thread.id}/terminals`, { title: "Split fixture" });
    expect(opened.alive).toBe(true);
    expect(opened.group).toBe(opened.id);

    await page.goto(`${service.page}#/threads/${thread.id}`);
    await expect(page.getByRole("heading", { name: "Terminal split", exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Terminal", exact: true }).click();
    const tab = page.getByRole("tab", { name: "Split fixture", exact: true });
    await expect(tab).toBeVisible();
    await tab.click();
    const firstPane = page.locator(".terminal-splits .xterm-helper-textarea");
    await expect(firstPane).toHaveCount(1);
    await runInPane(page, service, thread.id, opened.group, firstPane, "echo PANE_ONE_READY", "PANE_ONE_READY");

    // The real chord at the real terminal: the shell yields it to the app, the app asks the service
    // for a second shell in the shown tab's group, and the service starts it.
    await firstPane.focus();
    await page.keyboard.press("Control+d");
    await expect.poll(async () => (await inGroup(service, thread.id, opened.group)).length).toBe(2);
    const split = await inGroup(service, thread.id, opened.group);
    const child = split.find((pane) => pane.id !== opened.id);
    expect(split.every((pane) => pane.alive)).toBe(true);
    expect(child?.direction).toBe("horizontal");

    // One tab, two panes, and the new pane is a shell of its own: a command typed there reaches only it.
    await expect(page.getByRole("tab", { name: "Split fixture (2)", exact: true })).toHaveAttribute("aria-selected", "true");
    await expect(page.locator(".terminal-splits .term-view")).toHaveCount(2);
    const childPane = page.locator(".terminal-splits .term-view").filter({ has: page.locator(".term-view-title > span", { hasText: /^Terminal$/ }) }).locator(".xterm-helper-textarea");
    await expect(childPane).toHaveCount(1);
    await runInPane(page, service, thread.id, opened.group, childPane, "echo PANE_TWO_READY", "PANE_TWO_READY");
    expect(await scrollback(service, thread.id, opened.id)).toContain("PANE_ONE_READY");
    expect(await scrollback(service, thread.id, child!.id)).toContain("PANE_TWO_READY");
    expect(await scrollback(service, thread.id, child!.id)).not.toContain("PANE_ONE_READY");

    // The tab's X ends every pane of the split with one confirmation, and the main shell stays.
    await page.getByRole("button", { name: "Close Split fixture", exact: true }).click();
    const confirm = page.getByRole("dialog", { name: "End this tab?" });
    await expect(confirm).toBeVisible();
    await expect(confirm).toContainText("2 terminals");
    await confirm.getByRole("button", { name: "End all", exact: true }).click();
    await expect.poll(async () => (await inGroup(service, thread.id, opened.group)).length).toBe(0);
    await expect.poll(async () => (await panes(service, thread.id)).some((pane) => pane.id === "main" && pane.alive)).toBe(true);
    await expect(page.getByRole("tab", { name: "Split fixture", exact: true })).toHaveCount(0);
  } finally { await service.stop(); }
});
