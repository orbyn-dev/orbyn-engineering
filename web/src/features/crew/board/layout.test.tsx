import { render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { BoardView } from "./BoardView";

/**
 * The Board's narrow layout and the states before it has a crew to draw. The wide board itself, its columns, cards and filters are
 * board.test.tsx.
 */
let mock: MockApi;
beforeEach(() => {
  // The card says how long ago a task changed: the clock stands at the moment the sample was taken.
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date("2026-11-07T15:00:00Z"));
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  window.location.hash = "";
});

afterEach(() => vi.useRealTimers());

const calls = (part: string) => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes(part));
const writes = () => vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method && init.method !== "GET");
const column = (name: RegExp) => screen.getByRole("region", { name });
const cardOf = (name: RegExp, id: string) => column(name).querySelector<HTMLElement>(`[data-task="${id}"]`)!;

/** Adds a field the mock scenario does not carry, by changing the board answer before the page reads it. */
function patchBoard(patch: (card: { id: string; reason?: string | null }) => void) {
  const real = vi.mocked(fetch).getMockImplementation()!;
  vi.mocked(fetch).mockImplementation(async (input, init) => {
    const response = await real(input, init);
    if ((init?.method ?? "GET") !== "GET" || !String(input).includes("/api/crew/flow/board")) return response;
    const body = (await response.json()) as { columns?: Array<{ cards?: Array<{ id: string; reason?: string | null }> }> };
    for (const held of body.columns ?? []) for (const card of held.cards ?? []) patch(card);
    return new Response(JSON.stringify(body), { status: response.status });
  });
}

/** A ResizeObserver that measures every element it watches at one width, as a panel that wide would report. */
function panelWidth(width: number) {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(private readonly send: ResizeObserverCallback) {}
      observe(target: Element) {
        this.send([{ target, contentRect: { width } } as unknown as ResizeObserverEntry], this as unknown as ResizeObserver);
      }
      unobserve() {}
      disconnect() {}
    },
  );
}

it("measures the 320 px panel: Needs you and Running open first, and the rest behind their counts", async () => {
  panelWidth(320);
  mock.reset("crew");
  render(<BoardView />);
  await screen.findByRole("region", { name: /^Queued/ });
  expect(within(column(/^Needs you/)).getByRole("button", { name: /Needs you/ })).toHaveAttribute("aria-expanded", "true");
  expect(within(column(/^Needs you/)).getByText("Which target wins when both next and returnTo are set?")).toBeVisible();
  expect(within(column(/^Running/)).getByRole("button", { name: /Running/ })).toHaveAttribute("aria-expanded", "true");
  expect(within(column(/^Running/)).getByText("Use next in the auth redirect when it points inside the app")).toBeVisible();
  for (const name of [/^Queued/, /^Routed/, /^Verifying/, /^Passed/, /^Stopped/]) {
    expect(within(column(name)).getByRole("button", { name: /,?\s*\d+$/ })).toHaveAttribute("aria-expanded", "false");
  }
  expect(writes()).toHaveLength(0);
});

/**
 * The built 320 px panel over the mock's `crew-board-needs-kinds` scenario, not the Planned `needs-one-source-board-narrow-panel`
 * frame: this proves the built state-group list is drawn at 320 px (a group open in the wide board is collapsed here, so the test
 * fails wide) and keeps the Needs you cards' kind, title, engine and the card's own reason. The Planned frame draws differently
 * (only Needs you open, and four cards including two the field has no kind for), so it is not claimed here.
 */
it("measures the 320 px panel: the Needs you group keeps the one field's words, kind, title and engine", async () => {
  panelWidth(320);
  mock.reset("crew-board-needs-kinds");
  // A reason of its own, so the line proves the card's reason is drawn rather than the kind's fixed words.
  patchBoard((card) => {
    if (card.id === "b-manual") card.reason = "A distinct check only a person can make.";
  });
  render(<BoardView />);
  await screen.findByRole("region", { name: /^Queued/ });
  // Narrow, not wide: Queued is open in the wide board and collapsed in the panel.
  expect(document.querySelector(".bd-body")).toHaveAttribute("data-narrow");
  expect(within(column(/^Queued/)).getByRole("button", { name: /,?\s*\d+$/ })).toHaveAttribute("aria-expanded", "false");
  const needs = within(column(/^Needs you/));
  expect(needs.getByRole("button", { name: /Needs you/ })).toHaveAttribute("aria-expanded", "true");
  const question = cardOf(/^Needs you/, "b-question");
  expect(within(question).getByText("research")).toBeVisible();
  expect(within(question).getByText("Which target wins when both next and returnTo are set?")).toBeVisible();
  expect(within(question).getByText("opencode")).toBeVisible();
  expect(question.querySelector(".bd-amber")).toHaveTextContent("Asked you a question · Which target wins when both next and returnTo are set?");
  expect(cardOf(/^Needs you/, "b-merge").querySelector(".bd-amber")).toHaveTextContent("The merge waits for you");
  expect(cardOf(/^Needs you/, "b-manual").querySelector(".bd-amber")).toHaveTextContent("A distinct check only a person can make.");
  expect(writes()).toHaveLength(0);
});

describe("the Taskboard before it has a crew", () => {
  it("says it is reading the board until the crew list answers", async () => {
    mock.reset("crew");
    const real = vi.mocked(fetch).getMockImplementation()!;
    let answer = () => {};
    const answered = new Promise<void>((resolve) => (answer = resolve));
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (String(input).includes("/api/crew/flow/crews")) await answered;
      return real(input, init);
    });
    render(<BoardView />);
    expect(await screen.findByText("Reading the board")).toBeVisible();
    expect(screen.queryByRole("region", { name: /^Queued/ })).toBeNull();
    answer();
    expect(await screen.findByRole("region", { name: /^Queued/ })).toBeVisible();
    expect(calls("/api/crew/flow/crews")).toHaveLength(1);
  });

  it("says No crews running when the service lists none, and reads no board", async () => {
    mock.reset("crew-empty");
    render(<BoardView />);
    expect(await screen.findByText("No crews running")).toBeVisible();
    expect(screen.queryByRole("region", { name: /^Queued/ })).toBeNull();
    await waitFor(() => expect(calls("/api/crew/flow/crews")).toHaveLength(1));
    expect(calls("/api/crew/flow/board")).toHaveLength(0);
    expect(writes()).toHaveLength(0);
  });
});
