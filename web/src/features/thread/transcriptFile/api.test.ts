import { afterEach, describe, expect, it, vi } from "vitest";
import { exportFileName, importTranscript, MAX_IMPORT_BYTES } from "./api";

vi.mock("../../../api/personKey", () => ({ personKeyHeaders: async () => ({ "X-Orbyn-Person": "key" }) }));
vi.mock("../../../api/origin", () => ({ serviceUrl: (path: string) => `http://svc${path}` }));

afterEach(() => vi.unstubAllGlobals());

const file = (name = "t.orbyn.json", text = "{}") => new File([text], name, { type: "application/vnd.orbyn.transcript+json" });

describe("exportFileName", () => {
  it("cleans the title and adds the ending", () => {
    expect(exportFileName('Fix: "a/b" loop?')).toBe("Fix a b loop.orbyn.json");
    expect(exportFileName("  ")).toBe("Thread.orbyn.json");
    expect(exportFileName("..hidden")).toBe("hidden.orbyn.json");
  });
});

describe("importTranscript", () => {
  it("sends the raw file with the person key, and a dry run flag", async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({ dry_run: true, thread: null, title: "T", message_count: 2, dropped: 0, problems: [] }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const answer = await importTranscript({ projectId: "p 1", engine: "claude", dryRun: true, file: file() });
    expect(answer.message_count).toBe(2);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("http://svc/api/threads/import?project_id=p+1&name=t.orbyn.json&engine=claude&dry_run=true");
    expect(init.method).toBe("POST");
    expect((init.headers as Record<string, string>)["X-Orbyn-Person"]).toBe("key");
    expect(init.body).toBeInstanceOf(File);
  });

  it("leaves out the flags it was not given", async () => {
    const fetchMock = vi.fn(async () => new Response("{}", { status: 201 }));
    vi.stubGlobal("fetch", fetchMock);
    await importTranscript({ projectId: "p", dryRun: false, file: file() });
    expect((fetchMock.mock.calls[0] as unknown as [string])[0]).toBe("http://svc/api/threads/import?project_id=p&name=t.orbyn.json");
  });

  it("refuses a file over 2 MiB before sending", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    const big = file("big.orbyn.json", "x".repeat(MAX_IMPORT_BYTES + 1));
    await expect(importTranscript({ projectId: "p", dryRun: true, file: big })).rejects.toMatchObject({ code: "file_too_large", status: 413 });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("keeps the service's code for an unreadable or too-new file", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ code: "transcript_too_new", message: "newer" }), { status: 422 })));
    await expect(importTranscript({ projectId: "p", dryRun: true, file: file() })).rejects.toMatchObject({ code: "transcript_too_new", status: 422 });
  });
});
