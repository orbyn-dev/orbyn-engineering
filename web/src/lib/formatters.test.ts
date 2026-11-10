import { describe, expect, it } from "vitest";
import { agoWords, callDuration, compactAge, elapsed, formatBytes, formatTokens, mebibytes, relativeTime, uptime, waitLeft, workDuration } from "./format";

// These tables pin what every duration, age and size formatter writes today. The
// formatters differ on purpose (rounding, padding, units), so each keeps its own table.

const MS = [0, 400, 999, 1000, 4200, 9_950, 10_000, 41_000, 59_400, 59_600, 60_000, 62_000, 3_599_000, 3_600_000, 3_660_000, 3_723_400, 7_200_000, 90_061_000];
const START = new Date("2026-11-08T10:00:00.000Z");

const table = (format: (ms: number) => string, expected: string[]) => {
  expect(MS.map(format)).toEqual(expected);
};

describe("duration formatters", () => {
  it("elapsed: whole seconds, then m ss, then h mm", () => {
    table(
      (ms) => elapsed(START.toISOString(), START.getTime() + ms),
      ["0s", "0s", "1s", "1s", "4s", "10s", "10s", "41s", "59s", "1m 00s", "1m 00s", "1m 02s", "59m 59s", "1h 00m", "1h 01m", "1h 02m", "2h 00m", "25h 01m"],
    );
    expect(elapsed("not a date", 5)).toBe("");
  });

  it("callDuration: tenths under ten seconds, never hours", () => {
    table(callDuration, ["0.0s", "0.4s", "1.0s", "1.0s", "4.2s", "10s", "10s", "41s", "59s", "1m 00s", "1m 00s", "1m 02s", "59m 59s", "60m 00s", "61m 00s", "62m 03s", "120m 00s", "1501m 01s"]);
  });

  it("workDuration: drops zero seconds and zero minutes", () => {
    table(workDuration, ["0s", "0s", "1s", "1s", "4s", "10s", "10s", "41s", "59s", "1m", "1m", "1m 2s", "59m 59s", "1h", "1h 1m", "1h 2m", "2h", "25h 1m"]);
  });

  it("waitLeft: rounds up, and the minutes after an hour are not padded", () => {
    table(waitLeft, ["0s", "1s", "1s", "1s", "5s", "10s", "10s", "41s", "1m 00s", "1m 00s", "1m 00s", "1m 02s", "59m 59s", "1h 0m", "1h 1m", "1h 2m", "2h 0m", "25h 1m"]);
    expect(waitLeft(-5000)).toBe("0s");
  });
});

describe("age formatters", () => {
  const AGES = [...MS, 86_400_000, 172_800_000];

  it("agoWords: a sentence ending in ago, from epoch milliseconds", () => {
    expect(AGES.map((ms) => agoWords(0, ms))).toEqual([
      "just now", "just now", "just now", "just now", "just now", "just now", "just now", "just now", "just now", "just now",
      "1 min ago", "1 min ago", "59 min ago", "1 hr ago", "1 hr ago", "1 hr ago", "2 hr ago", "1 d ago", "1 d ago", "2 d ago",
    ]);
  });

  it("compactAge: the sidebar column, never in the future", () => {
    expect(AGES.map((ms) => compactAge(START.toISOString(), START.getTime() + ms))).toEqual([
      "now", "now", "now", "now", "now", "now", "now", "now", "now", "now",
      "1m", "1m", "59m", "1h", "1h", "1h", "2h", "1d", "1d", "2d",
    ]);
    expect(compactAge(START.toISOString(), START.getTime() - 5000)).toBe("now");
    expect(compactAge("not a date")).toBe("");
  });

  it("relativeTime: Just now under a minute; the wording past that follows the locale", () => {
    const at = (ms: number) => relativeTime(START.toISOString(), START.getTime() + ms);
    expect([0, 59_000, -59_000].map(at)).toEqual(["Just now", "Just now", "Just now"]);
    expect(at(120_000)).toBe(new Intl.RelativeTimeFormat(undefined, { numeric: "auto", style: "short" }).format(-2, "minute"));
    expect(at(-3_600_000)).toBe(new Intl.RelativeTimeFormat(undefined, { numeric: "auto", style: "short" }).format(1, "hour"));
    expect(relativeTime("not a date")).toBe("");
  });
});

describe("size and count formatters", () => {
  it("formatBytes: binary units, one decimal only when needed", () => {
    expect([0, 1, 1023, 1024, 1536, 1048576, 12582912, 2000000, 1e9, 1e12, 1e15, -1, Number.NaN].map(formatBytes)).toEqual([
      "0 B", "1 B", "1023 B", "1 KB", "1.5 KB", "1 MB", "12 MB", "1.9 MB", "953.7 MB", "931.3 GB", "909.5 TB", "0 B", "0 B",
    ]);
  });

  it("uptime and mebibytes: whole units, and one decimal of MiB", () => {
    expect([0, 59, 60, 3599, 3600, 7300].map(uptime)).toEqual(["0s", "59s", "1m", "59m", "1h", "2h"]);
    expect([0, 1_048_576, 1_572_864, 1_073_741_824].map(mebibytes)).toEqual(["0.0 MiB", "1.0 MiB", "1.5 MiB", "1024.0 MiB"]);
  });

  it("formatTokens: three significant figures with a K, M, B or T suffix", () => {
    expect([0, 950, 999, 1000, 1234, 79_200_000, 99_999, 100_000, 474_000_000, 5_790_000_000, 2e12].map(formatTokens)).toEqual([
      "0", "950", "999", "1K", "1.23K", "79.2M", "100K", "100K", "474M", "5.79B", "2T",
    ]);
  });
});
