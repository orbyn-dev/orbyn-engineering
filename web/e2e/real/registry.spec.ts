import { expect, test } from "@playwright/test";
import { createServer, type Server } from "node:http";
import { startProject, type Service } from "./service";

/** The ACP registry section, against a local registry file. */
const REGISTRY_PORT = 5287;
const CATALOG = {
  agents: [
    { id: "pal", name: "Pal Agent", version: "1.0.0", description: "Helps out", distribution: { npx: { package: "pal-agent" } } },
    { id: "bare", name: "Bare Agent", version: "2.0.0", description: "Nothing to run", distribution: {} },
  ],
};

let service: Service;
let registry: Server;

test.beforeAll(async () => {
  registry = createServer((_request, response) => {
    response.setHeader("content-type", "application/json");
    response.end(JSON.stringify(CATALOG));
  }).listen(REGISTRY_PORT);
  ({ service } = await startProject({ fakeEngines: true, registry: `http://127.0.0.1:${REGISTRY_PORT}/registry.json` }));
});

test.afterAll(async () => {
  await service.stop();
  registry.close();
});

test("an agent is found, installed as a provider, kept in its history and removed", async ({ page }) => {
  await page.goto(`${service.page}#/settings/providers`);
  await page.getByRole("button", { name: "Browse", exact: true }).click();
  const section = page.getByRole("table", { name: "ACP registry" });
  await page.getByRole("textbox", { name: "Search the registry" }).fill("pal");
  await expect(section.getByText("Pal Agent")).toBeVisible();
  await expect(section.getByText("Runs with npx · downloads when you start it")).toBeVisible();
  await section.getByRole("button", { name: "Install Pal Agent" }).click();
  await expect(section.getByRole("button", { name: "Remove Pal Agent" })).toBeVisible();
  const engines = () => service.api<{ id: string }[]>("GET", "/api/engines");
  await expect.poll(async () => (await engines()).some((engine) => engine.id === "acp-pal")).toBe(true);

  // The service keeps what the install did.
  const kept = await service.api<{ outcomes: { kind: string; ok: boolean }[] }>("GET", "/api/engines/acp-pal/installs");
  expect(kept.outcomes.map((outcome) => [outcome.kind, outcome.ok])).toEqual([["install", true]]);

  await section.getByRole("button", { name: "Remove Pal Agent" }).click();
  await page.getByRole("dialog", { name: "Remove Pal Agent?" }).getByRole("button", { name: "Remove", exact: true }).click();
  await expect(section.getByRole("button", { name: "Install Pal Agent" })).toBeVisible();
  await expect.poll(async () => (await engines()).some((engine) => engine.id === "acp-pal")).toBe(false);
  const after = await service.api<{ outcomes: { kind: string; ok: boolean }[] }>("GET", "/api/engines/acp-pal/installs");
  expect(after.outcomes.map((outcome) => outcome.kind)).toEqual(["remove", "install"]);
});
