import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ToolItem } from "../../product/timeline/types";
import { CodeDescribeBlock, CodeSearchBlock, declarationsOf, hitsOf } from "./LookupBlocks";

const item = (name: string, args: unknown, result: unknown): ToolItem => ({
  kind: "tool", key: "k", callId: "c", seq: 1, runId: "r", ts: "2026-11-21T10:00:00.000Z", endedAt: "2026-11-21T10:00:00.100Z",
  name, args: JSON.stringify(args), output: "", result: typeof result === "string" ? result : JSON.stringify(result), done: true, exitCode: null,
});

describe("orbyn_code_search", () => {
  it("lists the ranked paths with their one-line descriptions, and the question's arguments", () => {
    const reply = { results: [{ path: "orbyn_thread_send", description: "Queue a message on a thread." }, { path: "orbyn_thread_search", description: "Threads in this project matching every word." }] };
    render(<CodeSearchBlock item={item("mcp__orbyn_engineering__orbyn_code_search", { query: "send a message to a thread", detail: "summary", limit: 10 }, reply)} ms={100} inspect={<button type="button">Raw result</button>} />);
    expect(screen.getByText("2 tools")).toBeInTheDocument();
    expect(screen.getByText("orbyn_thread_send")).toBeInTheDocument();
    expect(screen.getByText("Queue a message on a thread.")).toBeInTheDocument();
    expect(screen.getByText("send a message to a thread")).toBeInTheDocument();
    expect(screen.getByText(/Read-only tool · 0\.1s/)).toBeInTheDocument();
  });

  it("says nothing matched, rather than showing an error", () => {
    render(<CodeSearchBlock item={item("x", { query: "q" }, [])} ms={null} inspect={null} />);
    expect(screen.getByText("0 tools")).toBeInTheDocument();
    expect(screen.getByText("No tool matches that.")).toBeInTheDocument();
  });

  it("shows text it cannot read as plain text", () => {
    render(<CodeSearchBlock item={item("x", { query: "q" }, "<b>not json</b>")} ms={null} inspect={null} />);
    expect(screen.getByText("<b>not json</b>")).toBeInTheDocument();
    expect(document.querySelector("b")).not.toBeNull();
    expect(document.querySelector(".cb b")?.closest("pre")).toBeNull();
  });

  it("reads a list of bare strings", () => {
    expect(hitsOf('["a","b"]')).toEqual([{ path: "a", line: null }, { path: "b", line: null }]);
    expect(hitsOf("nope")).toBeNull();
  });
});

describe("orbyn_code_describe", () => {
  it("shows declarations as code and unknown paths under Not available", () => {
    const reply = { declarations: ["declare function orbyn_thread_send(args: { thread_id: string }): Promise<unknown>;"], unknown: ["orbyn_thread_sned"] };
    render(<CodeDescribeBlock item={item("x", { paths: ["orbyn_thread_send", "orbyn_thread_sned"] }, reply)} ms={100} inspect={null} />);
    expect(screen.getByText(/declare function orbyn_thread_send/)).toBeInTheDocument();
    expect(screen.getByText("Not available · 1")).toBeInTheDocument();
    expect(screen.getByText("orbyn_thread_sned", { selector: ".co .mono" })).toBeInTheDocument();
    expect(screen.getByText("orbyn_thread_send, orbyn_thread_sned")).toBeInTheDocument();
  });

  it("reads one declaration string and nothing else as nothing", () => {
    expect(declarationsOf('{"declarations":"declare x"}')).toEqual({ code: "declare x", unknown: [] });
    expect(declarationsOf("[]")).toBeNull();
  });
});
