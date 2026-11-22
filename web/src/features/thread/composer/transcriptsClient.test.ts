import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure, NETWORK_ERROR } from "../../../api/client";
import { PERSON_HEADER } from "../../../api/personKey";
import { TRANSCRIPT_TYPE } from "../../../lib/transcript";
import { realTranscriptsApi } from "./transcriptsClient";

const file = (name = "redirect-loop.orbyn.json") =>
  new File([JSON.stringify({ format: "orbyn-transcript", version: 1 })], name, { type: TRANSCRIPT_TYPE });

const signal = () => new AbortController().signal;

const attached = (name = "Imported transcript - Redirect loop on logout.txt", messageCount = 214) =>
  new Response(JSON.stringify({
    dry_run: false,
    attachment: { id: "a1", thread_id: "t1", name, mime: "text/plain", size: 4120, created_at: "" },
    message_count: messageCount,
    dropped: 2,
    problems: [],
  }), { status: 201 });

const refused = (status: number, code: string) =>
  new Response(JSON.stringify({ code, message: `${code} words` }), { status });

beforeEach(() => sessionStorage.setItem("orbyn.token", "tok"));
afterEach(() => vi.unstubAllGlobals());

describe("realTranscriptsApi", () => {
  it("posts the raw file to the thread's attach route with the person key, the session and no dry run", async () => {
    const fetchMock = vi.fn(async () => attached());
    vi.stubGlobal("fetch", fetchMock);
    const controller = new AbortController();
    const transcript = file();

    await expect(realTranscriptsApi.read("t1", transcript, controller.signal)).resolves.toEqual({
      id: "a1", title: "Redirect loop on logout", message_count: 214,
    });

    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("/api/threads/t1/transcripts");
    expect(init.method).toBe("POST");
    expect(init.credentials).toBe("same-origin");
    expect(init.body).toBe(transcript);
    expect(init.signal).toBe(controller.signal);
    const headers = init.headers as Record<string, string>;
    expect(headers[PERSON_HEADER]).toBe("mock-person-key");
    expect(headers.Authorization).toBe("Bearer tok");
    expect(headers["Content-Type"]).toBe(TRANSCRIPT_TYPE);
    expect(url).not.toContain("dry_run");
  });

  it("falls back to the file name when the service's label does not match", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => attached("Redirect loop on logout")));
    await expect(realTranscriptsApi.read("t1", file("redirect-loop.ORBYN.JSON"), signal())).resolves.toMatchObject({
      title: "redirect-loop",
    });
  });

  it.each([
    [422, "transcript_unreadable"],
    [422, "transcript_too_new"],
    [413, "file_too_large"],
    [403, "person_required"],
  ])("keeps the service's %s refusal code for %s", async (status, code) => {
    vi.stubGlobal("fetch", vi.fn(async () => refused(status, code)));
    const failure = await realTranscriptsApi.read("t1", file(), signal()).catch((caught: unknown) => caught);
    expect(failure).toBeInstanceOf(ApiFailure);
    expect(failure).toMatchObject({ code, status, message: `${code} words` });
  });

  it("rethrows an abort instead of turning it into a network failure", async () => {
    const aborted = new DOMException("Aborted", "AbortError");
    vi.stubGlobal("fetch", vi.fn(async () => { throw aborted; }));
    await expect(realTranscriptsApi.read("t1", file(), signal())).rejects.toBe(aborted);
  });

  it("maps a request that never reached the service to NETWORK_ERROR", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => { throw new TypeError("Failed to fetch"); }));
    await expect(realTranscriptsApi.read("t1", file(), signal())).rejects.toMatchObject({ code: NETWORK_ERROR, status: 0 });
  });

  it("refuses an answer with no attachment", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({
      dry_run: false, attachment: null, message_count: 0, dropped: 0, problems: [],
    }), { status: 201 })));
    await expect(realTranscriptsApi.read("t1", file(), signal())).rejects.toMatchObject({
      code: "transcript_unreadable", status: 422,
    });
  });
});
