import { afterEach, expect, it, vi } from "vitest";
import { PERSON_HEADER } from "../../api/personKey";
import { createCredentialsApi } from "./credentialsApi";
import { KEYCHAIN_DENIED, KEYCHAIN_LOCKED } from "./contract";

const BASE = "/api/settings/engine-credentials/cursor";
const KEY = "sk-test KEY/+=?&0123456789".replace(" ", "-");
afterEach(() => vi.unstubAllGlobals());

type Call = { method: string; path: string; headers: Headers; body: string | undefined };
const status = (over: Record<string, unknown> = {}) => ({ engine: "cursor", opted_in: true, present: false, state: "not_saved", ...over });
const json = (body: unknown, code = 200) => new Response(JSON.stringify(body), { status: code, headers: { "Content-Type": "application/json" } });
const failure = (code: number, error: string, message: string) => json({ code: error, message }, code);

/** Stubs fetch with a script of answers, one per request, and records every request. */
function serve(...answers: (Response | Error)[]) {
  const calls: Call[] = [];
  const queue = [...answers];
  vi.stubGlobal("fetch", vi.fn(async (url: string, init: RequestInit) => {
    calls.push({ method: init.method ?? "GET", path: new URL(url, "http://x").pathname + new URL(url, "http://x").search, headers: new Headers(init.headers), body: init.body as string | undefined });
    const next = queue.shift();
    if (!next) throw new Error("unexpected request");
    if (next instanceof Error) throw next;
    return next;
  }));
  return calls;
}

it("reads the status with a plain GET that carries no person header, and maps each service state", async () => {
  const api = createCredentialsApi();
  for (const [state, opted, expected] of [
    ["off", false, "not_asked"], ["not_saved", true, "opted_in_no_key"], ["not_saved", false, "not_asked"], ["saved", true, "saved"], ["blocked", true, "locked"], ["unavailable", true, "denied"],
  ] as const) {
    const calls = serve(json(status({ state, opted_in: opted, present: state === "saved" })));
    const rows = await api.storage();
    expect(calls).toHaveLength(1);
    expect(calls[0]).toMatchObject({ method: "GET", path: BASE, body: undefined });
    expect(calls[0]?.headers.has(PERSON_HEADER)).toBe(false);
    expect(rows).toEqual([{ engine_id: "cursor", name: "Cursor", choice: expected }]);
  }
});

it("hides the section when the status is 404, and on any other failure", async () => {
  const api = createCredentialsApi();
  serve(failure(404, "not_found", "This engine does not keep a key."));
  expect(await api.storage()).toBeNull();
  serve(failure(503, "keychain_failed", "x"));
  await expect(api.storage()).rejects.toMatchObject({ code: "keychain_failed" });
});

it("an answer that is not a status is no row", async () => {
  const api = createCredentialsApi();
  serve(json({ catalog: {}, rows: [] }));
  await expect(api.storage()).rejects.toMatchObject({ code: "bad_answer" });
  serve(json(status({ state: "toString" })));
  await expect(api.storage()).rejects.toMatchObject({ code: "bad_answer" });
});

it("saving a key opts in first (once), then sends the key in the PUT body only, both with the person header", async () => {
  const api = createCredentialsApi();
  serve(json(status({ state: "off", opted_in: false })));
  await api.storage();
  const calls = serve(json(status()), json(status({ present: true, state: "saved" })));
  const row = await api.saveKey("cursor", KEY);
  expect(row).toMatchObject({ engine_id: "cursor", choice: "saved" });
  expect(calls.map((call) => `${call.method} ${call.path}`)).toEqual([`PUT ${BASE}`, `PUT ${BASE}/key`]);
  expect(calls[0]?.body).toBe(JSON.stringify({ opted_in: true }));
  expect(calls[1]?.body).toBe(JSON.stringify({ key: KEY }));
  for (const call of calls) {
    expect(call.headers.get(PERSON_HEADER)).toBe("mock-person-key");
    expect(call.path).not.toContain(KEY);
    expect(call.path).not.toContain(encodeURIComponent(KEY));
  }
  expect(calls[0]?.body).not.toContain(KEY);
  // Already opted in: the next save is one write.
  const again = serve(json(status({ present: true, state: "saved" })));
  await api.saveKey("cursor", KEY);
  expect(again.map((call) => `${call.method} ${call.path}`)).toEqual([`PUT ${BASE}/key`]);
});

it("a blocked or unavailable keychain rejects with the screen's codes, and nothing carries the key", async () => {
  const api = createCredentialsApi();
  serve(json(status()));
  await api.storage();
  serve(failure(409, "keychain_blocked", "The keychain refused for now."));
  await expect(api.saveKey("cursor", KEY)).rejects.toMatchObject({ code: KEYCHAIN_LOCKED, status: 409 });
  serve(failure(503, "keychain_unavailable", "There is no usable keychain on this computer."));
  await expect(api.saveKey("cursor", KEY)).rejects.toMatchObject({ code: KEYCHAIN_DENIED, status: 503 });
});

it("scrubs the key, and its encoded forms, from any error text before it leaves the client", async () => {
  const api = createCredentialsApi();
  serve(json(status()));
  await api.storage();
  const forms = [KEY, encodeURIComponent(KEY), JSON.stringify(KEY).slice(1, -1), btoa(KEY)];
  for (const form of forms) {
    serve(failure(400, "invalid_key", `Bad key ${form} here`));
    const error = await api.saveKey("cursor", KEY).catch((caught: unknown) => caught as Error);
    expect(error).toBeInstanceOf(Error);
    for (const leaked of forms) expect((error as Error).message).not.toContain(leaked);
    expect(JSON.stringify(error)).not.toContain(KEY);
  }
  serve(new Error(`socket closed for ${KEY}`));
  const network = (await api.saveKey("cursor", KEY).catch((caught: unknown) => caught)) as Error;
  expect(network.message).not.toContain(KEY);
});

it("keeps the service's code and the key out of the message when a save is refused at 409, 503 or 400", async () => {
  const api = createCredentialsApi();
  serve(json(status()));
  await api.storage();
  const sentinel = "sk-test-SENTINEL-9f3";
  for (const [status, code] of [[409, "opt_in_off"], [503, "keychain_failed"], [400, "invalid_key"]] as const) {
    serve(failure(status, code, `The service refused ${sentinel} while saving it.`));
    const caught = (await api.saveKey("cursor", sentinel).catch((thrown: unknown) => thrown)) as Error & { code: string; status: number };
    expect(caught).toBeInstanceOf(Error);
    expect(caught).toMatchObject({ code, status });
    expect(caught.message).toBe("The service refused [removed] while saving it.");
    expect(caught.message).not.toContain(sentinel);
  }
});

it("declining is PUT opted_in false with the person header, and a later read says declined", async () => {
  const api = createCredentialsApi();
  const calls = serve(json(status({ state: "off", opted_in: false })));
  const row = await api.decline("cursor");
  expect(calls).toHaveLength(1);
  expect(calls[0]).toMatchObject({ method: "PUT", path: BASE, body: JSON.stringify({ opted_in: false }) });
  expect(calls[0]?.headers.get(PERSON_HEADER)).toBe("mock-person-key");
  expect(row).toMatchObject({ choice: "declined" });
  serve(json(status({ state: "off", opted_in: false })));
  expect(await api.storage()).toEqual([{ engine_id: "cursor", name: "Cursor", choice: "declined" }]);
});

it("removing is DELETE .../key with the person header and no body, and reports how many threads it stopped", async () => {
  const api = createCredentialsApi();
  const calls = serve(json({ ...status(), stopped: 3 }));
  const row = await api.removeKey("cursor");
  expect(calls).toHaveLength(1);
  expect(calls[0]).toMatchObject({ method: "DELETE", path: `${BASE}/key`, body: undefined });
  expect(calls[0]?.headers.get(PERSON_HEADER)).toBe("mock-person-key");
  expect(row).toMatchObject({ engine_id: "cursor", choice: "opted_in_no_key", stopped: 3 });
});

it("retry is POST .../retry with the person header and answers the status", async () => {
  const api = createCredentialsApi();
  const calls = serve(json(status({ present: true, state: "saved" })));
  const row = await api.retryKeychain("cursor");
  expect(calls[0]).toMatchObject({ method: "POST", path: `${BASE}/retry`, body: undefined });
  expect(calls[0]?.headers.get(PERSON_HEADER)).toBe("mock-person-key");
  expect(row.choice).toBe("saved");
});

it("an engine id the service did not choose is encoded into the path and never read as a key of another object", async () => {
  const api = createCredentialsApi();
  const calls = serve(json(status()));
  await api.removeKey("a/b").catch(() => undefined);
  expect(calls[0]?.path).toBe("/api/settings/engine-credentials/a%2Fb/key");
  serve(json(status()));
  await expect(api.decline("__proto__")).resolves.toMatchObject({ engine_id: "__proto__" });
});

it("opted in with no key (read after a reload) retries with the key PUT only, and Remove opt-in sends opted_in false, both with the person header", async () => {
  const api = createCredentialsApi();
  serve(json(status()));
  expect(await api.storage()).toEqual([{ engine_id: "cursor", name: "Cursor", choice: "opted_in_no_key" }]);
  const retry = serve(json(status({ present: true, state: "saved" })));
  expect(await api.saveKey("cursor", KEY)).toMatchObject({ choice: "saved" });
  expect(retry.map((call) => `${call.method} ${call.path}`)).toEqual([`PUT ${BASE}/key`]);
  expect(retry[0]?.headers.get(PERSON_HEADER)).toBe("mock-person-key");
  const out = serve(json(status({ state: "off", opted_in: false })));
  expect(await api.removeOptIn("cursor")).toMatchObject({ choice: "not_asked" });
  expect(out.map((call) => `${call.method} ${call.path}`)).toEqual([`PUT ${BASE}`]);
  expect(out[0]?.body).toBe(JSON.stringify({ opted_in: false }));
  expect(out[0]?.headers.get(PERSON_HEADER)).toBe("mock-person-key");
});
