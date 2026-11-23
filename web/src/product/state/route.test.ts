import { act } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { closeSettings, navigate, parseRoute, routeHref, type Route } from "./route";

describe("routes", () => {
  it.each<Route>([
    { name: "home" },
    { name: "tasks" },
    { name: "welcome" },
    { name: "crew", tab: "flow", task: `sha256:${"a".repeat(64)}` },
    { name: "crew", tab: "flow", task: "task_1-a.b" },
    { name: "skills" },
    { name: "skills", skill: "a/b x?#" },
    { name: "pulls" },
    { name: "pulls", project: "p/1 x", number: 42 },
    { name: "issues" },
    { name: "issues", tracker: "github", scope: "orbyn-dev/orbyn engineering", key: "ORB 87/#1" },
    { name: "thread", id: "t/1 x" },
    { name: "new", projectId: null },
    { name: "new", projectId: "p1" },
    { name: "trust", projectId: "p", threadId: null },
    { name: "trust", projectId: "p", threadId: "t" },
    { name: "settings", section: null },
    { name: "settings", section: "projects" },
    { name: "settings", section: "appearance", sub: "theme-editor" },
    { name: "settings", section: "chat", row: "search-engine" },
    { name: "settings", section: "threads", machine: "remote/missing" },
    { name: "settings", section: "worktrees", selection: { kind: "project_group", group_key: "group:/one?x" } },
    { name: "settings", section: "threads", selection: { kind: "checkout", group_key: "group:/one", project_id: "id/2" } },
    { name: "settings", section: "tasks", task: "task/1 x" },
    { name: "settings", section: "crew", crewPage: "areas" },
    { name: "settings", section: "crew", crewPage: "areas", repo: "p/app" },
    { name: "settings", section: "crew", crewPage: "thresholds", repo: "p-app" },
    { name: "settings", section: "crew", crewPage: "capacity" },
    { name: "settings", section: "crew", crewPage: "coverage" },
    { name: "settings", section: "crew", crewPage: "coverage", repo: "p/app" },
    { name: "settings", section: "crew", crewPage: "coverage", repo: "p/app", from: "capacity" },
    { name: "settings", section: "crew", crewPage: "coverage", from: "escapes" },
    { name: "settings", section: "crew", crewPage: "capacity", repo: "p/app" },
    { name: "settings", section: "crew", crewPage: "dashboard" },
    { name: "settings", section: "crew", crewPage: "dashboard", repo: "p/app" },
    { name: "settings", section: "crew", crewPage: "escapes" },
    { name: "settings", section: "crew", crewPage: "escapes", repo: "p/app" },
    { name: "settings", section: "crew", crewPage: "possible", repo: "p/app" },
    { name: "settings", section: "crew", crewPage: "escape-settings" },
    { name: "settings", section: "crew", crewPage: "escape", id: "esc/1" },
    { name: "settings", section: "crew", crewPage: "backtest" },
    { name: "settings", section: "crew", crewPage: "estimate" },
    { name: "settings", section: "crew", crewPage: "report", id: "run-4n" },
  ])("round-trips %j", (route) => {
    expect(parseRoute(routeHref(route))).toEqual(route);
  });

  it("keeps the Crew entry pages at their own addresses and an unknown sub-page at its parent", () => {
    expect(routeHref({ name: "settings", section: "crew", crewPage: "escapes" })).toBe("#/settings/crew/escapes");
    expect(routeHref({ name: "settings", section: "crew", crewPage: "coverage" })).toBe("#/settings/crew/capacity/coverage");
    expect(routeHref({ name: "settings", section: "crew", crewPage: "escape", id: "a b" })).toBe("#/settings/crew/escapes/detail?id=a+b");
    expect(routeHref({ name: "settings", section: "crew", crewPage: "backtest" })).toBe("#/settings/crew/backtest");
    expect(routeHref({ name: "settings", section: "crew", crewPage: "dashboard", repo: "p/app" })).toBe("#/settings/crew/dashboard?repo=p%2Fapp");
    expect(parseRoute("#/settings/crew/dashboard/xyz")).toEqual({ name: "settings", section: "crew", crewPage: "dashboard" });
    expect(parseRoute("#/settings/crew/areas/xyz")).toEqual({ name: "settings", section: "crew", crewPage: "areas" });
    expect(parseRoute("#/settings/crew/capacity/xyz")).toEqual({ name: "settings", section: "crew", crewPage: "capacity" });
    expect(parseRoute("#/settings/crew/nope")).toEqual({ name: "settings", section: "crew" });
    // An id belongs only to the pages that name one.
    expect(parseRoute("#/settings/crew/escapes?id=x")).toEqual({ name: "settings", section: "crew", crewPage: "escapes" });
  });

  it("names the new escape pages and where the coverage map was opened from, and nothing else", () => {
    expect(routeHref({ name: "settings", section: "crew", crewPage: "possible" })).toBe("#/settings/crew/escapes/possible");
    expect(routeHref({ name: "settings", section: "crew", crewPage: "escape-settings" })).toBe("#/settings/crew/escapes/settings");
    expect(routeHref({ name: "settings", section: "crew", crewPage: "coverage", repo: "p-app", from: "escapes" })).toBe("#/settings/crew/capacity/coverage?repo=p-app&from=escapes");
    // The origin belongs to the coverage page only, and only the two known origins are read: anything else is no origin.
    expect(routeHref({ name: "settings", section: "crew", crewPage: "escapes", from: "capacity" })).toBe("#/settings/crew/escapes");
    expect(parseRoute("#/settings/crew/capacity/coverage?from=capacity")).toEqual({ name: "settings", section: "crew", crewPage: "coverage", from: "capacity" });
    expect(parseRoute("#/settings/crew/capacity/coverage?from=elsewhere")).toEqual({ name: "settings", section: "crew", crewPage: "coverage" });
    expect(parseRoute("#/settings/crew/capacity/coverage?from=")).toEqual({ name: "settings", section: "crew", crewPage: "coverage" });
    expect(parseRoute("#/settings/crew/escapes?from=capacity")).toEqual({ name: "settings", section: "crew", crewPage: "escapes" });
  });

  it("reads a row key from a Settings address, only a plain one, and never on Crew or over a sub-page", () => {
    expect(parseRoute("#/settings/chat/search-engine")).toEqual({ name: "settings", section: "chat", row: "search-engine" });
    expect(parseRoute("#/settings/appearance/theme-editor")).toEqual({ name: "settings", section: "appearance", sub: "theme-editor" });
    for (const bad of ["__proto__", "Search_Engine", "a%2Fb", "a b", "-a", "a--b", "a".repeat(5) + "-", ""]) {
      expect(parseRoute(`#/settings/chat/${bad}`)).toEqual({ name: "settings", section: "chat" });
    }
    expect(parseRoute("#/settings/crew/search-engine")).toEqual({ name: "settings", section: "crew" });
    expect(routeHref({ name: "settings", section: "chat", row: "__proto__" })).toBe("#/settings/chat");
  });

  it("falls back to the list for a pull request link without a number", () => {
    expect(parseRoute("#/pulls/p1")).toEqual({ name: "pulls" });
    expect(parseRoute("#/pulls/p1/abc")).toEqual({ name: "pulls" });
  });

  it("falls back to the issue list for an issue link without a key", () => {
    expect(parseRoute("#/issues/github")).toEqual({ name: "issues" });
    // A key is unique only within a scope, so a link without one is the list.
    expect(parseRoute("#/issues/github/412")).toEqual({ name: "issues" });
    expect(parseRoute("#/issues/github/orbyn-dev%2Forbyn-engineering/412")).toEqual({ name: "issues", tracker: "github", scope: "orbyn-dev/orbyn-engineering", key: "412" });
    expect(routeHref({ name: "issues", tracker: "github", scope: "orbyn-dev/orbyn-engineering", key: "412" })).toBe("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  });

  it("refuses malformed or action-bearing Crew task locators, as a not-found address", () => {
    for (const part of ["", ".", "..", "%2Ehidden", "a/b", "a/", "a%2Fb", "a%5Cb", "a%252Fb", "a%3Fb", "a%23b", "a%00b", "a%20b", "a%", "a?once=1", "a#publish", "a/publish", "a".repeat(97)]) {
      const route = parseRoute(`#/crew/flow/task/${part}`);
      expect(route.name).toBe("not-found");
      expect(route).not.toEqual({ name: "home" });
    }
    expect(parseRoute("#/crew/flow/task/")).toEqual({ name: "not-found", path: "crew/flow/task/" });
    expect(routeHref({name:"crew",tab:"flow",task:"a/b"})).toBe("#/");
    expect(routeHref({name:"crew",tab:"approvals",task:"task-1"})).toBe("#/");
  });

  it("preserves explicit malformed IDs and selection across settings navigation", () => {
    expect(parseRoute("#/settings/worktrees?project=g?bad")).toEqual({ name: "settings", section: "worktrees", selection: { kind: "project_group", group_key: "g?bad" } });
    expect(parseRoute("#/settings/worktrees?checkout=gone")).toEqual({ name: "settings", section: "worktrees", selection: { kind: "checkout", group_key: "", project_id: "gone" } });
    expect(parseRoute("#/settings/worktrees?project=")).toEqual({ name: "settings", section: "worktrees", selection: { kind: "project_group", group_key: "" } });
    window.location.hash = "#/settings/worktrees?project=g&checkout=p";
    navigate({ name: "settings", section: "threads" });
    expect(window.location.hash).toBe("#/settings/threads?project=g&checkout=p");
    window.location.hash = "#/settings/worktrees?machine=missing&project=g";
    navigate({ name: "settings", section: "threads" });
    expect(parseRoute(window.location.hash)).toEqual({ name: "settings", section: "threads", machine: "missing", selection: { kind: "project_group", group_key: "g" } });
    navigate({ name: "settings", section: "threads", selection: undefined });
    expect(window.location.hash).toBe("#/settings/threads");
  });

  it("keeps a task link until another settings route replaces it", () => {
    expect(parseRoute("#/settings/tasks?task=")).toEqual({ name: "settings", section: "tasks", task: "" });
    window.location.hash = "#/settings/tasks?task=task-1&project=g";
    navigate({ name: "settings", section: "tasks", task: "task-2" });
    expect(window.location.hash).toBe("#/settings/tasks?project=g&task=task-2");
    navigate({ name: "settings", section: "threads" });
    expect(window.location.hash).toBe("#/settings/threads?project=g");
  });

  it("reads a page inside a page, and keeps an unknown page name as data, never as a lookup key", () => {
    expect(parseRoute("#/settings/appearance/theme-editor")).toEqual({ name: "settings", section: "appearance", sub: "theme-editor" });
    // A sub-page the section does not have is read as a row key, which Settings looks up and ignores when it has no such row.
    expect(parseRoute("#/settings/appearance/nope")).toEqual({ name: "settings", section: "appearance", row: "nope" });
    expect(parseRoute("#/settings/chat/theme-editor")).toEqual({ name: "settings", section: "chat", row: "theme-editor" });
    expect(routeHref({ name: "settings", section: "chat", sub: "theme-editor" })).toBe("#/settings/chat");
    for (const name of ["__proto__", "constructor", "toString", "hasOwnProperty"]) {
      expect(parseRoute(`#/settings/${name}`)).toEqual({ name: "settings", section: null, unknown: name });
      // A row key is data: it is never a property name on anything (Settings keeps its rows in a Map).
      expect(parseRoute(`#/settings/appearance/${name}`)).toEqual(name === "constructor" ? { name: "settings", section: "appearance", row: name } : { name: "settings", section: "appearance" });
    }
    // The note is shown once; an address never carries it.
    expect(routeHref({ name: "settings", section: null, unknown: "voice" })).toBe("#/settings");
  });

  it("reads an address that names no screen as not found, and unknown sections as none", () => {
    expect(parseRoute("#/nowhere")).toEqual({ name: "not-found", path: "nowhere" });
    expect(parseRoute("#/settings/nope")).toEqual({ name: "settings", section: null, unknown: "nope" });
  });
});

describe("an address that names no screen", () => {
  it("is a not-found route that keeps the address, never another page's fallback", () => {
    expect(parseRoute("#/nonsense")).toEqual({ name: "not-found", path: "nonsense" });
    expect(parseRoute("#/nowhere/deeper?from=chat")).toEqual({ name: "not-found", path: "nowhere/deeper" });
    expect(parseRoute("#/crew/flow/task/!!!")).toEqual({ name: "not-found", path: "crew/flow/task/!!!" });
    // The negative: an unknown path is not quietly read as the home page with the bad address left in place.
    expect(parseRoute("#/nonsense").name).not.toBe("home");
    expect(parseRoute("#/crew/flow/task/!!!").name).not.toBe("home");
    // A root address still is home: nothing was asked for, so nothing is missing.
    expect(parseRoute("#/")).toEqual({ name: "home" });
    expect(parseRoute("")).toEqual({ name: "home" });
  });

  it("keeps its own address, so the page can show what was asked for", () => {
    expect(routeHref({ name: "not-found", path: "nowhere/deeper" })).toBe("#/nowhere/deeper");
    expect(parseRoute(routeHref(parseRoute("#/does/not/exist")))).toEqual({ name: "not-found", path: "does/not/exist" });
  });
});

describe("closing Settings", () => {
  const settle = async () => {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  };

  it("goes back to the page it covered, adding no entry to back out of", async () => {
    window.location.hash = "#/usage";
    await settle();
    window.location.hash = "#/threads/t-done";
    await settle();
    const page = window.location.hash;
    const length = window.history.length;

    navigate({ name: "settings", section: "chat" });
    expect(parseRoute(window.location.hash)).toEqual({ name: "settings", section: "chat" });
    // A move inside Settings holds that same entry: Back out of Settings is the page, not the section before.
    navigate({ name: "settings", section: "appearance" });
    expect(window.history.length).toBe(length + 1);

    closeSettings({ name: "thread", id: "t-done" });
    await settle();
    expect(parseRoute(window.location.hash)).toEqual({ name: "thread", id: "t-done" });
    expect(window.location.hash).toBe(page);
    // Closing added nothing: one entry for the visit, and no second one to walk back out of.
    expect(window.history.length).toBe(length + 1);
    expect(window.history.length).not.toBe(length + 2);
    expect(window.history.state).not.toMatchObject({ orbynSettings: true });

    // The negative the issue reported: Back from here must not land on Settings again.
    window.history.back();
    await settle();
    expect(parseRoute(window.location.hash)).toEqual({ name: "usage" });
  });

  it("replaces its own entry when it arrived on its own address, so nothing grows", async () => {
    window.location.replace("#/settings/appearance");
    await settle();
    const length = window.history.length;
    closeSettings({ name: "home" });
    await settle();
    expect(parseRoute(window.location.hash)).toEqual({ name: "home" });
    expect(window.history.length).toBe(length);
  });
});
