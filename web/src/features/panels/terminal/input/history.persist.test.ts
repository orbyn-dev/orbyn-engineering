import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const STORE = "orbyn.terminal.history.v1";
const fresh = async () => {
  vi.resetModules();
  return import("./history");
};
const stored = () => window.localStorage.getItem(STORE);

describe("saved history", () => {
  beforeEach(() => window.localStorage.clear());
  afterEach(() => vi.restoreAllMocks());

  it("survives a reload, for the terminal and across the project", async () => {
    const first = await fresh();
    first.recordCommand("t1:main", "git status", 1);
    first.recordCommand("t1:build", "npm test", 2);
    first.recordCommand("t2:main", "other", 3);
    const second = await fresh();
    expect(second.historyOf("t1:main").map((e) => e.command)).toEqual(["git status"]);
    expect(second.historyAcross(["t1"]).map((e) => e.command)).toEqual(["git status", "npm test"]);
    second.recordCommand("t1:main", "ls", 4);
    expect(second.historyAcross(["t1"]).map((e) => e.command)).toEqual(["git status", "npm test", "ls"]);
  });

  it("stores only the key and the lines", async () => {
    const mod = await fresh();
    mod.recordCommand("t1:main", "ls", 1);
    expect(JSON.parse(stored() ?? "null")).toEqual({ v: 1, h: { "t1:main": [{ command: "ls", ok: null, at: 1 }] } });
  });

  it("never writes a secret line, and drops one found in storage", async () => {
    const mod = await fresh();
    mod.recordCommand("k", "export TOKEN=abcdef", 1);
    mod.recordCommand("k", "ls", 2);
    expect(stored()).not.toContain("TOKEN");
    window.localStorage.setItem(
      STORE,
      JSON.stringify({ v: 1, h: { k: [{ command: "export TOKEN=abcdef", ok: null, at: 1 }, { command: "pwd", ok: true, at: 2 }] } }),
    );
    const again = await fresh();
    expect(again.historyOf("k").map((e) => e.command)).toEqual(["pwd"]);
  });

  it.each(["not json", "[]", "null", '{"v":2,"h":{}}', '{"v":1,"h":[]}', '{"v":1,"h":{"k":"x"}}'])("discards %s whole", async (text) => {
    window.localStorage.setItem(STORE, text);
    const mod = await fresh();
    expect(mod.historyOf("k")).toEqual([]);
    expect(mod.historyAcross(["k"])).toEqual([]);
  });

  it("validates entries", async () => {
    const bad = [
      { command: 5, ok: null, at: 1 },
      { command: "a\u0001b", ok: null, at: 1 },
      { command: "x".repeat(4097), ok: null, at: 1 },
      { command: "ok", ok: "yes", at: 1 },
      { command: "inf", ok: null, at: null },
      { command: "good\tline", ok: false, at: 7 },
    ];
    window.localStorage.setItem(STORE, JSON.stringify({ v: 1, h: { k: bad } }));
    const mod = await fresh();
    expect(mod.historyOf("k")).toEqual([{ command: "good\tline", ok: false, at: 7 }]);
  });

  it("keeps recording when storage throws, reads or writes", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("full", "QuotaExceededError");
    });
    const mod = await fresh();
    mod.recordCommand("k", "ls", 1);
    expect(mod.historyOf("k").map((e) => e.command)).toEqual(["ls"]);
  });

  it("keeps recording when the storage object itself throws", async () => {
    const spy = vi.spyOn(window, "localStorage", "get").mockImplementation(() => {
      throw new Error("denied");
    });
    const mod = await fresh();
    mod.recordCommand("k", "ls", 1);
    expect(mod.historyOf("k")).toHaveLength(1);
    spy.mockRestore();
  });

  it("holds 500 lines per terminal, saved and loaded", async () => {
    const mod = await fresh();
    for (let i = 0; i < 520; i += 1) mod.recordCommand("k", `cmd ${i}`, i);
    const again = await fresh();
    const list = again.historyOf("k");
    expect(list).toHaveLength(500);
    expect(list[0]?.command).toBe("cmd 20");
  });

  it("holds 50 terminals, dropping the least recent", async () => {
    const mod = await fresh();
    for (let i = 0; i < 60; i += 1) mod.recordCommand(`t${i}:main`, "ls", i);
    const saved = JSON.parse(stored() ?? "{}") as { h: Record<string, unknown> };
    expect(Object.keys(saved.h)).toHaveLength(50);
    expect(saved.h["t0:main"]).toBeUndefined();
    expect(saved.h["t59:main"]).toBeDefined();
    const again = await fresh();
    expect(again.historyAcross(["t0"])).toEqual([]);
    expect(again.historyAcross(["t59"])).toHaveLength(1);
  });

  it("stays near 1 MB, dropping the oldest first", async () => {
    const mod = await fresh();
    const line = "echo " + "ab ".repeat(1000);
    for (let t = 0; t < 30; t += 1) for (let i = 0; i < 15; i += 1) mod.recordCommand(`t${t}:main`, `${line} ${i}`, t * 100 + i);
    expect((stored() ?? "").length).toBeLessThanOrEqual(1_000_000);
    const saved = JSON.parse(stored() ?? "{}") as { h: Record<string, unknown> };
    expect(saved.h["t29:main"]).toBeDefined();
    expect(saved.h["t0:main"]).toBeUndefined();
  });
});
