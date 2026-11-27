import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { MapView } from "./MapView";

/** KC3 on the Map, black-box through the rendered view; only the network is faked (the mock's crew-map scenarios). */
let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  window.location.hash = "";
});

async function open(scenario: "crew" | `crew-${string}`) {
  mock.reset(scenario);
  render(<MapView />);
  await screen.findByRole("list", { name: "Crew" });
}
const rowOf = (task: string) => screen.getByText(task).closest("button") as HTMLElement;

describe("the Map's Working word", () => {
  it("says Working with its why under the task, and counts workers, those working and those that need you", async () => {
    await open("crew-map-working");
    expect(screen.getByTestId("map-summary")).toHaveTextContent("4 workers · 3 working · 1 needs you");

    const retry = rowOf("Use next in the auth redirect when it points inside the app");
    expect(within(retry).getByText("Working")).toBeInTheDocument();
    expect(within(retry).getByText("Retrying after a failed check")).toBeInTheDocument();
    expect(within(retry).queryByText("Running")).toBeNull();

    const checks = rowOf("Add tests for the redirect, including //evil.com");
    expect(within(checks).getByText("Working")).toBeInTheDocument();
    expect(within(checks).getByText("Checks running")).toBeInTheDocument();
    expect(within(checks).queryByText("Verifying")).toBeNull();

    // A plain run is Working with no why line.
    const plain = rowOf("Which target wins when both next and returnTo are set?");
    expect(within(plain).getByText("Working")).toBeInTheDocument();
    expect(plain.querySelector(".mp-why")).toBeNull();

    // The waiting worker keeps its own word and has no why line.
    const waiting = rowOf("Which tone should the guide take?");
    expect(within(waiting).getByText("Needs you")).toBeInTheDocument();
    expect(waiting.querySelector(".mp-why")).toBeNull();
    expect(document.body.textContent).not.toMatch(/\bIdle\b/);
  });

  it("keeps Running and Verifying when the service sends no working field", async () => {
    await open("crew");
    expect(screen.getByTestId("map-summary")).toHaveTextContent("3 workers · 0 working · 0 need you");
    expect(screen.getAllByText("Running").length).toBeGreaterThan(0);
    expect(screen.getByText("Verifying")).toBeInTheDocument();
    expect(document.querySelector(".mp-why")).toBeNull();
  });

  it("draws no summary for a crew with no worker", async () => {
    mock.reset("crew-map-empty");
    render(<MapView />);
    await screen.findByText("No workers yet");
    expect(screen.queryByTestId("map-summary")).toBeNull();
  });

  it("uses the singular for one worker", async () => {
    await open("crew-map-one");
    expect(screen.getByTestId("map-summary")).toHaveTextContent("1 worker · 0 working · 0 need you");
  });
});
