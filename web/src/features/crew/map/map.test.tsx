import { act, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { HOSTILE_BODY, HOSTILE_VALUE } from "../../../../mock/map";
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
    expect(rows[1]).toHaveTextContent("1 unread");
    expect(rows[1]).not.toHaveTextContent("Subagents not reported");
    expect(rows[2]).toHaveTextContent("Verifying");
    expect(rows[2]).toHaveTextContent("Subagents not reported");
    expect(rows[3]).toHaveTextContent("Subagents not reported");
    // Only workers hang under the lead.
    expect(rows.map((one) => one.hasAttribute("data-worker"))).toEqual([false, true, true, true]);
    // The routes carry no tokens, level or blocked state, so none is drawn.
    expect(document.body.textContent).not.toMatch(/tokens|Blocked|\bL[0-2]\b|Open thread[^A]/);
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
    await user.click(row(/claude\s*Opus/));
    const lines = within(await screen.findByRole("list", { name: "Messages" })).getAllByRole("listitem").map((one) => one.textContent ?? "");
    const answer = lines.find((line) => line.includes("an answer"))!;
    expect(answer).toMatch(/lead → all · an answer$/);
    expect(lines.some((line) => /held$/.test(line))).toBe(true);
    expect(lines.some((line) => /tool result$/.test(line))).toBe(true);
    expect(lines.some((line) => /failed$/.test(line))).toBe(true);
  });

  it("draws the member menu: Cancel and Re-run checks inert (the map carries no flags) and the rest After the beta", async () => {
    const user = await openMap();
    await user.click(row(/Use next in the auth redirect/));
    await user.click(await screen.findByRole("button", { name: "Actions for this member" }));
    const menu = await screen.findByRole("menu", { name: "Member actions" });
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((item) => item.textContent)).toEqual([
      "Open thread After the beta",
      "Cancel Use the Board card",
      "Re-run checks Use the Board card",
      "Reassign engine After the beta",
      "Message into a lane After the beta",
      "Add reviewer lane After the beta",
      "Propose a check After the beta",
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
    expect(within(row(/Which target wins/)).getByText("Subagents not reported")).toBeVisible();
    expect(tree().getAllByRole("listitem")).toHaveLength(2);
  });

  it("draws a Claude worker, which reports its subagents, with no not-reported line", async () => {
    await openMap("crew-map-deep");
    expect(within(row(/Use next in the auth redirect/)).queryByText("Subagents not reported")).toBeNull();
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

  it("never draws a message body, a fact value or a thread a hostile service adds to its answers", async () => {
    const user = await openMap("crew-map-hostile");
    await user.click(row(/Add tests for the redirect/));
    await screen.findByRole("list", { name: "Messages" });
    expect(document.body.textContent).not.toContain(HOSTILE_BODY);
    expect(document.body.textContent).not.toContain(HOSTILE_VALUE);
    expect(document.body.innerHTML).not.toContain(HOSTILE_BODY);
  });
});

describe("the conversation lanes on the message and fact routes", () => {
  async function openLanes(scenario: "crew" | `crew-${string}` = "crew") {
    mock.reset(scenario);
    const user = userEvent.setup();
    render(<LanesView />);
    await screen.findByRole("list", { name: "Lanes" });
    return user;
  }

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
    expect(within(screen.getByRole("list", { name: "Lanes" })).getAllByRole("listitem")).toHaveLength(4);
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
    expect(within(screen.getByRole("list", { name: "Lanes" })).getAllByRole("listitem").length).toBeGreaterThan(1);
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

  it("never draws a message body or a fact value a hostile service adds", async () => {
    await openLanes("crew-map-hostile");
    await screen.findByRole("list", { name: "Facts" });
    expect(document.body.textContent).not.toContain(HOSTILE_BODY);
    expect(document.body.textContent).not.toContain(HOSTILE_VALUE);
  });

  it("names an unreadable service and retries with reads only", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/flow/map", 1);
    const user = userEvent.setup();
    render(<LanesView />);
    expect(await screen.findByText("Can’t read the lanes")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("list", { name: "Lanes" })).toBeVisible();
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
