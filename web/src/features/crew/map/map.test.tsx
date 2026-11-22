import { act, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { HOSTILE_MARKERS } from "../../../../mock/hostile";
import { MOCK_CREW } from "../../../../mock/flow";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { CrewSurface } from "../autopilot/approvals/CrewSurface";
import { useBoardFocus } from "../board/focus";
import { LanesView } from "./LanesView";
import { MapView } from "./MapView";
import { useCrewMessages } from "./useMap";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  window.location.hash = "";
});

const fetched = () => vi.mocked(fetch).mock.calls;
const calls = (part: string) => fetched().filter(([url]) => String(url).includes(part));
const nonGets = () => fetched().filter(([, init]) => init?.method && init.method !== "GET");
const urls = () => fetched().map(([url]) => String(url));

async function openMap(scenario: "crew" | `crew-${string}` = "crew") {
  mock.reset(scenario);
  const user = userEvent.setup();
  render(<MapView />);
  await screen.findByRole("list", { name: "Crew" });
  return user;
}
async function openLanes(scenario: "crew" | `crew-${string}` = "crew") {
  mock.reset(scenario);
  const user = userEvent.setup();
  render(<LanesView />);
  await screen.findByRole("group", { name: "Swim lanes" });
  return user;
}
const tree = () => within(screen.getByRole("list", { name: "Crew" }));
const row = (name: RegExp) => tree().getByRole("button", { name });

describe("the Crew map on the map route", () => {
  it("draws nothing and reads no map route while Crew Preview is off (the default scenario answers 404)", async () => {
    mock.reset("default");
    const { container } = render(<MapView />);
    await waitFor(() => expect(calls("/api/crew/preview")).toHaveLength(1));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container).toBeEmptyDOMElement();
    expect(calls("/api/crew/flow")).toHaveLength(0);
  });

  it("draws the lead, then each worker under it, with the brief's outcome, state, unread and the subagents word", async () => {
    await openMap();
    const rows = tree().getAllByRole("listitem");
    expect(rows).toHaveLength(4);
    expect(rows[0]).toHaveTextContent("claude");
    expect(rows[0]).toHaveTextContent("Opus 5.5 · Medium");
    expect(rows[0]).toHaveTextContent("Lead");
    expect(rows[1]).toHaveTextContent("Use next in the auth redirect when it points inside the app");
    expect(rows[1]).toHaveTextContent("Running");
    expect(rows[1]!.querySelector(".mp-unread")).toHaveAttribute("title", "1 unread");
    expect(rows[2]).toHaveTextContent("Verifying");
    // Only workers hang under the lead.
    expect(rows.map((one) => one.hasAttribute("data-worker"))).toEqual([false, true, true, true]);
    // The map route carries no token count, branch, level or blocked state: those are Planned chips, never values.
    expect(document.body.textContent).not.toMatch(/\d+k tokens|Blocked|\bL[0-2]\b|Open thread[^A]/);
    expect(nonGets()).toHaveLength(0);
  });

  it("opens a member's details with its messages as one line each, and never a message's text", async () => {
    const user = await openMap();
    await user.click(row(/Add tests for the redirect/));
    const detail = await screen.findByRole("complementary", { name: /^codex, details/ });
    expect(within(detail).getByText("auth/*.test.ts")).toBeVisible();
    expect(within(detail).getByText("lead")).toBeVisible();
    const lines = await within(detail).findByRole("list", { name: "Messages" });
    const text = within(lines).getAllByRole("listitem").map((one) => one.textContent);
    expect(text.some((line) => /^\d\d:\d\d · implement → tests · a fact · live$/.test(line!))).toBe(true);
    // A message with no delivery says nothing about it; a held, failed and tool-result one say so.
    expect(text.some((line) => /· lead → all|lead → tests · a fact · failed$/.test(line!))).toBe(true);
    expect(urls().some((url) => url.includes("/flow/messages") && url.includes("member=tests"))).toBe(true);
    await user.click(within(detail).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("complementary")).toBeNull();
  });

  it("says a message with no delivery has none, and words held, failed and tool result", async () => {
    const user = await openMap();
    await user.click(row(/Opus 5.5/));
    const lines = within(await screen.findByRole("list", { name: "Messages" })).getAllByRole("listitem").map((one) => one.textContent ?? "");
    const answer = lines.find((line) => line.includes("an answer"))!;
    expect(answer).toMatch(/lead → all · an answer$/);
    expect(lines.some((line) => /held$/.test(line))).toBe(true);
    expect(lines.some((line) => /tool result$/.test(line))).toBe(true);
    expect(lines.some((line) => /failed$/.test(line))).toBe(true);
  });

  it("draws the member menu: Cancel and Re-run checks inert (the map carries no flags) and the rest Planned", async () => {
    const user = await openMap();
    await user.click(row(/Use next in the auth redirect/));
    await user.click(await screen.findByRole("button", { name: "Actions for this member" }));
    const menu = await screen.findByRole("menu", { name: "Member actions" });
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((item) => item.textContent)).toEqual([
      "Open thread Planned",
      "Cancel Use the Board card",
      "Re-run checks Use the Board card",
      "Reassign engine Planned",
      "Message into a lane Planned",
      "Add reviewer lane Planned",
      "Propose a check Planned",
    ]);
    for (const item of items) expect(item).toBeDisabled();
    await user.click(items[1]!);
    expect(nonGets()).toHaveLength(0);
  });

  it("says a crew with no workers has none yet", async () => {
    mock.reset("crew-map-empty");
    render(<MapView />);
    expect(await screen.findByText("No workers yet")).toBeVisible();
    expect(screen.getByText(/The lead hands tasks to other engines/)).toBeVisible();
  });

  it("draws a failed worker red, a quiet one in quiet ink and one that may be stuck amber", async () => {
    await openMap("crew-map-problems");
    const failed = row(/Typecheck failed twice/);
    expect(within(failed).getByText("Failed")).toHaveClass("mp-err");
    const quiet = within(row(/Add tests for the redirect/)).getByText("Quiet · 5 min");
    expect(quiet).toHaveClass("mp-neutral");
    expect(quiet).not.toHaveClass("mp-warn");
    expect(within(row(/Update the docs/)).getByText("May be stuck")).toHaveClass("mp-warn");
    // No colour alone: every one of them is a word.
    expect(tree().queryByText("Blocked")).toBeNull();
  });

  it("says an engine that is not installed or signed out cannot take work, from the registry's states", async () => {
    await openMap("crew-map-offline");
    expect(screen.getByText("codex isn’t running.")).toBeVisible();
    expect(within(screen.getByRole("list", { name: "Engines" })).getByText("codex · Not installed")).toBeVisible();
    expect(within(row(/Add tests for the redirect/)).getByText("Not installed")).toBeVisible();
    expect(within(row(/Add tests for the redirect/)).getByText("Needs you")).toHaveClass("mp-warn");
  });

  it("says an engine that is signed out is not signed in", async () => {
    await openMap("crew-map-signedout");
    expect(screen.getByText("codex isn’t signed in.")).toBeVisible();
    expect(within(row(/Add tests for the redirect/)).getByText("Signed out")).toBeVisible();
  });

  it("draws one worker, an ACP engine and a Claude worker that reports its subagents", async () => {
    await openMap("crew-map-acp");
    expect(within(screen.getByRole("group", { name: "Subagent reports" })).getByText("Subagents not reported")).toBeVisible();
    expect(tree().getAllByRole("listitem")).toHaveLength(2);
  });

  it("draws a Claude worker, which reports its subagents, with no not-reported line", async () => {
    await openMap("crew-map-deep");
    const subs = document.querySelectorAll(".mp-subcard");
    expect(subs).toHaveLength(1);
    expect(subs[0]).toHaveTextContent("Subagents reported");
    expect(subs[0]).not.toHaveTextContent("Clear");
  });

  it("draws one worker beside the lead", async () => {
    await openMap("crew-map-one");
    expect(tree().getAllByRole("listitem")).toHaveLength(2);
  });

  it("says the list was cut at its cap when truncated", async () => {
    await openMap("crew-map-truncated");
    expect(screen.getByRole("note")).toHaveTextContent("A list was cut at its cap; the newest are shown.");
  });

  it("names an unreadable service and retries with reads only", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/flow/map", 1);
    const user = userEvent.setup();
    render(<MapView />);
    expect(await screen.findByText("Can’t read the map")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("list", { name: "Crew" })).toBeVisible();
    expect(nonGets()).toHaveLength(0);
  });

  it("never draws a message body, a thread, a worktree, tokens or a level a hostile service adds, as text or in any attribute", async () => {
    const user = await openMap("crew-map-hostile");
    await user.click(row(/Add tests for the redirect/));
    await screen.findByRole("list", { name: "Messages" });
    // The whole serialised page: text, titles, aria-*, data-* and every other attribute.
    for (const marker of HOSTILE_MARKERS) expect(document.body.innerHTML, marker).not.toContain(marker);
  });

  it("does not send a member filter for a name the service scrubbed, and says it cannot be filtered", async () => {
    const user = await openMap("crew-map-scrubbed");
    await user.click(row(/Work whose member name was scrubbed/));
    expect(await screen.findByText("Messages can’t be filtered for this member: its name is withheld.")).toBeVisible();
    await user.click(row(/Work whose member name was withheld/));
    expect(await screen.findByText("Messages can’t be filtered for this member: its name is withheld.")).toBeVisible();
    expect(urls().some((url) => url.includes("/flow/messages"))).toBe(false);
    // A member with a real name is still filtered.
    await user.click(row(/Add tests for the redirect/));
    await screen.findByRole("list", { name: "Messages" });
    expect(urls().filter((url) => url.includes("/flow/messages") && url.includes("member=tests"))).toHaveLength(1);
  });

  it("reads the details' messages again every 30 seconds while the window is visible, and not while it is hidden", async () => {
    const user = await openMap();
    await user.click(row(/Add tests for the redirect/));
    await screen.findByRole("list", { name: "Messages" });
    const reads = () => urls().filter((url) => url.includes("/flow/messages") && url.includes("member=tests")).length;
    expect(reads()).toBe(1);
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    try {
      // The interval was set under real timers; a new selection sets one under the fake clock.
      await user.click(screen.getByRole("button", { name: "Close" }));
      await user.click(row(/Add tests for the redirect/));
      await screen.findByRole("list", { name: "Messages" });
      const base = reads();
      await vi.advanceTimersByTimeAsync(30_000);
      await waitFor(() => expect(reads()).toBe(base + 1));
      Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "hidden" });
      await vi.advanceTimersByTimeAsync(90_000);
      expect(reads()).toBe(base + 1);
    } finally {
      Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "visible" });
      vi.useRealTimers();
    }
    expect(nonGets()).toHaveLength(0);
  });
});

const swimEl = () => screen.getByRole("group", { name: "Swim lanes" });
const swim = () => within(swimEl());

describe("the wide Map tree from the map route", () => {
  it("puts the lead in the first column and a card per worker in the second, with a solid line for who started whom", async () => {
    await openMap();
    const cards = [...document.querySelectorAll<HTMLElement>(".mp-node")];
    expect(cards).toHaveLength(4);
    expect(cards.map((card) => card.style.left)).toEqual(["0px", "290px", "290px", "290px"]);
    expect(cards.map((card) => card.style.top)).toEqual(["160px", "0px", "160px", "320px"]);
    expect(document.querySelectorAll(".mp-ed .mp-start")).toHaveLength(3);
    expect(nonGets()).toHaveLength(0);
  });

  it("draws each message as a dashed line between its members, with the message's own label and how it arrived", async () => {
    await openMap();
    // Eight messages between members: seven between two, one to everyone drawn to each other lane.
    expect(document.querySelectorAll(".mp-ed .mp-talk")).toHaveLength(10);
    expect(document.querySelectorAll(".mp-elab")).toHaveLength(8);
    const label = document.querySelector(".mp-elab")!;
    expect(label).toHaveTextContent("implement → tests");
    expect(label).toHaveTextContent("a fact");
    expect(label).toHaveTextContent("live");
    expect(label.getAttribute("title")).toBe("implement → tests: 1 message, a fact, newest live");
  });

  it("draws the design's legend for what the lines mean, and its words for how a message arrives", async () => {
    await openMap();
    const legend = document.querySelector(".mp-legend")!;
    expect(legend).toHaveTextContent("started by");
    expect(legend).toHaveTextContent("talking to");
    expect(legend).toHaveTextContent("“live” means handed to a turn in progress");
    expect(legend).toHaveTextContent("“next turn” means queued, or sent with the next prompt");
  });

  it("draws one subagent report beside each worker: reported without inventing the rows, not reported as not an empty tree", async () => {
    await openMap();
    const subs = [...document.querySelectorAll<HTMLElement>(".mp-subcard")];
    expect(subs).toHaveLength(3);
    expect(subs.map((sub) => sub.dataset.member)).toEqual(["implement", "tests", "review"]);
    const reported = subs[0]!;
    expect(reported).toHaveTextContent("Subagents reported");
    expect(reported).not.toHaveTextContent("None running now");
    expect(reported.querySelector(".mp-pl")?.getAttribute("title")).toBe("Planned: no Map route names a worker’s subagents");
    const missing = subs[1]!;
    expect(missing).toHaveTextContent("Subagents not reported");
    expect(missing).toHaveTextContent("Not an empty tree: not reported.");
  });

  it("keeps per-worker tokens and the branch Planned, naming the reads the map does not carry", async () => {
    await openMap();
    const card = document.querySelector<HTMLElement>('.mp-node[data-worker] .mp-card')!;
    const titles = [...card.querySelectorAll(".mp-pl")].map((pl) => pl.getAttribute("title"));
    expect(titles).toContain("Planned: no Map route reports per-worker tokens yet");
    expect(titles).toContain("Planned: no Map route reports a branch");
    expect(card).toHaveTextContent("tokens");
    expect(card).toHaveTextContent("branch");
    expect(card.textContent).not.toMatch(/\d+k tokens/);
    // The tree draws no lane level: no route reports one.
    expect(document.body.textContent).not.toMatch(/\bL[0-2]\b/);
  });
});

describe("the conversation lanes' swim lanes", () => {
  it("draws one lane per member, the lead first, with the engine and model the map route carries", async () => {
    await openLanes();
    const lanes = swim().getAllByRole("listitem");
    expect(lanes.map((lane) => lane.querySelector("b")?.textContent)).toEqual(["Lead", "implement", "tests", "review"]);
    expect(lanes[0]).toHaveTextContent("claude · Opus 5.5");
    expect(lanes[2]).toHaveTextContent("codex · GPT-5.5");
    expect(nonGets()).toHaveLength(0);
  });

  it("draws each message as an arrow between its lanes, labelled with how it arrived and its kind", async () => {
    await openLanes();
    // Eight messages, one of them to everyone, which is drawn to each other lane.
    expect(document.querySelectorAll(".mp-arrow")).toHaveLength(10);
    const labels = [...document.querySelectorAll(".mp-lmk")].map((one) => one.textContent);
    expect(labels).toContain("live · a fact");
    expect(labels).toContain("next turn · a handoff");
    expect(labels).toContain("tool result · a handoff");
    expect(labels).toContain("held · a message");
    expect(labels).toContain("failed · a fact");
    // A message with no delivery row says nothing about it: only its kind is drawn.
    expect(labels).toContain("an answer");
    for (const one of labels) expect(one).not.toContain("undefined");
  });

  it("takes its time axis from the messages and marks the lane level it cannot read Planned", async () => {
    await openLanes();
    const ticks = [...document.querySelectorAll(".mp-tick")];
    expect(ticks.length).toBeGreaterThanOrEqual(3);
    for (const tick of ticks) expect(tick.textContent).toMatch(/^\d\d:\d\d$/);
    expect(swimEl().querySelector(".mp-pl")?.getAttribute("title")).toBe("Planned: no route reports a lane’s level");
    expect(document.body.textContent).not.toMatch(/\bL[0-2]\b/);
  });

  it("draws the worker's outcome as a bar over its run, and no crew label the map does not carry", async () => {
    await openLanes();
    const bars = [...document.querySelectorAll<HTMLElement>(".mp-lbar")];
    expect(bars.map((bar) => bar.textContent)).toEqual([
      "Use next in the auth redirect when it points inside the app",
      "Add tests for the redirect, including //evil.com",
      "Which target wins when both next and returnTo are set?",
    ]);
    expect(bars.every((bar) => bar.style.width !== "0px")).toBe(true);
    const lead = swim().getAllByRole("listitem")[0]!;
    expect(lead.querySelector(".mp-lbar")).toBeNull();
  });
});

describe("the conversation lanes on the message and fact routes", () => {
  it("draws draws nothing while Crew Preview is off", async () => {
    mock.reset("default");
    const { container } = render(<LanesView />);
    await waitFor(() => expect(calls("/api/crew/preview")).toHaveLength(1));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container).toBeEmptyDOMElement();
  });

  it("lists the messages in one line each (time, from, to, kind, delivery) and the board's facts by label", async () => {
    await openLanes();
    const lines = within(await screen.findByRole("list", { name: "Messages" })).getAllByRole("listitem").map((one) => one.textContent ?? "");
    expect(lines).toHaveLength(8);
    expect(lines[0]).toMatch(/^\d\d:\d\d · implement → tests · a fact · live$/);
    expect(lines.some((line) => /lead → all · an answer$/.test(line))).toBe(true);
    expect(screen.getByText("8 of 8 shown")).toBeVisible();
    const facts = within(await screen.findByRole("list", { name: "Facts" })).getAllByRole("listitem");
    expect(facts).toHaveLength(3);
    expect(facts[0]).toHaveTextContent("contract");
    expect(facts[0]).toHaveTextContent("contract · implement · rev 2");
    expect(facts[0]).toHaveTextContent("Verified");
    expect(facts[1]).not.toHaveTextContent("Verified");
    expect(swim().getAllByRole("listitem")).toHaveLength(4);
    expect(nonGets()).toHaveLength(0);
  });

  it("narrows to one member with a read of its own", async () => {
    const user = await openLanes();
    await screen.findByRole("list", { name: "Messages" });
    await user.selectOptions(screen.getByRole("combobox", { name: "Member" }), "review");
    await waitFor(() => expect(urls().some((url) => url.includes("/flow/messages") && url.includes("member=review"))).toBe(true));
    await waitFor(() => expect(screen.getByText(/of \d+ shown/)).toHaveTextContent("3 of 3 shown"));
  });

  it("says nothing has been said yet, with the lanes still listed", async () => {
    await openLanes("crew-map-no-messages");
    expect(await screen.findByText("No messages yet")).toBeVisible();
    expect(screen.getByText(/Workers talk to each other and to the lead through Crew/)).toBeVisible();
    expect(swim().getAllByRole("listitem").length).toBeGreaterThan(1);
  });

  it("pages messages with +N more, and a double call sends one read", async () => {
    mock.reset("crew-map-many-messages");
    const real = vi.mocked(fetch).getMockImplementation()!;
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (String(input).includes("offset=50")) await new Promise((resolve) => setTimeout(resolve, 60));
      return real(input, init);
    });
    const { result } = renderHook(() => useCrewMessages(MOCK_CREW, null));
    await waitFor(() => expect(result.current.rows).toHaveLength(50));
    await act(async () => {
      void result.current.showMore();
      void result.current.showMore();
    });
    await waitFor(() => expect(result.current.rows).toHaveLength(60));
    expect(calls("offset=50")).toHaveLength(1);
  });

  it("never draws a message body, a fact value, a thread or a worktree a hostile service adds, as text or in any attribute", async () => {
    await openLanes("crew-map-hostile");
    await screen.findByRole("list", { name: "Facts" });
    await screen.findByRole("list", { name: "Messages" });
    for (const marker of HOSTILE_MARKERS) expect(document.body.innerHTML, marker).not.toContain(marker);
  });

  it("does not offer a scrubbed member name as a filter", async () => {
    await openLanes("crew-map-scrubbed");
    const option = screen.getByRole("option", { name: "[redacted] (can’t be filtered)" });
    expect(option).toBeDisabled();
    expect(screen.getByRole("option", { name: "Withheld. (can’t be filtered)" })).toBeDisabled();
    expect(screen.getByRole("option", { name: "tests" })).toBeEnabled();
  });

  it("names an unreadable service and retries with reads only", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/flow/map", 1);
    const user = userEvent.setup();
    render(<LanesView />);
    expect(await screen.findByText("Can’t read the lanes")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("group", { name: "Swim lanes" })).toBeVisible();
    expect(nonGets()).toHaveLength(0);
  });
});

describe("all five registry states", () => {
  it("draws each engine's registry word in the list, on its row and, for the four that cannot take work, in a banner", async () => {
    mock.reset("crew-map-engine-states");
    render(<MapView />);
    await screen.findByRole("list", { name: "Crew" });
    const engines = within(screen.getByRole("list", { name: "Engines" })).getAllByRole("listitem").map((one) => one.textContent);
    expect(engines).toEqual(["claude · Ready", "codex · Not installed", "opencode · Signed out", "cursor · Sign-in unconfirmed", "kimi · Broken"]);
    expect(within(row(/Work on cursor/)).getByText("Sign-in unconfirmed")).toBeVisible();
    expect(within(row(/Work on kimi/)).getByText("Broken")).toBeVisible();
    expect(within(row(/Work on codex/)).getByText("Not installed")).toBeVisible();
    expect(within(row(/Work on opencode/)).getByText("Signed out")).toBeVisible();
    const banners = screen.getAllByRole("status").map((one) => one.textContent);
    expect(banners).toEqual(["codex isn’t running.", "opencode isn’t signed in.", "cursor: Sign-in unconfirmed", "kimi: Broken"]);
  });
});

describe("Open on Board", () => {
  beforeEach(() => useBoardFocus.getState().clear());

  async function surface(scenario: "crew" | `crew-${string}` = "crew") {
    mock.reset(scenario);
    const user = userEvent.setup();
    render(<CrewSurface tab="map" />);
    await screen.findByRole("list", { name: "Crew" });
    return user;
  }

  it("switches to the Board tab and puts focus on the node's card, with no write and no read but the board's", async () => {
    const user = await surface();
    await user.click(row(/Add tests for the redirect/));
    const before = fetched().length;
    await user.click(await screen.findByRole("button", { name: "Open on Board" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "Board" })).toHaveAttribute("aria-selected", "true"));
    await screen.findByRole("region", { name: /^Verifying/ });
    await waitFor(() => expect(document.activeElement?.getAttribute("data-task")).toBe("b-verifying"));
    expect(urls().slice(before).every((url) => url.includes("/api/crew/flow/board") || url.includes("/api/crew/flow/crews") || url.includes("/api/crew/preview"))).toBe(true);
    expect(nonGets()).toHaveLength(0);
    expect(useBoardFocus.getState().task).toBeNull();
  });

  it("says plainly when the card is not on the current page of its column", async () => {
    const user = await surface("crew-map-many");
    await user.click(row(/Task 2/));
    await user.click(await screen.findByRole("button", { name: "Open on Board" }));
    expect(await screen.findByText("That task’s card isn’t on the current page of its column. Use “+N more” in its column to find it.")).toBeVisible();
    expect(nonGets()).toHaveLength(0);
  });

  it("keeps Cancel and Re-run checks inert on the node", async () => {
    const user = await surface();
    await user.click(row(/Use next in the auth redirect/));
    await user.click(await screen.findByRole("button", { name: "Actions for this member" }));
    const menu = await screen.findByRole("menu", { name: "Member actions" });
    expect(within(menu).getByRole("menuitem", { name: /^Cancel/ })).toBeDisabled();
    expect(within(menu).getByRole("menuitem", { name: /^Re-run checks/ })).toBeDisabled();
  });

  it("refuses a task id the shared id check refuses, so it never becomes a route, a hash or an element id", () => {
    for (const id of ["..", ".", "a/b", "a?b", "a#b", "", ".x", "x".repeat(97), "<img src=x>"]) {
      expect(useBoardFocus.getState().ask("crew", id)).toBe(false);
      expect(useBoardFocus.getState().task).toBeNull();
    }
    expect(useBoardFocus.getState().ask("", "b-running")).toBe(false);
    expect(useBoardFocus.getState().ask("crew", "b-running")).toBe(true);
    expect(window.location.hash).toBe("");
  });
});

/** A panel or window narrower than 640 px: the board's own width hook reads this from the element's observed size. */
function narrowPanel() {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(private readonly callback: ResizeObserverCallback) {}
      observe() {
        this.callback([{ contentRect: { width: 500, height: 600 } } as ResizeObserverEntry], this as unknown as ResizeObserver);
      }
      unobserve() {}
      disconnect() {}
    },
  );
}

describe("the Map as the nested list, never the drawn tree, in a narrow panel and past eight workers", () => {
  it("draws the lead, then each worker under it, with the outcome, state, unread and the subagents word, when the panel is narrow", async () => {
    narrowPanel();
    try {
      await openMap();
      const rows = tree().getAllByRole("listitem");
      expect(rows).toHaveLength(4);
      expect(rows[0]).toHaveTextContent("claude");
      expect(rows[0]).toHaveTextContent("Opus 5.5 · Medium");
      expect(rows[0]).toHaveTextContent("Lead");
      expect(rows[1]).toHaveTextContent("Use next in the auth redirect when it points inside the app");
      expect(rows[1]).toHaveTextContent("Running");
      expect(rows[1]).toHaveTextContent("1 unread");
      expect(rows[1]).not.toHaveTextContent("Subagents not reported");
      expect(rows[2]).toHaveTextContent("Verifying");
      expect(rows[2]).toHaveTextContent("Subagents not reported");
      expect(rows[3]).toHaveTextContent("Subagents not reported");
      expect(rows.map((one) => one.hasAttribute("data-worker"))).toEqual([false, true, true, true]);
      expect(document.querySelector(".mp-map, .mp-ed, .mp-subcard, .mp-card")).toBeNull();
      expect(document.body.textContent).not.toMatch(/tokens|Blocked|\bL[0-2]\b|Open thread[^A]/);
      expect(nonGets()).toHaveLength(0);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("still opens a member's details from the list", async () => {
    narrowPanel();
    try {
      const user = await openMap();
      await user.click(row(/Add tests for the redirect/));
      expect(await screen.findByRole("complementary", { name: /^codex, details/ })).toBeVisible();
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("is a list past eight workers even in a wide panel (crew-map-many)", async () => {
    await openMap("crew-map-many");
    expect(tree().getAllByRole("listitem")).toHaveLength(13);
    expect(document.querySelector(".mp-map, .mp-ed, .mp-subcard, .mp-card")).toBeNull();
    expect(row(/Task 2/)).toHaveTextContent("updated");
  });

  it("is the drawn tree in a wide panel with a few workers", async () => {
    await openMap();
    expect(document.querySelector(".mp-map")).not.toBeNull();
    expect(document.querySelector(".mp-nest")).toBeNull();
  });

  it("draws the lanes as the list of members, with no swim lanes, when the panel is narrow", async () => {
    narrowPanel();
    try {
      mock.reset("crew");
      render(<LanesView />);
      const lanes = await screen.findByRole("list", { name: "Lanes" });
      expect(within(lanes).getAllByRole("listitem")).toHaveLength(4);
      expect(within(lanes).getAllByRole("listitem")[0]).toHaveTextContent("Lead");
      expect(screen.queryByRole("group", { name: "Swim lanes" })).toBeNull();
      expect(document.querySelector(".mp-swim, .mp-ltl")).toBeNull();
      // The messages and the shared board still read.
      expect(await screen.findByRole("list", { name: "Messages" })).toBeVisible();
      expect(await screen.findByRole("list", { name: "Facts" })).toBeVisible();
      expect(nonGets()).toHaveLength(0);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("draws the swim lanes and not the list when the panel is wide", async () => {
    await openLanes();
    expect(document.querySelector(".mp-lanes")).toBeNull();
    expect(document.querySelector(".mp-swim")).not.toBeNull();
  });
});

describe("how long a worker has been going", () => {
  it("stops a finished worker's time at its last update, while a running one keeps going", async () => {
    await openMap("crew-map-problems");
    // The failed worker ran 30 min to 2 min ago: 28m, however long ago that was. A running worker's time is to now.
    const failed = [...document.querySelectorAll<HTMLElement>(".mp-card")].find((card) => card.textContent?.includes("Typecheck failed twice"))!;
    expect(failed).toHaveTextContent("Sonnet 5.5 · Medium · 28m");
    const running = [...document.querySelectorAll<HTMLElement>(".mp-card")].find((card) => card.textContent?.includes("Add tests for the redirect"))!;
    expect(running.textContent).not.toContain("28m");
  });
});
