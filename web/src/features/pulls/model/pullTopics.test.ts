import { describe, expect, it } from "vitest";
import { projectPullsChanged, parsePullTopic, pullChanged } from "./pullTopics";

describe("pull topics", () => {
  it("reads the project and number apart, even when the project id holds a colon", () => {
    expect(parsePullTopic("pull:p1:42")).toEqual({ project: "p1", number: 42 });
    expect(parsePullTopic("pull:a:b:7")).toEqual({ project: "a:b", number: 7 });
    expect(parsePullTopic("pull:p1:x")).toBeNull();
    expect(parsePullTopic("pulls")).toBeNull();
    expect(parsePullTopic("pull:3")).toBeNull();
  });

  it("tells a project `a` from a project `a:b`", () => {
    expect(projectPullsChanged("a")("pull:a:b:7")).toBe(false);
    expect(projectPullsChanged("a:b")("pull:a:b:7")).toBe(true);
    expect(pullChanged("a", 7)("pull:a:b:7")).toBe(false);
  });

  it("lets a project-wide change reach every open pull request, and its own topic reach only it", () => {
    expect(pullChanged("p", 3)("pulls")).toBe(true);
    expect(pullChanged("p", 3)("pull:p:3")).toBe(true);
    expect(pullChanged("p", 3)("pull:p:4")).toBe(false);
    expect(pullChanged("p", 3)("pull:q:3")).toBe(false);
  });

  it("reads a stack again for any pull request of its project", () => {
    expect(projectPullsChanged("p")("pull:p:9")).toBe(true);
    expect(projectPullsChanged("p")("pull:q:9")).toBe(false);
    expect(projectPullsChanged("p")("pulls")).toBe(true);
  });
});
