import { describe, expect, it } from "vitest";
import { issueFixtures } from "../../../mock/issues";
import type { IssueRef } from "./contract";
import { activeChips, applyQuery, choiceLabel, hostFor, knownTrackers, trackerDetail, sameState, trackerLine, trackerStatus, labelCounts, noQuery, numberOf, parseSearch, shownLine, stateWord, threadMarker } from "./model";

const ref = (state: IssueRef["state"], key = "1"): IssueRef => ({ tracker: "github", scope: "o/r", key, display_key: `#${key}`, title: "t", state, labels: [] });

it("says how a closed issue closed only when the tracker says so", () => {
  expect(stateWord({ category: "open", name: "Open" })).toBe("Open");
  expect(stateWord({ category: "closed", name: "Closed" })).toBe("Closed");
  expect(stateWord({ category: "closed", name: "Closed", close_reason: "completed" })).toBe("Closed");
  expect(stateWord({ category: "closed", name: "Closed", close_reason: "not_planned" })).toBe("Closed as not planned");
});

it("reads label: words out of the search box and keeps the rest as words", () => {
  expect(parseSearch("label:bug reload")).toEqual({ text: "reload", labels: ["bug"] });
  expect(parseSearch("  keyboard   label:perf label:a11y ")).toEqual({ text: "keyboard", labels: ["perf", "a11y"] });
  expect(parseSearch("")).toEqual({ text: "", labels: [] });
});

it("filters by state, label, assignee and words, newest update first, and by nothing else", () => {
  const all = issueFixtures();
  expect(applyQuery(all, noQuery, "riley").map((one) => one.key)).toEqual(["412", "409", "ORB-87", "ORB-91", "405", "398"]);
  expect(applyQuery(all, { ...noQuery, state: "closed" }, null).map((one) => one.key)).toEqual(["391", "377"]);
  expect(applyQuery(all, { ...noQuery, tracker: "linear" }, null)).toHaveLength(2);
  expect(applyQuery(all, { ...noQuery, labels: ["BUG"], text: "reload" }, null).map((one) => one.key)).toEqual(["405"]);
  expect(applyQuery(all, { ...noQuery, assignee: "none" }, null).map((one) => one.key)).toEqual(["ORB-87", "ORB-91", "405"]);
  expect(applyQuery(all, { ...noQuery, assignee: "me" }, "sam").map((one) => one.key)).toEqual(["398"]);
  expect(applyQuery(all, { ...noQuery, assignee: "me" }, null)).toEqual([]);
});

it("counts labels, most used first", () => {
  expect(labelCounts(issueFixtures()).slice(0, 2)).toEqual([{ name: "bug", count: 3 }, { name: "design", count: 2 }]);
});

it("makes one chip per active filter", () => {
  expect(activeChips(noQuery)).toEqual([]);
  expect(activeChips({ ...noQuery, text: "reload", labels: ["bug"], assignee: "none" }).map((chip) => chip.label)).toEqual(["“reload”", "label:bug", "Unassigned"]);
});

it("takes a bare number or #number as a number and everything else as words", () => {
  expect(numberOf("412")).toBe("412");
  expect(numberOf(" #412 ")).toBe("412");
  expect(numberOf("ORB-87")).toBeNull();
  expect(numberOf("https://github.com/o/r/issues/412")).toBeNull();
});

it("words the count under the toolbar from the unfiltered count", () => {
  expect(shownLine(1, { open: 14, closed: 3 }, "open")).toBe("1 of 14 open issues");
  expect(shownLine(2, { open: 14, closed: 3 }, "all")).toBe("2 of 17 issues");
  expect(shownLine(2, null, "open")).toBeNull();
});

it("marks a thread with the ring, closed only when every issue is closed, and says nothing for none", () => {
  expect(threadMarker([])).toBeNull();
  expect(threadMarker([ref({ category: "open", name: "Open" })])).toMatchObject({ count: 1, closed: false, label: "Issue #1, open" });
  expect(threadMarker([ref({ category: "closed", name: "Closed" })])?.closed).toBe(true);
  expect(threadMarker([ref({ category: "closed", name: "Closed" }), ref({ category: "open", name: "Open" }, "2")])).toMatchObject({ count: 2, closed: false, label: "2 issues: #1, #2" });
});

describe("state choices and tracker lines", () => {
  it("names a workflow state by its own word and a plain one by the usual words", () => {
    expect(choiceLabel({ category: "open", name: "In review" })).toBe("In review");
    expect(choiceLabel({ category: "closed", name: "Closed", close_reason: "not_planned" })).toBe("Closed as not planned");
    expect(sameState({ category: "closed", name: "Closed" }, { category: "closed", name: "Closed", close_reason: "completed" })).toBe(false);
  });
  it("reads a tracker's credential from the row and caps the service's text", () => {
    expect(trackerStatus({ tracker: "github", connected: true })).toBe("signed_in");
    expect(trackerStatus({ tracker: "gitlab", connected: true })).toBe("saved");
    expect(trackerStatus({ tracker: "gitlab", connected: false })).toBe("not_connected");
    expect(trackerLine({ tracker: "gitlab", connected: true, host: `https://${"x".repeat(200)}` }).length).toBeLessThan(120);
    // A stored connection that is not working needs attention, and its reason is the service's, capped.
    expect(trackerStatus({ tracker: "gitlab", connected: false, host: "https://g.example", detail: "No answer." })).toBe("attention");
    expect(trackerStatus({ tracker: "github", connected: false, detail: "gh is not installed" })).toBe("not_connected");
    expect(trackerLine({ tracker: "github", connected: false, detail: "gh is not installed" })).toBe("gh is not installed");
    expect(trackerDetail({ tracker: "gitlab", connected: false, detail: "y".repeat(400) }).length).toBe(200);
  });
  it("sends a bare host name with https in front and leaves a scheme the person typed for the service to refuse", () => {
    expect(hostFor(" gitlab.example.com ")).toBe("https://gitlab.example.com");
    expect(hostFor("https://git.orbyn.dev")).toBe("https://git.orbyn.dev");
    expect(hostFor("http://git.orbyn.dev")).toBe("http://git.orbyn.dev");
  });
  it("keeps the five known trackers in the design's order and drops one the service invents", () => {
    const rows = knownTrackers([{ tracker: "linear", connected: false }, { tracker: "__proto__" as never, connected: true }, { tracker: "github", connected: true }]);
    expect(rows.map((row) => row.tracker)).toEqual(["github", "linear"]);
  });
});
