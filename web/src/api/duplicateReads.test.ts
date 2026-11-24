import { afterEach, describe, expect, it, vi } from "vitest";
import { installMockFetch } from "../test-support";
import { api } from "./client";
import { queryPreferences, readPreferences } from "./preferences";

/** The requests the stubbed network received, by method and path (with its query). */
const wire = () => vi.mocked(fetch).mock.calls.map(([url, init]) => ({ method: init?.method ?? "GET", url: String(url) }));

const count = (path: string) => wire().filter((call) => call.url === path).length;
const writes = (path: string) => wire().filter((call) => call.url === path && call.method !== "GET").length;

afterEach(() => vi.unstubAllGlobals());

/**
 * The initial load mounts several readers of one resource at once. These are the requests the
 * client used to send once per caller; each group of identical reads is now one request.
 */
describe("duplicate initial reads", () => {
  it("reads /api/info once for every caller that mounts together", async () => {
    installMockFetch();
    const answers = await Promise.all([api.info(), api.info(), api.info(), api.info()]);
    expect(count("/api/info")).toBe(1);
    for (const answer of answers) expect(answer).toEqual(answers[0]);
  });

  it("reads the thread list once for the callers that ask for it together", async () => {
    installMockFetch();
    const [all, allAgain, project, projectAgain] = await Promise.all([
      api.allThreads(),
      api.allThreads(),
      api.threads("p-app"),
      api.threads("p-app"),
    ]);
    expect(count("/api/threads")).toBe(1);
    expect(count("/api/threads?project_id=p-app")).toBe(1);
    expect(all).toEqual(allAgain);
    expect(project).toEqual(projectAgain);
  });

  it("sends one settings preference query for identical questions asked together", async () => {
    installMockFetch();
    const body = { scope: {}, keys: ["default_thread_env_mode"] };
    const answers = await Promise.all([queryPreferences(body), queryPreferences(body), queryPreferences(body)]);
    expect(count("/api/settings/preferences/query")).toBe(1);
    for (const answer of answers) expect(answer).toEqual(answers[0]);
  });

  it("keeps different bodies and the plain preference read separate", async () => {
    installMockFetch();
    const settled = (call: Promise<unknown>) => call.catch(() => undefined);
    await Promise.all([
      settled(queryPreferences({ scope: {}, keys: ["default_thread_env_mode"] })),
      settled(queryPreferences({ scope: {}, keys: ["terminal_attention_enabled"] })),
      settled(readPreferences()),
      settled(readPreferences()),
    ]);
    expect(count("/api/settings/preferences/query")).toBe(2);
    expect(count("/api/preferences")).toBe(1);
  });

  it("never shares a write with another write", async () => {
    installMockFetch();
    const settings = await api.settings();
    await Promise.all([api.putSettings(settings).catch(() => undefined), api.putSettings(settings).catch(() => undefined)]);
    expect(writes("/api/settings")).toBe(2);
  });
});
