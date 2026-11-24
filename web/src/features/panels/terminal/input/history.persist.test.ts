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

  it("keeps how a command ended across a reload, for the line the shell reported", async () => {
    const first = await fresh();
    first.recordCommand("k:main", "npm test", 1);
    first.recordCommand("k:main", "npm test", 2);
    first.recordExitCode("k:main", "npm test", 1);
    expect(JSON.parse(stored() ?? "null").h["k:main"][0].ok).toBe(false);
    const again = await fresh();
    expect(again.historyOf("k:main").map((entry) => entry.ok)).toEqual([false, null]);
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

  it("drops a literal fallback found in storage, at load", async () => {
    window.localStorage.setItem(
      STORE,
      JSON.stringify({ v: 1, h: { k: [{ command: "API_TOKEN=${TOKEN:-storedhunter2}", ok: null, at: 1 }, { command: "pwd", ok: true, at: 2 }] } }),
    );
    const again = await fresh();
    expect(again.historyOf("k").map((e) => e.command)).toEqual(["pwd"]);
  });

  it.each([
    ["export API_TOKEN=abc123", "abc123"],
    ["OPENAI_API_KEY=sk-abcdefghijklmnop1234 run-tool", "sk-abcdefghijklmnop1234"],
    ["curl -H 'Authorization: Bearer abcdefghijklmnop1234' https://x.test", "abcdefghijklmnop1234"],
    ["git push https://ghp_abcdefghijklmnopqrstuvwxyz0123@github.com/o/r.git", "ghp_abcdefghijklmnopqrstuvwxyz0123"],
    ["aws configure set aws_access_key_id AKIAABCDEFGHIJKLMNOP", "AKIAABCDEFGHIJKLMNOP"],
    ["export API_TOKEN=${TOKEN:-fallbackhunter2}", "fallbackhunter2"],
    ["DB_PASS=${PASS:=fallbacks3cret}", "fallbacks3cret"],
    ["export API_TOKEN=\"${TOKEN:-fallback a b}\"", "fallback a b"],
    ["export API_TOKEN=$'ansihunter2'", "ansihunter2"],
    ["export API_TOKEN=$\"localehunter2\"", "localehunter2"],
    ["TOKEN=${1:-positionalhunter2}", "positionalhunter2"],
    ["export DB_PASSWORD=\"quotedhunter2\"", "quotedhunter2"],
  ])("keeps the secret in %s out of local storage while a harmless line beside it is kept", async (line, secret) => {
    const mod = await fresh();
    mod.recordCommand("k", line, 1);
    mod.recordCommand("k", "ls", 2);
    expect(stored()).not.toContain(secret);
    expect(mod.historyOf("k").map((e) => e.command)).toEqual(["ls"]);
  });

  it.each(["tool --password hunter2", "mysql -u root -pSecretValue db", "curl https://user:pass@host.test/x", "mysql --password $'hunter2'", "curl --pass ${PASS:-hunter2}", "curl --pass ${PASS:=hunter2}"])(
    "keeps %s out of local storage",
    async (line) => {
      const mod = await fresh();
      mod.recordCommand("k", line, 1);
      mod.recordCommand("k", "ls", 2);
      expect(stored()).not.toMatch(/hunter2|SecretValue|user:pass/);
      expect(mod.historyOf("k").map((e) => e.command)).toEqual(["ls"]);
    },
  );

  it.each(["mkdir -p out/a", "docker run -p 8080:80 app", "tool --password-file ./pw.txt", "git push https://github.com/o/r.git", "mysql -u root -p", "ssh -p 22 host", "curl 'https://host.test?contact=team:x@example.test'", "curl https://host.test/#a:b@c", "tool --password $HOME_PASSWORD", "tool --password \"$HOME_PASSWORD\"", "tool --token=${API_TOKEN}", "TOKEN=$(gh auth token)", "TOKEN=${VAR}", "TOKEN=${VAR:-$OTHER}", "PATH=${PATH:-/usr/local/bin}", "API_TOKEN=${TOKEN:-`gh auth token`}", "API_TOKEN=\"${TOKEN:-`gh auth token`}\"", "TOKEN=\"${VAR}\"", "TOKEN=\"$(gh auth token)\""])(
    "still keeps the ordinary line %s",
    async (line) => {
      const mod = await fresh();
      mod.recordCommand("k", line, 1);
      expect(mod.historyOf("k").map((e) => e.command)).toEqual([line]);
    },
  );

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
  }, 30_000);

  it("keeps a multi-line command, newlines and all, across a reload", async () => {
    const first = await fresh();
    const command = "for f in *.ts; do\n  echo $f\ndone";
    first.recordCommand("k:main", command, 1);
    expect(JSON.parse(stored() ?? "null").h["k:main"][0].command).toBe(command);
    const again = await fresh();
    expect(again.historyOf("k:main").map((e) => e.command)).toEqual([command]);
    expect(again.historyAcross(["k"]).map((e) => e.command)).toEqual([command]);
  });

  it("drops the whole entry when a later line is a secret, at write and at load", async () => {
    const mod = await fresh();
    mod.recordCommand("k", "echo start\nexport TOKEN=abcdef\necho end", 1);
    mod.recordCommand("k", "echo a\necho b", 2);
    expect(mod.historyOf("k").map((e) => e.command)).toEqual(["echo a\necho b"]);
    expect(stored()).not.toContain("TOKEN");
    window.localStorage.setItem(
      STORE,
      JSON.stringify({ v: 1, h: { k: [{ command: "echo start\nexport TOKEN=abcdef\necho end", ok: null, at: 1 }, { command: "pwd\nls", ok: null, at: 2 }] } }),
    );
    const again = await fresh();
    expect(again.historyOf("k").map((e) => e.command)).toEqual(["pwd\nls"]);
  });

  it("still refuses a carriage return and every other control character but tab, at write and at load", async () => {
    const mod = await fresh();
    for (const bad of ["a\rb", "a\u001bb", "a\u0000b", "a\u007fb", "a\nb\u0007"]) mod.recordCommand("k", bad, 1);
    mod.recordCommand("k", "a\tb\nc", 2);
    const saved = JSON.parse(stored() ?? "null") as { h: Record<string, { command: string }[]> };
    expect(saved.h.k?.map((e) => e.command)).toEqual(["a\tb\nc"]);
    const entries = ["a\rb", "a\u001bb", "a\u0000b", "a\u007fb", "a\u000bb", "a\u000cb", "ok\nline"].map((command, at) => ({ command, ok: null, at }));
    window.localStorage.setItem(STORE, JSON.stringify({ v: 1, h: { k: entries } }));
    const again = await fresh();
    expect(again.historyOf("k").map((e) => e.command)).toEqual(["ok\nline"]);
  });

  it("keeps the 4096 cap on a multi-line entry", async () => {
    const line = (n: number) => `${"x".repeat(n - 1)}\n`;
    const edge = line(2048) + "y".repeat(2048);
    expect(edge).toHaveLength(4096);
    const mod = await fresh();
    mod.recordCommand("k", edge, 1);
    mod.recordCommand("k", `${edge}z`, 2);
    const again = await fresh();
    expect(again.historyOf("k").map((e) => e.command)).toEqual([edge]);
  });
});
