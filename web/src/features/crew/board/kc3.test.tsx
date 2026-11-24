import { render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { FlowCard } from "../../../generated/FlowCard";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { useWrites } from "../writes/store";
import { BoardView } from "./BoardView";

/**
 * KC3's Taskboard words, black-box through the rendered Board: the Working line, the amber needs-you sentence by kind and the
 * empty Needs you column. Only the network is faked (installMockFetch and the mock's own board answer); when a scenario does not
 * carry a field the words need, the test changes that answer at the same network edge, never a page module.
 */
let mock: MockApi;
beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-07T15:00:00Z"));
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  useWrites.setState({ ops: {} });
  window.location.hash = "";
});

afterEach(() => vi.useRealTimers());

const column = (name: RegExp) => screen.getByRole("region", { name });
const cardOf = (name: RegExp, id: string) => column(name).querySelector<HTMLElement>(`[data-task="${id}"]`)!;

/** A card the test may change: the needs-you kind is loosened so a value a later service sends can be exercised too. */
type PatchCard = Omit<FlowCard, "needs_you"> & { needs_you?: string | null };

/** Adds fields a scenario does not carry, by changing the mock's answer before the page reads it. */
function patchBoard(patch: (card: PatchCard) => void) {
  const real = vi.mocked(fetch).getMockImplementation()!;
  vi.mocked(fetch).mockImplementation(async (input, init) => {
    const response = await real(input, init);
    if ((init?.method ?? "GET") !== "GET" || !String(input).includes("/api/crew/flow/board")) return response;
    const body = (await response.json()) as { columns?: Array<{ cards?: PatchCard[] }> };
    for (const held of body.columns ?? []) for (const card of held.cards ?? []) patch(card);
    return new Response(JSON.stringify(body), { status: response.status });
  });
}

async function open(scenario: "crew" | `crew-${string}`, patch?: (card: PatchCard) => void) {
  mock.reset(scenario);
  if (patch) patchBoard(patch);
  render(<BoardView />);
  await screen.findByRole("region", { name: /^Queued/ });
}

describe("the Taskboard's KC3 words", () => {
  it("draws every Working line on its card, and none on a card whose worker owns nothing", async () => {
    const lines = ["running", "verifying", "retry", "fix"] as const;
    await open("crew-board-many", (card) => {
      const index = ["b-m0", "b-m1", "b-m2", "b-m3"].indexOf(card.id);
      if (index >= 0) card.working = lines[index]!;
    });
    const running = within(column(/^Running/));
    expect(cardOf(/^Running/, "b-m0").querySelector(".bd-working")).toHaveTextContent(/^Working$/);
    expect(cardOf(/^Running/, "b-m1").querySelector(".bd-working")).toHaveTextContent("Working · checks running");
    expect(cardOf(/^Running/, "b-m2").querySelector(".bd-working")).toHaveTextContent("Working · retrying after a failed check");
    expect(cardOf(/^Running/, "b-m3").querySelector(".bd-working")).toHaveTextContent("Working · fixing a review finding");
    // Twelve running tasks, four with a working field: the cards drawn without one show no line.
    expect(running.getAllByText(/^Working/)).toHaveLength(4);
  });

  it("draws the retry and verifying lines the working frame carries", async () => {
    await open("crew-board-working");
    expect(cardOf(/^Running/, "b-running").querySelector(".bd-working")).toHaveTextContent("Working · retrying after a failed check");
    expect(cardOf(/^Verifying/, "b-verifying").querySelector(".bd-working")).toHaveTextContent("Working · checks running");
  });

  it("draws no Working line on a waiting card even when its worker field is set", async () => {
    await open("crew-board-needs-kinds", (card) => {
      if (card.id === "b-merge") card.working = "running";
    });
    expect(column(/^Needs you/).querySelector(".bd-working")).toBeNull();
  });

  it("says nothing waits for you in an open Needs you column with no card", async () => {
    await open("crew-board-working");
    const line = "Nothing waits for you. The Inbox, the Review queue and the sidebar say the same.";
    expect(within(column(/^Needs you/)).getByText(line)).toBeVisible();
    expect(document.querySelectorAll(".bd-empty")).toHaveLength(1);
  });

  it("does not say nothing waits for you when a card waits for you", async () => {
    await open("crew");
    expect(screen.queryByText(/Nothing waits for you/)).toBeNull();
    expect(column(/^Needs you/).querySelector(".bd-empty")).toBeNull();
  });

  it("draws the sentence each needs-you kind gives, with the question in its line", async () => {
    await open("crew-board-needs-kinds");
    const question = cardOf(/^Needs you/, "b-question").querySelector(".bd-amber")!;
    expect(question).toHaveTextContent("Asked you a question · Which target wins when both next and returnTo are set?");
    expect(question.querySelector("b")).toHaveTextContent("Asked you a question");
    const merge = cardOf(/^Needs you/, "b-merge").querySelector(".bd-amber")!;
    expect(merge).toHaveTextContent("The merge waits for you");
    expect(merge).not.toHaveTextContent("A merge needs your decision.");
    expect(cardOf(/^Needs you/, "b-manual").querySelector(".bd-amber")).toHaveTextContent("A check only a person can make is waiting.");
  });

  it("falls back to the card's reason for a kind it does not know and for no kind", async () => {
    await open("crew-board-needs-kinds", (card) => {
      if (card.id === "b-merge") {
        // A kind this page does not know is no wait by itself: the card reads as waiting only through its run state.
        card.needs_you = "future_kind";
        card.state = "needs_person";
      }
      if (card.id === "b-question") card.needs_you = null;
    });
    expect(cardOf(/^Needs you/, "b-merge").querySelector(".bd-amber")).toHaveTextContent("A merge needs your decision.");
    expect(cardOf(/^Needs you/, "b-question").querySelector(".bd-amber")).toHaveTextContent("Which target wins when both next and returnTo are set?");
  });
});
