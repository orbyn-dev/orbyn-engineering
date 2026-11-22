import { afterEach, describe, expect, it, vi } from "vitest";
import { sendsPersonKey } from "../../../../api/personKey";
import { realBlocksApi } from "./blocksClient";
import type { BlockLimits, BlockOutputPage, CommandBlock } from "./blocksContract";
import { readBlockOutput } from "./readOutput";

const terminal = { threadId: "th", terminalId: "main" };
const limits: BlockLimits = { max_blocks: 500, max_block_bytes: 1_048_576, max_total_bytes: 8_388_608 };
const block: CommandBlock = {
  id: "b1", command: "git status", started_at: "2026-11-22T14:00:00Z", ended_at: "2026-11-22T14:00:01Z", exit_code: 0, cwd: "/work",
  output_bytes: 12, truncated: false, dropped_lines: 0, fullscreen: false,
};
const page: BlockOutputPage = { text: "On branch main\n", next: null, total_lines: 1, dropped_lines: 0 };
const signal = () => new AbortController().signal;
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const stub = (...answers: Array<Response | (() => Response)>) => {
  const fetching = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => {
    const next = answers.shift();
    if (next === undefined) throw new Error("Unexpected request");
    return typeof next === "function" ? next() : next;
  });
  vi.stubGlobal("fetch", fetching);
  return fetching;
};

const callAt = (fetching: ReturnType<typeof stub>, index = 0) => {
  const [url, init] = fetching.mock.calls[index]!;
  return {
    method: init?.method,
    path: String(url).replace(/^.*?(?=\/api)/, ""),
    credentials: init?.credentials,
    signal: init?.signal,
    headers: new Headers(init?.headers),
  };
};

afterEach(() => vi.unstubAllGlobals());

describe("the real command blocks client", () => {
  it("reads the newest page and an earlier page from encoded path segments and cursor, without a limit", async () => {
    const fetching = stub(
      json({ blocks: [block], next_before: "c 1", limits }),
      json({ blocks: [], next_before: null, limits }),
    );
    const newest = await realBlocksApi.list({ threadId: "th/1", terminalId: "t 2" }, {}, signal());
    expect(callAt(fetching).method).toBe("GET");
    expect(callAt(fetching).path).toBe("/api/threads/th%2F1/terminals/t%202/blocks");
    expect(newest).toEqual({ blocks: [block], next_before: "c 1", limits });

    const earlier = await realBlocksApi.list(terminal, { before: "c/1 &x" }, signal());
    expect(callAt(fetching, 1).path).toBe("/api/threads/th/terminals/main/blocks?before=c%2F1%20%26x");
    expect(callAt(fetching, 1).path).not.toContain("limit=");
    expect(earlier).toEqual({ blocks: [], next_before: null, limits });
  });

  it("reads one output page from encoded path segments and cursor", async () => {
    const fetching = stub(json(page));
    const got = await realBlocksApi.output({ threadId: "th/1", terminalId: "t 2" }, "b/1 &2", { after: "a/1 &b" }, signal());
    expect(callAt(fetching).path).toBe("/api/threads/th%2F1/terminals/t%202/blocks/b%2F1%20%262/output?after=a%2F1%20%26b");
    expect(got).toEqual(page);
  });

  it("asks for the first output page with no cursor", async () => {
    const fetching = stub(json(page));
    await realBlocksApi.output(terminal, "b1", {}, signal());
    expect(callAt(fetching).path).toBe("/api/threads/th/terminals/main/blocks/b1/output");
  });

  it("sends the session token and no person key on either Local read", async () => {
    sessionStorage.setItem("orbyn.token", "tok");
    const fetching = stub(json({ blocks: [], next_before: null, limits }));
    await realBlocksApi.list(terminal, {}, signal());
    const call = callAt(fetching);
    expect(call.credentials).toBe("same-origin");
    expect(call.headers.get("authorization")).toBe("Bearer tok");
    expect(call.headers.has("x-orbyn-person")).toBe(false);
    expect(sendsPersonKey("GET", "/api/threads/th/terminals/main/blocks")).toBe(false);
    expect(sendsPersonKey("GET", "/api/threads/th/terminals/main/blocks/b1/output")).toBe(false);
  });

  it("passes the caller's signal to the request and rejects when it aborts", async () => {
    const controller = new AbortController();
    const fetching = vi.fn((_input: RequestInfo | URL, init?: RequestInit) =>
      new Promise<Response>((_resolve, reject) => {
        if (init?.signal?.aborted) reject(new DOMException("Aborted", "AbortError"));
        init?.signal?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")));
      }),
    );
    vi.stubGlobal("fetch", fetching);
    const pending = realBlocksApi.list(terminal, {}, controller.signal);
    controller.abort();
    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
    expect(fetching).toHaveBeenCalledTimes(1);
    expect(fetching.mock.calls[0]![1]?.signal).toBe(controller.signal);
  });

  it("keeps the service's 410 code on the rejection, which is what readOutput checks", async () => {
    stub(json({ code: "cursor_expired", message: "The service dropped that text." }, 410));
    await expect(realBlocksApi.output(terminal, "b1", { after: "old" }, signal())).rejects.toMatchObject({
      name: "ApiFailure",
      code: "cursor_expired",
      status: 410,
    });
  });

  it("starts a block's output again from the top after a 410, once", async () => {
    const fetching = stub(
      json({ text: "first\n", next: "c1", total_lines: null, dropped_lines: 0 }),
      json({ code: "cursor_expired", message: "The service dropped that text." }, 410),
      json({ text: "first\n", next: null, total_lines: 1, dropped_lines: 0 }),
    );
    const output = await readBlockOutput(realBlocksApi, terminal, "b1", signal());
    expect(output).toEqual({ text: "first\n", total_lines: 1, dropped_lines: 0, capped: false });
    expect(fetching).toHaveBeenCalledTimes(3);
    expect(callAt(fetching, 1).path).toContain("?after=c1");
    expect(callAt(fetching, 2).path).not.toContain("?after=");
  });

  it("reads a 404 list as no blocks and no error, for a gone terminal or a service without the route", async () => {
    stub(new Response("Not found", { status: 404 }));
    await expect(realBlocksApi.list(terminal, {}, signal())).resolves.toEqual({
      blocks: [],
      next_before: null,
      limits: { max_blocks: 0, max_block_bytes: 0, max_total_bytes: 0 },
    });
  });

  it("surfaces a 422 bad_cursor and lets any list failure through", async () => {
    stub(json({ code: "bad_cursor", message: "That cursor is not one the service made." }, 422));
    await expect(realBlocksApi.output(terminal, "b1", { after: "nope" }, signal())).rejects.toMatchObject({
      code: "bad_cursor",
      status: 422,
    });
    stub(json({ code: "internal", message: "boom" }, 500));
    await expect(realBlocksApi.list(terminal, {}, signal())).rejects.toMatchObject({ code: "internal", status: 500 });
  });

  it("keeps the generated numbers as plain JSON numbers", async () => {
    stub(json({ blocks: [{ ...block, exit_code: 1, output_bytes: 123, dropped_lines: 7 }], next_before: null, limits }));
    const answer = await realBlocksApi.list(terminal, {}, signal());
    expect(answer.blocks[0]).toMatchObject({ exit_code: 1, output_bytes: 123, dropped_lines: 7 });
    expect(answer.limits).toEqual(limits);
    expect(typeof answer.blocks[0]!.exit_code).toBe("number");
    expect(typeof answer.blocks[0]!.output_bytes).toBe("number");
    expect(typeof answer.limits.max_total_bytes).toBe("number");
  });

  it("keeps a null total_lines for a running block and coerces the numbers in an output page", async () => {
    stub(json({ text: "still going\n", next: "2", total_lines: null, dropped_lines: 4 }));
    const got = await realBlocksApi.output(terminal, "b1", {}, signal());
    expect(got).toEqual({ text: "still going\n", next: "2", total_lines: null, dropped_lines: 4 });
    expect(typeof got.dropped_lines).toBe("number");
  });

  it("coerces bigint-typed generated numbers to the contract's plain numbers", async () => {
    const listing = new Response(null, { status: 200 });
    vi.spyOn(listing, "json").mockResolvedValue({
      blocks: [{ ...block, exit_code: 1n, output_bytes: 123n, dropped_lines: 7n }],
      next_before: null,
      limits: { max_blocks: 500n, max_block_bytes: 1_048_576n, max_total_bytes: 8_388_608n },
    });
    const output = new Response(null, { status: 200 });
    vi.spyOn(output, "json").mockResolvedValue({ ...page, total_lines: 1n, dropped_lines: 0n });
    const fetching = stub(listing, output);

    const answer = await realBlocksApi.list(terminal, {}, signal());
    expect(answer.blocks[0]).toMatchObject({ exit_code: 1, output_bytes: 123, dropped_lines: 7 });
    expect(answer.limits).toEqual(limits);
    expect([answer.blocks[0]!.exit_code, answer.blocks[0]!.output_bytes, answer.blocks[0]!.dropped_lines, answer.limits.max_total_bytes].map((value) => typeof value))
      .toEqual(["number", "number", "number", "number"]);

    const got = await realBlocksApi.output(terminal, "b1", {}, signal());
    expect(got).toEqual({ text: page.text, next: null, total_lines: 1, dropped_lines: 0 });
    expect([got.total_lines, got.dropped_lines].map((value) => typeof value)).toEqual(["number", "number"]);
    expect(fetching).toHaveBeenCalledTimes(2);
  });
});
