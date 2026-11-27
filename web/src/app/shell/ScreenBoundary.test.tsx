import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { reloadPage } from "./pageReload";
import { ScreenBoundary } from "./ScreenBoundary";

vi.mock("./pageReload", () => ({ reloadPage: vi.fn() }));

/** A screen whose chunk never arrived: React throws the rejected import to the nearest boundary. */
const NoChunk = () => {
  throw new Error("Failed to fetch dynamically imported module");
};

afterEach(() => {
  vi.restoreAllMocks();
});

describe("the boundary a deferred screen lands in", () => {
  it("draws the screen when its code arrives", () => {
    render(
      <ScreenBoundary>
        <p>Pulls</p>
      </ScreenBoundary>,
    );
    expect(screen.getByText("Pulls")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("answers a failed import with the error state and a reload instead of a blank window", () => {
    // React reports the error it caught; the test states the boundary's answer, not the log.
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(
      <ScreenBoundary>
        <NoChunk />
      </ScreenBoundary>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("This screen did not open");
    expect(screen.queryByText("Pulls")).toBeNull();
    screen.getByRole("button", { name: "Retry" }).click();
    expect(vi.mocked(reloadPage)).toHaveBeenCalledTimes(1);
  });
});
