import { afterEach, expect, it, vi } from "vitest";
import { sendsPersonKey } from "../../api/personKey";
import { profilesApi, setProfilesApi } from "./api";
import { emptyDraft } from "./model";

afterEach(() => { setProfilesApi(null); vi.unstubAllGlobals(); });
const answer = (status: number, body: unknown) => vi.fn(async (_url: string, _init?: RequestInit) => new Response(status === 204 ? null : JSON.stringify(body), { status, headers: { "content-type": "application/json" } }));
const sent = (fetching: ReturnType<typeof answer>) => { const [url, init] = fetching.mock.calls[0]!; return { url: String(url), method: init?.method, body: init?.body ? JSON.parse(String(init.body)) : undefined }; };

it("answers a 404 from the list as no feature, with no error", async () => {
  vi.stubGlobal("fetch", answer(404, { code: "not_found", message: "No such route." }));
  await expect(profilesApi().list()).resolves.toBeNull();
});

it("answers a body that is not a profile list as no feature", async () => {
  vi.stubGlobal("fetch", answer(200, { hello: "world" }));
  await expect(profilesApi().list()).resolves.toBeNull();
});

it("lets any other failure through, so it is not mistaken for a missing feature", async () => {
  vi.stubGlobal("fetch", answer(500, { code: "tool_failed", message: "x" }));
  await expect(profilesApi().list()).rejects.toMatchObject({ code: "tool_failed" });
});

it("reads the list and defaults the limit to 50", async () => {
  vi.stubGlobal("fetch", answer(200, { profiles: [{ id: "builtin:careful", name: "Careful" }, "junk"] }));
  const list = await profilesApi().list();
  expect(list?.profiles).toHaveLength(1);
  expect(list?.limit).toBe(50);
});

it("sends each write to its route with the typed body, and the seen revision on a change", async () => {
  const draft = { ...emptyDraft(), name: "Mine" };
  let fetching = answer(201, {});
  vi.stubGlobal("fetch", fetching);
  await profilesApi().create(draft);
  expect(sent(fetching)).toMatchObject({ method: "POST", body: draft });
  expect(sent(fetching).url).toMatch(/\/api\/profiles$/);

  fetching = answer(200, {});
  vi.stubGlobal("fetch", fetching);
  await profilesApi().update("01j/0 x", draft, 3);
  expect(sent(fetching)).toMatchObject({ method: "PATCH", body: { ...draft, seen_revision: 3 } });
  expect(sent(fetching).url).toContain("/api/profiles/01j%2F0%20x");

  fetching = answer(204, null);
  vi.stubGlobal("fetch", fetching);
  await profilesApi().remove("01j0");
  expect(sent(fetching).method).toBe("DELETE");

  fetching = answer(200, { applied: {}, ignored: [], application: {} });
  vi.stubGlobal("fetch", fetching);
  await profilesApi().applyToThread("t 1", { profile_id: "01j0", seen_revision: 2, ack_grants: ["full_access"] });
  expect(sent(fetching)).toMatchObject({ method: "PUT", body: { profile_id: "01j0", seen_revision: 2, ack_grants: ["full_access"] } });
  expect(sent(fetching).url).toContain("/api/threads/t%201/profile");

  fetching = answer(200, { applied: {}, ignored: [], application: {} });
  vi.stubGlobal("fetch", fetching);
  await profilesApi().applyToCrew("c1", { profile_id: "01j0" });
  expect(sent(fetching)).toMatchObject({ method: "POST", body: { crew_id: "c1", profile_id: "01j0" } });
});

it("treats every write as Person-class and the reads and the check as not", () => {
  expect(sendsPersonKey("POST", "/api/profiles")).toBe(true);
  expect(sendsPersonKey("PUT", "/api/threads/t1/profile")).toBe(true);
  expect(sendsPersonKey("POST", "/api/profiles/check")).toBe(false);
  expect(sendsPersonKey("GET", "/api/profiles")).toBe(false);
});
