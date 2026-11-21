(() => {
  const g = globalThis;
  const start = g.__start, call = g.__call, log = g.__log;
  delete g.__start; delete g.__call; delete g.__log;
  const show = (v) => {
    if (typeof v === "string") return v;
    try { const s = JSON.stringify(v); return s === undefined ? String(v) : s; } catch { return String(v); }
  };
  const out = (level) => (...a) => log(level, a.map(show).join(" "));
  Object.defineProperty(g, "console", { value: Object.freeze({ log: out("log"), info: out("log"), warn: out("warn"), error: out("error") }) });
  const fail = function () { throw new TypeError("Code generation is not allowed here."); };
  for (const f of [function () {}, async function () {}, function* () {}, async function* () {}]) {
    Object.defineProperty(Object.getPrototypeOf(f), "constructor", { value: fail });
  }
  g.Function = fail; g.eval = fail;
  let s = g.__seed >>> 0 || 1; delete g.__seed;
  Math.random = () => { s ^= s << 13; s >>>= 0; s ^= s >>> 17; s ^= s << 5; s >>>= 0; return s / 4294967296; };
  const D = Date;
  g.Date = new Proxy(D, {
    construct: (t, a) => (a.length ? new t(...a) : new t(start)),
    apply: () => new D(start).toString(),
    get: (t, k) => (k === "now" ? () => start : t[k]),
  });
  class ToolError extends Error { constructor(e) { super(e.message); this.name = "ToolError"; this.code = e.code; } }
  const tools = new Proxy(Object.create(null), {
    get: (_, name) => typeof name !== "string" ? undefined : async (args) => {
      const r = JSON.parse(await call(name, JSON.stringify(args === undefined ? {} : args)));
      if ("err" in r) throw new ToolError(r.err);
      return r.ok;
    },
  });
  Object.defineProperty(g, "tools", { value: tools });
})();
