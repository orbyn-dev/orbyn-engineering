import { expect, test, type Page } from "@playwright/test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { startProject } from "./service";

const executable = join(dirname(process.env.ORBYN_BIN ?? resolve(import.meta.dirname, "../../../target/debug/orbynd")), "orbyn-fake-engine");
const history = (kind: string, text: string) => ({ sessionUpdate: kind, content: { type: "text", text } });
const row = (sessionId: string, cwd: string, title: string) => ({ sessionId, cwd, title, history: [history("user_message_chunk", "saved question"), history("agent_message_chunk", "saved answer")] });
const stored = (native: string) => readFileSync(join(native, "sessions.json"), "utf8");
const providers = (page: Page) => page.getByRole("table", { name: "Providers" });
type ListedSession = { session_id: string; title: string | null; imported_thread_id: string | null };
type SessionPage = { sessions: ListedSession[]; next_cursor: string | null };

/**
 * A local ACP command is not one of the families with a detail page (providerDetailFamily.ts), so its
 * Settings row opens nothing and offers no session panel. The service still lists, imports and deletes
 * an agent's own sessions, and that is exercised here against the same service the app talks to.
 */
test("a local ACP agent's saved sessions are listed, imported and deleted through the service, and its row offers no session control", async ({ page }, info) => {
  test.setTimeout(120_000);
  const native = mkdtempSync(join(tmpdir(), "orbyn-native-"));
  const { service } = await startProject({ nativeSessions: native });
  try {
    const [project] = await service.api<{ id: string; path: string }[]>("GET", "/api/projects");
    writeFileSync(join(native, "sessions.json"), JSON.stringify([row("a1", project!.path, "Alpha chat"), row("b2", project!.path, "Beta chat"), row("c3", project!.path, "Gamma chat"), row("z9", "/not/this/project", "Elsewhere")]));
    await service.api("PUT", "/api/preferences", { values: { provider_instances: [{ id: "agent", driver: "_local_acp", name: "Saved agent", program: executable, args: ["acp"] }] } });
    await page.goto(`${service.page}#/settings/providers`);
    const agent = providers(page).getByRole("row").filter({ hasText: "Saved agent" });
    await expect(agent).toBeVisible({ timeout: 60_000 });
    await expect(agent).toContainText("via ACP");
    await expect(agent.getByRole("button", { name: "Edit Saved agent", exact: true })).toBeVisible();
    // Nothing in Settings opens an ACP provider's sessions: no detail page, no Models and no List sessions.
    await expect(providers(page).getByRole("button", { name: "Saved agent", exact: true })).toHaveCount(0);
    await expect(page.getByText("Models", { exact: true })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "List sessions", exact: true })).toHaveCount(0);
    await expect(page.getByText("Native sessions", { exact: true })).toHaveCount(0);
    await page.screenshot({ path: info.outputPath("native-sessions.png") });

    const capabilities = await service.api<{ can_list: boolean; can_load: boolean; can_resume: boolean; can_delete: boolean }>("GET", "/api/engines/agent/native-sessions/capabilities");
    expect(capabilities).toMatchObject({ can_list: true, can_load: true, can_delete: true });
    const query = `?project_id=${encodeURIComponent(project!.id)}`;
    const list = (cursor?: string) => service.api<SessionPage>("GET", `/api/engines/agent/native-sessions${query}${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ""}`);
    const first = await list();
    expect(first.sessions.map((session) => session.title)).toEqual(["Alpha chat", "Beta chat"]);
    expect(first.next_cursor).not.toBeNull();
    const more = await list(first.next_cursor ?? "");
    expect(more.sessions.map((session) => session.title)).toEqual(["Gamma chat"]);
    expect(more.next_cursor).toBeNull();

    const imported = await service.api<{ thread_id: string; imported: boolean }>("POST", "/api/engines/agent/native-sessions/import", { project_id: project!.id, session_id: "a1", title: "Alpha chat" });
    expect(imported.imported).toBe(true);
    const threads = await service.api<{ id: string; title: string; engine: string }[]>("GET", "/api/threads");
    expect(threads.filter((thread) => thread.engine === "agent").map((thread) => thread.title)).toEqual(["Alpha chat"]);
    expect(threads.find((thread) => thread.id === imported.thread_id)?.title).toBe("Alpha chat");
    expect((await list()).sessions.find((session) => session.session_id === "a1")?.imported_thread_id).toBe(imported.thread_id);

    await service.api("POST", "/api/engines/agent/native-sessions/delete", { project_id: project!.id, session_id: "b2" });
    expect(stored(native)).not.toContain("b2");
    expect(stored(native)).toContain("a1");
  } finally { await service.stop(); }
});

/**
 * An agent without a saved-session capability reports none, and Settings offers no session control
 * for it either: a local ACP command has no detail page to carry one.
 */
test("an agent that cannot list sessions shows no session controls", async ({ page }) => {
  test.setTimeout(120_000);
  const { service } = await startProject();
  try {
    await service.api("PUT", "/api/preferences", { values: { provider_instances: [{ id: "plain", driver: "_local_acp", name: "Plain agent", program: executable, args: ["acp"] }] } });
    await page.goto(`${service.page}#/settings/providers`);
    const agent = providers(page).getByRole("row").filter({ hasText: "Plain agent" });
    await expect(agent).toBeVisible({ timeout: 60_000 });
    await expect(agent).toContainText("via ACP");
    await expect(agent.getByRole("button", { name: "Edit Plain agent", exact: true })).toBeVisible();
    await expect(providers(page).getByRole("button", { name: "Plain agent", exact: true })).toHaveCount(0);
    await expect(page.getByText("Models", { exact: true })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "List sessions", exact: true })).toHaveCount(0);
    await expect(page.getByText("Native sessions", { exact: true })).toHaveCount(0);
    const capabilities = await service.api<{ can_list: boolean; can_load: boolean; can_resume: boolean; can_delete: boolean }>("GET", "/api/engines/plain/native-sessions/capabilities");
    expect(capabilities).toEqual({ can_list: false, can_load: false, can_resume: false, can_delete: false });
  } finally { await service.stop(); }
});
