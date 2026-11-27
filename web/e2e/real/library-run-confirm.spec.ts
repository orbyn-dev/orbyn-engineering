import { expect, test } from "@playwright/test";
import { openPanelTab, startProject, type Service } from "./service";

/**
 * The library run's confirm sheet against a real orbynd. #1064 said the library's `409 confirm_required` had never run
 * against a live service. The first `POST /api/library/workflows/{id}/run` answers `409` with the grants the run needs
 * and, for each env set, the name and the key names it would inject. The sheet shows exactly that: Cancel sends nothing
 * and starts no terminal, and Run anyway repeats the same run with the grants the sheet showed and starts one terminal.
 * The run is a workflow beside one env set, so the grant is `env_values` and no engine turn is needed.
 */
let service: Service;
let threadId = "";

interface RunBody {
  values: Record<string, string>;
  thread_id?: string;
  ack_grants: string[];
}

interface RunRefusal {
  grants: string[];
  env_sets: { name: string; keys: string[] }[];
}

test.beforeAll(async () => {
  const started = await startProject({ fakeEngines: true });
  service = started.service;
  // Seeded through the service's own routes, the same ones the panel reads.
  const set = await service.api<{ id: string }>("POST", "/api/library/env-sets", {
    name: "Staging",
    description: "",
    keys: ["API_TOKEN"],
  });
  await service.api("POST", "/api/library/workflows", {
    name: "Deploy",
    description: "",
    command: "echo deployed",
    params: [],
    env_set_ids: [set.id],
  });
  // A run needs the thread whose checkout it starts in; a fake engine is enough to have one.
  const thread = await service.api<{ id: string }>("POST", "/api/threads", {
    project_id: started.projectId,
    engine: started.engines[0]?.id ?? "claude",
    model: null,
    title: "Library run",
    worktree: null,
    skill_profile: null,
  });
  threadId = thread.id;
});

test.afterAll(async () => {
  await service?.stop();
});

test("the sheet shows the grants and env sets the service named, and only Run anyway starts the run", async ({ page }) => {
  // The first load compiles the dev server, and the flow asks the service three times.
  test.setTimeout(90_000);
  const asked: RunBody[] = [];
  const refused: RunRefusal[] = [];
  const isRun = (url: string) => /\/api\/library\/workflows\/[^/]+\/run$/.test(new URL(url).pathname);
  page.on("request", (request) => {
    if (request.method() === "POST" && isRun(request.url())) asked.push(request.postDataJSON() as RunBody);
  });
  page.on("response", (response) => {
    if (response.request().method() !== "POST" || !isRun(response.url()) || response.status() !== 409) return;
    void response
      .json()
      .then((body: { detail?: RunRefusal }) => refused.push(body.detail ?? { grants: [], env_sets: [] }))
      .catch(() => undefined);
  });
  const terminals = async () => (await service.api<unknown[]>("GET", `/api/threads/${threadId}/terminals`)).length;

  await page.goto(`${service.page}#/threads/${threadId}`);
  const panel = await openPanelTab(page, "Library");
  await panel.getByRole("tab", { name: /^Personal/ }).click();
  await panel.getByRole("button", { name: /Deploy/ }).click();
  await panel.getByRole("button", { name: /^Run\u2026$/ }).click();

  // The Use form, then its Run: the service refuses with the grants, which the sheet shows.
  const use = page.getByRole("dialog", { name: /^Run .Deploy.$/ });
  await expect(use).toBeVisible();
  const before = await terminals();
  await use.getByRole("button", { name: "Run", exact: true }).click();
  const sheet = page.getByRole("dialog", { name: /^Run .Deploy.\?$/ });
  await expect(sheet).toBeVisible();
  await expect(sheet).toContainText("This run needs your confirmation:");
  // The service's own words for `env_values`, then the names only: the set, and the key it would inject.
  await expect(sheet).toContainText("The saved values of these env sets go into the command");
  await expect(sheet).toContainText("Env sets applied:");
  await expect(sheet).toContainText("Staging");
  await expect(sheet).toContainText("API_TOKEN");
  await expect.poll(() => refused.length).toBe(1);
  expect(refused[0]).toEqual({ grants: ["env_values"], env_sets: [{ name: "Staging", keys: ["API_TOKEN"] }] });
  expect(asked).toHaveLength(1);
  expect(asked[0]?.ack_grants).toEqual([]);
  expect(asked[0]?.thread_id).toBe(threadId);
  expect(await terminals()).toBe(before);

  // Cancel answers a sheet, not a run: nothing is asked again and nothing starts.
  await sheet.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(sheet).toHaveCount(0);
  await expect(use).toBeVisible();
  expect(asked).toHaveLength(1);
  expect(await terminals()).toBe(before);

  // Asking again is refused again: the service records no acknowledgement of its own for a cancelled sheet.
  await use.getByRole("button", { name: "Run", exact: true }).click();
  await expect(sheet).toBeVisible();
  await expect.poll(() => asked.length).toBe(2);
  await expect.poll(() => refused.length).toBe(2);
  expect(refused[1]).toEqual(refused[0]);
  expect(asked[1]?.ack_grants).toEqual([]);
  expect(await terminals()).toBe(before);

  // Run anyway hands back exactly the grants it showed, and that run starts.
  await sheet.getByRole("button", { name: "Run anyway", exact: true }).click();
  await expect(sheet).toHaveCount(0);
  await expect(use).toContainText("Started in a terminal. Its output is there.");
  await expect.poll(() => asked.length).toBe(3);
  expect(asked[2]?.ack_grants).toEqual(["env_values"]);
  expect(asked[2]?.thread_id).toBe(threadId);
  await expect.poll(terminals).toBe(before + 1);
});
