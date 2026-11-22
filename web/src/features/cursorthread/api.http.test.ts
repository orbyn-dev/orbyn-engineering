import { afterEach, expect, it, vi } from "vitest";
import { sendsPersonKey } from "../../api/personKey";
import { cursorThreadApi, setCursorThreadApi } from "./api";

afterEach(() => { setCursorThreadApi(null); vi.unstubAllGlobals(); });
const answer = (status: number, body: unknown) => vi.fn(async (_url: string, _init?: RequestInit) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } }));

it("reads the notice of an engine from the encoded project and engine path, and a 404 as no notice", async () => {
  const fetching = answer(200, { engine: "cursor", project_id: "p 1", acknowledged: false, acknowledged_at: null });
  vi.stubGlobal("fetch", fetching);
  await expect(cursorThreadApi().notice("p 1", "cursor")).resolves.toMatchObject({ acknowledged: false });
  expect(String(fetching.mock.calls[0]![0])).toContain("/api/projects/p%201/engines/cursor/notice");
  vi.stubGlobal("fetch", answer(404, { code: "not_found", message: "No notice." }));
  await expect(cursorThreadApi().notice("p", "claude")).resolves.toBeNull();
});

it("lets any other notice failure through", async () => {
  vi.stubGlobal("fetch", answer(500, { code: "read_failed", message: "x" }));
  await expect(cursorThreadApi().notice("p", "cursor")).rejects.toMatchObject({ code: "read_failed" });
});

it("posts the acknowledgement for the project and engine", async () => {
  const fetching = answer(200, { engine: "cursor", project_id: "p", acknowledged: true, acknowledged_at: "2026-11-22T10:00:00Z" });
  vi.stubGlobal("fetch", fetching);
  await expect(cursorThreadApi().acknowledge("p", "cursor")).resolves.toMatchObject({ acknowledged: true });
  const [url, init] = fetching.mock.calls[0]!;
  expect(String(url)).toContain("/api/projects/p/engines/cursor/notice/acknowledge");
  expect(init?.method).toBe("POST");
});

it("sends one turn without approvals with the run body and no permission mode", async () => {
  const fetching = answer(200, { id: "r1" });
  vi.stubGlobal("fetch", fetching);
  await cursorThreadApi().sendWithoutApprovals("t 1", { prompt: "Run the tests.", attachment_ids: ["a1"] });
  const [url, init] = fetching.mock.calls[0]!;
  expect(String(url)).toContain("/api/threads/t%201/send-without-approvals");
  expect(init?.method).toBe("POST");
  const body = JSON.parse(String(init?.body));
  expect(body).toEqual({ prompt: "Run the tests.", attachment_ids: ["a1"] });
  expect(body).not.toHaveProperty("permission_mode");
});

it("treats the acknowledgement and the override as Person-class, and the notice read as not", () => {
  expect(sendsPersonKey("POST", "/api/threads/t1/send-without-approvals")).toBe(true);
  expect(sendsPersonKey("POST", "/api/projects/p/engines/cursor/notice/acknowledge")).toBe(true);
  expect(sendsPersonKey("GET", "/api/projects/p/engines/cursor/notice")).toBe(false);
});
