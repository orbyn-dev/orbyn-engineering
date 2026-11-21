import { beforeEach, describe, expect, it } from "vitest";
import { historyOf, maskSecrets, recordCommand, searchHistory, suggestion, type HistoryEntry } from "./history";

const at = (command: string, n = 0): HistoryEntry => ({ command, ok: true, at: n });

describe("history", () => {
  beforeEach(() => {
    for (let i = 0; i < 1; i += 1) recordCommand("reset", "x");
  });
  it("searches newest first, once per command, case-insensitively", () => {
    const list = [at("git status"), at("npm test"), at("git status"), at("git log")];
    const found = searchHistory(list, "GIT");
    expect(found.map((row) => row.entry.command)).toEqual(["git log", "git status"]);
    expect(found[0]?.match).toEqual([0, 3]);
  });
  it("lists everything for an empty query, with no match range", () => {
    expect(searchHistory([at("a"), at("b")], "").map((row) => row.shown)).toEqual(["b", "a"]);
  });
  it("masks a secret in what is listed and in what a search can reach", () => {
    const list = [at("export OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwx"), at("echo hi")];
    expect(searchHistory(list, "")[1]?.shown).toBe("export OPENAI_API_KEY=••••••");
    expect(searchHistory(list, "sk-abc")).toEqual([]);
    expect(maskSecrets("curl -H 'x' AKIAIOSFODNN7EXAMPLE")).toBe("curl -H 'x' ••••••");
  });
  it("never records a secret or a blank line", () => {
    recordCommand("k", "  ");
    recordCommand("k", "export TOKEN=abcdef");
    recordCommand("k", "ls");
    expect(historyOf("k").map((entry) => entry.command)).toEqual(["ls"]);
  });
  it("suggests the rest of the newest earlier command", () => {
    const list = [at("git status"), at("git stash")];
    expect(suggestion(list, "git sta")).toBe("sh");
    expect(suggestion(list, "git status")).toBe("");
    expect(suggestion(list, "")).toBe("");
    expect(suggestion(list, "a\nb")).toBe("");
  });
});
