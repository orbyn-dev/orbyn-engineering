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
 * The KC3 320 px panel frame (`needs-one-source-board-narrow-panel`): the same Needs you cards in the panel's state-group list,
 * each keeping its kind, its title, the engine and the one amber sentence. The panel's arrangement is
 * `crew-board-narrow-panel`'s (Needs you and Running open); this measures the KC3 words in it, so the design pass that flips
 * that frame has a width test to cite.
 */
it("measures the 320 px panel: the Needs you group keeps the one field's words, kind, title and engine", async () => {
  panelWidth(320);
  mock.reset("crew-board-needs-kinds");
  render(<BoardView />);
  await screen.findByRole("region", { name: /^Queued/ });
  const needs = within(column(/^Needs you/));
  expect(needs.getByRole("button", { name: /Needs you/ })).toHaveAttribute("aria-expanded", "true");
  const question = cardOf(/^Needs you/, "b-question");
  expect(within(question).getByText("research")).toBeVisible();
  expect(within(question).getByText("Which target wins when both next and returnTo are set?")).toBeVisible();
  expect(within(question).getByText("opencode")).toBeVisible();
  expect(question.querySelector(".bd-amber")).toHaveTextContent("Asked you a question · Which target wins when both next and returnTo are set?");
  expect(cardOf(/^Needs you/, "b-merge").querySelector(".bd-amber")).toHaveTextContent("The merge waits for you");
  expect(cardOf(/^Needs you/, "b-manual").querySelector(".bd-amber")).toHaveTextContent("A check only a person can make is waiting.");
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
