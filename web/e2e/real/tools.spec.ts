import { expect, test } from "@playwright/test";
import { openDetails, settled, startProject, type Service } from "./service";

/** Tools and connectors: the thread's Tools tab and Settings, against the stand-in engines. */

let service: Service;
let projectId = "";

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ fakeEngines: true }));
});
test.afterAll(() => service.stop());
test.beforeEach(({ page }) => page.setViewportSize({ width: 1500, height: 900 }));

const thread = async (engine: string) => {
  const created = await service.api<{ id: string }>("POST", "/api/threads", { project_id: projectId, engine, title: `Tools ${engine}` });
  return created.id;
};

test("a Claude thread's Tools row opens the tab, grouped, with statuses in words and no switches", async ({ page }) => {
  const id = await thread("claude");
  await service.api("POST", `/api/threads/${id}/runs`, { prompt: "hello" });
  await settled(service, id);
  await page.goto(`${service.page}#/threads/${id}`);
  const card = await openDetails(page);
  const row = card.getByRole("button", { name: "Tools" });
  // What the run reported (our Orbyn tools, a plugin) and what the CLI listed (servers, connectors).
  await expect(row).toContainText("ready");
  await expect(row).toContainText("1 needs sign-in · 1 failed");
  await row.click();
  await expect(page.getByRole("tab", { name: "Tools" })).toHaveAttribute("aria-selected", "true");
  const mcp = page.getByRole("region", { name: "Your MCP servers" });
  await expect(mcp.getByText("github")).toBeVisible();
  await expect(mcp.getByText("Failed", { exact: true })).toBeVisible();
  await expect(mcp.getByText("Command not found: pg-mcp")).toBeVisible();
  const connectors = page.getByRole("region", { name: "Connectors" });
  await expect(connectors.getByText("Vercel")).toBeVisible();
  await expect(connectors.getByText("Needs sign-in")).toBeVisible();
  await expect(connectors.getByRole("link", { name: /Sign in/ })).toHaveAttribute("href", /claude\.ai/);
  await expect(page.getByRole("switch")).toHaveCount(0);
});

test("a thread that has not run says the row fills in once the engine has run", async ({ page }) => {
  const id = await thread("claude");
  await page.goto(`${service.page}#/threads/${id}`);
  const card = await openDetails(page);
  await expect(card.getByRole("button", { name: "Tools" })).toContainText("Shown once this engine has run");
  await card.getByRole("button", { name: "Tools" }).click();
  await expect(page.getByText("No tools reported for this run")).toBeVisible();
});

test("Settings › Tools and connectors has one tab per engine and says what Codex does not report", async ({ page }) => {
  await page.goto(`${service.page}#/settings/tools`);
  await expect(page.getByRole("tab", { name: "Claude Code" })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("region", { name: "Connectors" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Check again" })).toBeVisible();
  await page.getByRole("tab", { name: "Codex" }).click();
  await expect(page.getByText("Codex doesn’t report its built-in tools.")).toBeVisible();
  await expect(page.getByRole("region", { name: "Built-in" })).toHaveCount(0);
  await expect(page.getByRole("region", { name: "Connectors" }).getByText("GitHub")).toBeVisible();
});
