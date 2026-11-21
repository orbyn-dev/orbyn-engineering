import { describe, expect, it } from "vitest";
import {
  FRECENCY_MAX_IDS,
  FRECENCY_MAX_STAMPS,
  frecencyScore,
  loadFrecency,
  rankEntries,
  recordUse,
  saveFrecency,
  type Rankable,
} from "./paletteRanking";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;
const NOW = Date.UTC(2026, 10, 21, 12);

const entry = (key: string, label: string, search = "", group = "Threads"): Rankable => ({ key, group, label, search });

describe("frecency score", () => {
  it("weighs a recent use above an old one and sums repeats", () => {
    expect(frecencyScore([NOW - HOUR], NOW)).toBeGreaterThan(frecencyScore([NOW - 10 * DAY], NOW));
    expect(frecencyScore([NOW - HOUR, NOW - 2 * HOUR], NOW)).toBeGreaterThan(frecencyScore([NOW - HOUR], NOW));
    expect(frecencyScore([], NOW)).toBe(0);
  });

  it("ignores stamps from the future", () => {
    expect(frecencyScore([NOW + DAY], NOW)).toBe(0);
  });
});

describe("recordUse", () => {
  it("keeps the newest stamps per id and does not change the input", () => {
    const first = { a: [1] };
    const next = recordUse(first, "a", 2);
    expect(first).toEqual({ a: [1] });
    let book = next;
    for (let at = 3; at < 3 + FRECENCY_MAX_STAMPS + 4; at += 1) book = recordUse(book, "a", at);
    expect(book.a).toHaveLength(FRECENCY_MAX_STAMPS);
    expect(book.a?.[0]).toBe(3 + FRECENCY_MAX_STAMPS + 3);
  });

  it("drops the least recently used ids past the cap", () => {
    let book = {};
    for (let at = 0; at < FRECENCY_MAX_IDS + 5; at += 1) book = recordUse(book, `id${at}`, at + 1);
    expect(Object.keys(book)).toHaveLength(FRECENCY_MAX_IDS);
    expect(book).not.toHaveProperty("id0");
    expect(book).toHaveProperty(`id${FRECENCY_MAX_IDS + 4}`);
  });
});

describe("rankEntries", () => {
  const entries = [
    entry("t1", "Fix login redirect", "orbyn-engineering fix/login"),
    entry("t2", "Login page copy", "orbyn-planner main"),
    entry("t3", "Flaky E2E shard", "orbyn-engine login"),
  ];

  it("keeps the given order with no query and no history", () => {
    expect(rankEntries(entries, "", {}, NOW).map((one) => one.key)).toEqual(["t1", "t2", "t3"]);
  });

  it("puts a title that starts with the word before one that only contains it, then before a branch match", () => {
    const ranked = rankEntries(entries, "login", {}, NOW).map((one) => one.key);
    expect(ranked).toEqual(["t2", "t1", "t3"]);
  });

  it("lets a recent pick beat the default order among equal matches", () => {
    const ranked = rankEntries(entries, "", { t3: [NOW - HOUR] }, NOW).map((one) => one.key);
    expect(ranked[0]).toBe("t3");
  });

  it("is deterministic for a fixed clock and moves as the clock moves", () => {
    const book = { t2: [NOW - 40 * DAY, NOW - 41 * DAY, NOW - 42 * DAY], t3: [NOW - 2 * HOUR] };
    const a = rankEntries(entries, "", book, NOW).map((one) => one.key);
    expect(rankEntries(entries, "", book, NOW).map((one) => one.key)).toEqual(a);
    expect(a[0]).toBe("t3");
    // Far in the future every stamp has aged into the same bucket, so t2's three uses win.
    expect(rankEntries(entries, "", book, NOW + 400 * DAY)[0]?.key).toBe("t2");
  });

  it("never lets frecency lift a worse text match above a better one", () => {
    const ranked = rankEntries(entries, "login", { t3: [NOW - HOUR, NOW - 2 * HOUR] }, NOW).map((one) => one.key);
    expect(ranked).toEqual(["t2", "t1", "t3"]);
  });

  it("ranks inside a group and keeps the group order", () => {
    const mixed = [entry("a1", "Toggle theme", "", "Actions"), entry("p1", "Fix login", "", "Pull requests"), entry("t1", "Fix login", "", "Threads")];
    const ranked = rankEntries(mixed, "", { t1: [NOW - HOUR] }, NOW);
    expect(ranked.map((one) => one.group)).toEqual(["Actions", "Pull requests", "Threads"]);
  });
});

describe("frecency storage", () => {
  const store = (initial: Record<string, string> = {}) => {
    const data = { ...initial };
    return {
      data,
      getItem: (key: string) => data[key] ?? null,
      setItem: (key: string, value: string) => {
        data[key] = value;
      },
    };
  };

  it("round-trips ids and timestamps only", () => {
    const area = store();
    saveFrecency({ "thread:1": [5, 4] }, area);
    expect(loadFrecency(area)).toEqual({ "thread:1": [5, 4] });
    expect(Object.values(area.data).join("")).not.toMatch(/title|branch/i);
  });

  it("reads nothing from damaged or foreign data", () => {
    expect(loadFrecency(store({ "orbyn.palette.frecency": "not json" }))).toEqual({});
    expect(loadFrecency(store({ "orbyn.palette.frecency": JSON.stringify({ a: ["x", 3, null], b: "no", c: [7] }) }))).toEqual({ a: [3], c: [7] });
  });

  it("survives a store that throws", () => {
    const broken = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("full");
      },
    };
    expect(loadFrecency(broken)).toEqual({});
    expect(() => saveFrecency({ a: [1] }, broken)).not.toThrow();
  });

  it("survives no store at all", () => {
    expect(loadFrecency(null)).toEqual({});
    expect(() => saveFrecency({ a: [1] }, null)).not.toThrow();
  });
});
