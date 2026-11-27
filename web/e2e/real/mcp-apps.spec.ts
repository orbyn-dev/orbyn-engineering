import { expect, test, type FrameLocator, type Page } from "@playwright/test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { send, settled, startProject, type Service } from "./service";

/**
 * R3: the four MCP App service methods through a real orbynd and a real browser.
 *
 * Real: a real `orbynd` runs the Codex driver, which reads the app's page from the
 * engine over `mcpServer/resource/read`, stores it as a render with its own frame
 * token, and the product's host (`McpApp.tsx`) frames it in the C21 sandbox in a real
 * Chromium. The four methods (tool-call, tool-info, context PUT/DELETE and the refusal
 * of a page's `resources/read`), the policy and the audit are the service's own code.
 *
 * Fixture: the Codex app-server is this repo's `orbyn-fake-engine` protocol double, the
 * same one `crates/orbynd/tests/mcp_apps.rs` runs, and its `mcp-app-scripted` marker
 * serves the page whose own inline script drives the bridge. So the engine's answers,
 * the MCP server's tool list and the app's page are the double's, not a real Codex
 * install and a real MCP server. Tool calls are recorded by the double, so the spec can
 * read which server and tool the service actually reached.
 */

interface Launch {
  mode: string;
  args: string[];
}

const launches = (log: string): Launch[] =>
  readFileSync(log, "utf8")
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line) as Launch);

const params = (row: Launch): Record<string, unknown> => JSON.parse(row.args[0] ?? "{}") as Record<string, unknown>;

const threadLog = (page: Page) => page.getByRole("log", { name: "Thread messages" });

/** The developer instructions Codex took with the last started or resumed thread. */
function lastThreadContext(log: string): string {
  const rows = launches(log).filter((row) => row.mode === "codex-thread");
  const last = rows.at(-1);
  if (!last) throw new Error("the run started no Codex thread");
  const instructions = params(last).developerInstructions;
  return typeof instructions === "string" ? instructions : "";
}

let service: Service;
let projectId = "";
let fixture = "";
let launchLog = "";

test.beforeAll(async () => {
  // A folder holding the double's launch log and its marker files side by side.
  fixture = mkdtempSync(join(tmpdir(), "orbyn-mcp-apps-"));
  launchLog = join(fixture, "launch.ndjson");
  writeFileSync(launchLog, "");
  // The page the double serves as the app: its own inline script drives the host bridge.
  writeFileSync(join(fixture, "mcp-app-scripted"), "");
  // The tool list the double answers: three servers, the app's own one in the middle.
  writeFileSync(join(fixture, "mcp-app-multi"), "");
  ({ service, projectId } = await startProject({ fakeEngines: true, engineLaunchLog: launchLog }));
});

test.afterAll(async () => {
  await service.stop();
  rmSync(fixture, { recursive: true, force: true });
});

/** Opens a thread, runs the fake Codex turn that reports an app, and returns its live page. */
async function openApp(page: Page) {
  const thread = await service.api<{ id: string }>("POST", "/api/threads", {
    project_id: projectId,
    engine: "codex",
    model: null,
    title: "MCP app",
    worktree: null,
    skill_profile: null,
  });
  await page.goto(`${service.page}#/threads/${thread.id}`);
  await send(page, "[mcp-app]");
  await settled(service, thread.id);
  // A finished turn keeps its calls in a closed summary; opening it mounts the app card.
  await threadLog(page).locator("button.work-summary").last().click();
  const card = page.locator("figure.mcp-app").last();
  await expect(card).toBeVisible({ timeout: 15_000 });
  const frame: FrameLocator = card.frameLocator("iframe");
  await expect(frame.getByRole("button", { name: "Initialize", exact: true })).toBeVisible({ timeout: 15_000 });
  const src = await card.locator("iframe").getAttribute("src");
  if (!src) throw new Error("the app card draws no frame");
  const address = /\/renders\/([0-9a-z]+)\?render=([0-9a-f]+)$/.exec(src);
  if (!address) throw new Error(`unexpected app frame address: ${src}`);
  return { threadId: thread.id, renderId: address[1]!, frameToken: address[2]!, src, card, frame };
}

test("the page host answers ui/initialize, tool input and result, and tool-info with only the app's own tool", async ({ page }) => {
  const toolInfo = page.waitForResponse(
    (response) =>
      response.request().method() === "GET" && /\/mcp-apps\/[^/]+\/tool-info$/.test(new URL(response.url()).pathname),
  );
  const before = launches(launchLog).length;
  const app = await openApp(page);

  const info = await toolInfo;
  expect(info.status()).toBe(200);
  const descriptor = (await info.json()) as { name: string; description: string; inputSchema: unknown };
  expect(descriptor.name).toBe("show_widget");
  expect(descriptor.description).toBe("Shows a widget");
  expect(descriptor.inputSchema).toEqual({ type: "object" });
  // The double answered three servers; the host got the asked one only.
  expect(JSON.stringify(descriptor)).not.toContain("THIRD SERVER");
  expect(JSON.stringify(descriptor)).not.toContain("OTHER SERVER");
  expect(JSON.stringify(descriptor)).not.toContain("SECRET-MARKER");
  // The service asked Codex for the app's own server, by name.
  const reads = launches(launchLog).slice(before).filter((row) => row.mode === "codex-app-tools");
  expect(reads.length).toBeGreaterThan(0);
  for (const read of reads) expect(params(read).serverName).toBe("demo");
  // A spoofed query on the read changes nothing: the tool is the render's own.
  const spoofed = await service.api<{ name: string; description: string }>(
    "GET",
    `/api/threads/${app.threadId}/mcp-apps/${app.renderId}/tool-info?name=evil&description=spoofed`,
  );
  expect(spoofed.name).toBe("show_widget");
  expect(spoofed.description).toBe("Shows a widget");

  // The page asks under a spoofed host context; the answer carries the service's descriptor.
  await app.frame.getByRole("button", { name: "Initialize", exact: true }).click();
  const initialized = app.frame.locator("#init");
  await expect(initialized).toContainText("protocol 2026-01-26");
  await expect(initialized).toContainText('"name":"show_widget"');
  await expect(initialized).toContainText('"description":"Shows a widget"');
  await expect(initialized).not.toContainText("spoofed");

  // After the page says it initialized, the host sends the call's input and result.
  await app.frame.getByRole("button", { name: "Initialized", exact: true }).click();
  const notifications = app.frame.locator("#notifications");
  await expect(notifications).toContainText('input {"text":"hello"}');
  await expect(notifications).toContainText("shown: hello");
  await expect(notifications).toContainText('"structuredContent":{"text":"hello"}');

  // readResource is the driver's one-page read, not a host method: the page's own
  // resources/read, and any other request it does not serve, are refused.
  await app.frame.getByRole("button", { name: "Read the page's resource" }).click();
  await expect(app.frame.locator("#read-result")).toContainText("-32601");
  await expect(app.frame.locator("#read-result")).toContainText("resources/read");
  await app.frame.getByRole("button", { name: "Ask for a link" }).click();
  await expect(app.frame.locator("#unknown-result")).toContainText("-32601");

  // The recorded scope: only this page's own token opens it.
  const address = new URL(app.src, service.url);
  expect(address.pathname).toBe(`/api/threads/${app.threadId}/renders/${app.renderId}`);
  expect((await page.request.get(`${service.url}${address.pathname}?render=${app.frameToken}`)).status()).toBe(200);
  expect((await page.request.get(`${service.url}${address.pathname}`)).status()).toBe(401);
  expect((await page.request.get(`${service.url}${address.pathname}?render=${"0".repeat(64)}`)).status()).toBe(401);
});

test("a page's tool call reaches only the app's own server, after the person confirms it", async ({ page }) => {
  const app = await openApp(page);
  const before = launches(launchLog).length;
  const turnsBefore = launches(launchLog).filter((row) => row.mode === "codex-turn").length;

  await app.frame.getByRole("button", { name: "Call tool" }).click();
  const dialog = page.getByRole("dialog", { name: "demo wants to run show_widget" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Run tool", exact: true }).click();
  const result = app.frame.locator("#call-result");
  await expect(result).toContainText("called show_widget");
  await expect(result).toContainText('"structuredContent":{"text":"from app"}');

  // The page asked with spoofed fields; the service called the render's own server.
  const calls = launches(launchLog).slice(before).filter((row) => row.mode === "codex-app-tool");
  expect(calls).toHaveLength(1);
  const call = params(calls[0]!);
  expect(call.server).toBe("demo");
  expect(call.tool).toBe("show_widget");
  expect(call.arguments).toEqual({ text: "from app" });
  expect(JSON.stringify(call)).not.toContain("evil");
  expect(launches(launchLog).filter((row) => row.mode === "codex-turn")).toHaveLength(turnsBefore);

  // A direct request with spoofed fields is the same: the render's record decides the server.
  const answered = await service.api<{ structuredContent: unknown }>(
    "POST",
    `/api/threads/${app.threadId}/mcp-apps/${app.renderId}/tool-call`,
    { name: "show_widget", arguments: { text: "from api" }, server: "evil", tool: "evil", origin: "evil", confirmed: true },
  );
  expect(answered.structuredContent).toEqual({ text: "from api" });
  const last = launches(launchLog).slice(before).filter((row) => row.mode === "codex-app-tool").at(-1);
  const spoofed = params(last!);
  expect(spoofed.server).toBe("demo");
  expect(spoofed.tool).toBe("show_widget");
  expect(JSON.stringify(spoofed)).not.toContain("evil");
});

test("the page's note is sent with the next turn once, and Remove drops it", async ({ page }) => {
  const app = await openApp(page);
  const note = page.getByText("Your next message carries a note from demo.", { exact: true });

  const put = page.waitForResponse(
    (response) =>
      response.request().method() === "PUT" && /\/mcp-apps\/[^/]+\/context$/.test(new URL(response.url()).pathname),
  );
  await app.frame.getByRole("button", { name: "Send note" }).click();
  expect((await put).status()).toBe(204);
  await expect(app.frame.locator("#note-result")).toContainText('"result":{}');
  await expect(note).toBeVisible();

  // The next turn's developer instructions carry the note in the app's own block.
  await send(page, "carry the app note");
  await settled(service, app.threadId);
  const carried = lastThreadContext(launchLog);
  expect(carried).toContain('<mcp-app-context server="demo">');
  expect(carried).toContain("selected: blue");
  expect(carried).toContain('{"color":"blue"}');

  // A later note is dropped by the card's Remove before it can ride a turn.
  const again = page.waitForResponse(
    (response) =>
      response.request().method() === "PUT" && /\/mcp-apps\/[^/]+\/context$/.test(new URL(response.url()).pathname),
  );
  await app.frame.getByRole("button", { name: "Send note" }).click();
  expect((await again).status()).toBe(204);
  await expect(note).toBeVisible();
  const clear = page.waitForResponse(
    (response) =>
      response.request().method() === "DELETE" && new URL(response.url()).pathname.endsWith("/mcp-app-context"),
  );
  // The card's row sits under a tall frame: bring it up before the person's own Remove.
  await note.evaluate((element) => element.scrollIntoView({ block: "center" }));
  await page.getByRole("button", { name: "Remove", exact: true }).click();
  expect((await clear).status()).toBe(204);
  await expect(note).toHaveCount(0);

  await send(page, "a plain message after the note was dropped");
  await settled(service, app.threadId);
  expect(lastThreadContext(launchLog)).not.toContain("mcp-app-context");
  expect(lastThreadContext(launchLog)).not.toContain("selected: blue");
});
