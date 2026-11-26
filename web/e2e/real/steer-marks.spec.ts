import { expect, test, type Page } from "@playwright/test";
import type { WireThreadEvent } from "../../src/api/types";
import { send, settled, startProject, type Service } from "./service";

/**
 * B61: a mid-turn message's three settle states, through the real service and the real page.
 *
 * Real: an `orbynd` binary over its own HTTP and SSE routes, the dev UI in Chromium, and the marks the page
 * draws under the message. The steer goes out from the composer and the service's answer is the real
 * `POST /api/threads/{id}/steer` 202 body; the queue is read back from `GET .../queue` and the log straight
 * from the service's own event stream.
 *
 * Fake, and only this: the engine process. Every run below is the repository's protocol double
 * (crates/orbynd/src/bin/orbyn-fake-engine) launched as the Codex binary, except the queued case, which
 * registers the same double as a custom ACP engine (`<double> acp`) because that driver has no mid-turn
 * input. No real Codex, Claude or ACP agent runs. There is no page.route, stub, sleep or skip: every wait is
 * on the service's own events or on the page's own state.
 *
 * One settle per case:
 * - delivered: the Codex double waits inside the turn for a steer, takes it, and answers what it was handed;
 *   the service logs the message row under the page's message id and `steer.state delivered`.
 * - queued: the no-input ACP driver makes the service hold the text for the next turn; the service reports
 *   `state: queued` with the queue row it created, and that row is in the queue the page reads.
 * - failed: a Codex run held open by a running sub-agent refuses the steer; the service first says delivered,
 *   then logs `steer.failed` and `steer.state failed`, and puts the text back in the held queue, so no next
 *   turn starts and the failed mark stays readable until one does.
 */

interface SteerAnswer {
  message_id: string;
  state: "delivered" | "queued" | "failed";
  queued_message_id?: string;
}

interface QueuedRow {
  id: string;
  prompt: string;
  held: boolean;
}

interface ThreadView {
  active_run: { id: string; state: string } | null;
}

let service: Service;
let projectId = "";

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ fakeEngines: true }));
});
test.afterAll(() => service.stop());

const box = (page: Page) => page.getByRole("textbox", { name: /^Message/ });
const stop = (page: Page) => page.getByRole("button", { name: "Stop", exact: true });
/** The mark under the message whose bubble holds `text`: the message, not the queue row. */
const markOf = (page: Page, text: string) => page.locator(".msg-user").filter({ hasText: text }).getByRole("status");

/** A thread on `engine`, opened in the page, whose first run is sent from its composer and is running. */
async function threadOn(page: Page, engine: string, prompt: string): Promise<string> {
  const thread = await service.api<{ id: string }>("POST", "/api/threads", {
    project_id: projectId, engine, model: null, title: `Steer ${engine}`, worktree: null, skill_profile: null,
  });
  await page.goto(`${service.page}#/threads/${thread.id}`);
  await send(page, prompt);
  await expect(stop(page)).toBeVisible({ timeout: 15_000 });
  await expect
    .poll(async () => (await service.api<ThreadView>("GET", `/api/threads/${thread.id}`)).active_run?.state, { timeout: 15_000 })
    .toBe("running");
  return thread.id;
}

/** Types into the composer and sends into the running turn with the opposite key (the setting queues plain Enter). */
async function steer(page: Page, text: string): Promise<SteerAnswer> {
  await box(page).fill(text);
  const [response] = await Promise.all([
    page.waitForResponse(
      (answer) => answer.request().method() === "POST" && /\/api\/threads\/[^/]+\/steer$/.test(new URL(answer.url()).pathname),
    ),
    box(page).press("ControlOrMeta+Enter"),
  ]);
  expect(response.status()).toBe(202);
  return (await response.json()) as SteerAnswer;
}

/** A `CUSTOM` event's value, as the log carries it. */
function customOf(event: WireThreadEvent, name: string): Record<string, unknown> | null {
  const payload = event.event;
  if (payload.type !== "CUSTOM" || payload.name !== name) return null;
  return payload.value && typeof payload.value === "object" && !Array.isArray(payload.value)
    ? (payload.value as Record<string, unknown>)
    : null;
}

test("a steer the engine takes settles delivered, and the service logs its message row", async ({ page }) => {
  test.setTimeout(60_000);
  const id = await threadOn(page, "codex", "[hang]");
  const answer = await steer(page, "also check the signup redirect");
  expect(answer.state).toBe("delivered");
  expect(answer.queued_message_id).toBeUndefined();

  const mark = markOf(page, "also check the signup redirect");
  await expect(mark).toHaveAttribute("data-state", "delivered");
  await expect(mark).toHaveText("Delivered");

  // The service's own log: the steer's message row under the page's message id, and its one mark.
  const events = await service.events(id, (event) => {
    const state = customOf(event, "steer.state");
    return state?.message_id === answer.message_id && state.state === "delivered";
  });
  const rows = events.filter((event) => event.event.type === "TEXT_MESSAGE_CONTENT" && event.event.messageId === answer.message_id);
  expect(rows.map((event) => (event.event as { delta: string }).delta)).toEqual(["also check the signup redirect"]);
  expect(events.filter((event) => event.event.type === "TEXT_MESSAGE_CONTENT" && event.event.messageId === answer.message_id)).toHaveLength(1);
  expect(events.some((event) => customOf(event, "steer.failed") !== null)).toBe(false);
  expect(await service.api<QueuedRow[]>("GET", `/api/threads/${id}/queue`)).toEqual([]);

  // The double answers what it was handed, so the service's log proves the engine received the steer text.
  await expect(page.locator(".msg-assistant").getByText("steered: also check the signup redirect")).toBeVisible({ timeout: 15_000 });
  await settled(service, id);
  await expect(mark).toHaveAttribute("data-state", "delivered");
  await expect(mark).toHaveText("Delivered");
});

test("a steer the engine cannot take settles queued with the service's own queue row", async ({ page }) => {
  test.setTimeout(60_000);
  const engine = await service.api<{ id: string; tier: string }>("POST", "/api/engines/custom", {
    id: "no-mid-turn-input", name: "No mid-turn input", tier: "acp", target: service.fakeEngine, args: ["acp"],
  });
  expect(engine.id).toBe("no-mid-turn-input");
  expect(engine.tier).toBe("acp");
  const id = await threadOn(page, engine.id, "[hang]");

  const answer = await steer(page, "hold this for the next turn");
  expect(answer.state).toBe("queued");
  expect(answer.queued_message_id).toBeTruthy();

  const mark = markOf(page, "hold this for the next turn");
  await expect(mark).toHaveAttribute("data-state", "queued");
  await expect(mark).toHaveText("Queued for the next turn");

  // The service's queue row, named by the service's own answer, and the page's row drawn from that queue.
  const rows = await service.api<QueuedRow[]>("GET", `/api/threads/${id}/queue`);
  expect(rows.find((row) => row.id === answer.queued_message_id)).toMatchObject({
    prompt: "hold this for the next turn",
    held: false,
  });
  await expect(page.getByLabel("Queued messages").getByText("hold this for the next turn")).toBeVisible();

  const events = await service.events(id, (event) => {
    const state = customOf(event, "steer.state");
    return state?.message_id === answer.message_id && state.state === "queued";
  });
  expect(events.some((event) => customOf(event, "steer.failed") !== null)).toBe(false);
  expect(events.some((event) => event.event.type === "TEXT_MESSAGE_CONTENT" && event.event.messageId === answer.message_id)).toBe(false);
});

test("a refused steer settles failed, goes back to the held queue and stays marked", async ({ page }) => {
  test.setTimeout(90_000);
  const id = await threadOn(page, "codex", "[hang]");

  // A held queue row first: a stop holds it, and the refused steer joins that held queue, so the next turn
  // never starts and the failed mark stays readable. The held run below is a sub-agent that never ends.
  await box(page).fill("wait behind the stop");
  await box(page).press("Enter");
  await expect(page.getByLabel("Queued messages").getByText("wait behind the stop")).toBeVisible();
  await stop(page).click();
  await settled(service, id);
  await expect(page.getByText("Held after the last run stopped.")).toBeVisible();

  await send(page, "go [bg-agent-hang]");
  await expect(stop(page)).toBeVisible({ timeout: 15_000 });
  // The double's warning after the parent turn ends is the service's signal that the run is held.
  await service.events(id, (event) => customOf(event, "engine.warning") !== null);
  await expect
    .poll(async () => (await service.api<ThreadView>("GET", `/api/threads/${id}`)).active_run?.state, { timeout: 15_000 })
    .toBe("running");

  const answer = await steer(page, "refused this time");
  // The engine took it from the service; the refusal comes back from the engine afterwards.
  expect(answer.state).toBe("delivered");

  const mark = markOf(page, "refused this time");
  await expect(mark).toHaveAttribute("data-state", "failed", { timeout: 15_000 });
  await expect(mark).toHaveText("Not delivered · moved to the queue");
  // The page read the same refusal in the service's log, and the queue row it left behind is drawn.
  await expect(page.getByText("The engine refused a message mid-turn, so it moved to the queue.")).toBeVisible();
  await expect(page.getByLabel("Queued messages").getByText("refused this time")).toBeVisible();

  // The service's own log: delivered first, then the refusal and its failed mark, under one message id.
  const events = await service.events(id, (event) => {
    const state = customOf(event, "steer.state");
    return state?.message_id === answer.message_id && state.state === "failed";
  });
  const marks = events
    .map((event) => customOf(event, "steer.state"))
    .filter((state) => state?.message_id === answer.message_id)
    .map((state) => state?.state);
  expect(marks).toEqual(["delivered", "failed"]);
  const refused = events.map((event) => customOf(event, "steer.failed")).find((value) => value?.message_id === answer.message_id);
  expect(refused).toMatchObject({ message_id: answer.message_id, prompt: "refused this time" });

  // The requeued text joins the held queue, so no next turn starts and the mark is still readable after the held run ends.
  const rows = await service.api<QueuedRow[]>("GET", `/api/threads/${id}/queue`);
  expect(rows.find((row) => row.prompt === "refused this time")).toMatchObject({ held: true });
  expect(rows.find((row) => row.prompt === "wait behind the stop")).toMatchObject({ held: true });
  await settled(service, id);
  await expect(mark).toHaveAttribute("data-state", "failed");
  await expect(mark).toHaveText("Not delivered · moved to the queue");
});
