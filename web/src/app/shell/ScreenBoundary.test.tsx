import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { reloadPage } from "./pageReload";
import { ScreenBoundary } from "./ScreenBoundary";

vi.mock("./pageReload", () => ({ reloadPage: vi.fn() }));

/** A screen whose chunk never arrived: React throws the rejected import to the nearest boundary. */
const NoChunk = () => {
  throw new Error("Failed to fetch dynamically imported module");
};

afterEach(() => {
  document.querySelectorAll("dialog").forEach((dialog) => dialog.remove());
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

  it("reports the address whose screen failed, so focus follows the error state", () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const onScreen = vi.fn();
    render(
      <ScreenBoundary screenKey="#/pulls" onScreen={onScreen}>
        <NoChunk />
      </ScreenBoundary>,
    );
    expect(onScreen).toHaveBeenCalledWith("#/pulls");
  });

  it("closes Settings on Escape when the screen that answers the key never arrived", () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const onClose = vi.fn();
    render(
      <ScreenBoundary screenKey="#/settings" onScreen={vi.fn()} onClose={onClose}>
        <NoChunk />
      </ScreenBoundary>,
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("leaves Escape to a dialog that owns it, and to every other key", () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const dialog = document.createElement("dialog");
    dialog.setAttribute("open", "");
    document.body.append(dialog);
    const onClose = vi.fn();
    render(
      <ScreenBoundary screenKey="#/settings" onScreen={vi.fn()} onClose={onClose}>
        <NoChunk />
      </ScreenBoundary>,
    );
    fireEvent.keyDown(document, { key: "Escape" });
    fireEvent.keyDown(document, { key: "Enter" });
    expect(onClose).not.toHaveBeenCalled();
  });

  it("leaves Escape to the sidebar's thread search, which answers it without claiming the key", () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    // The search field is beside Settings rather than under it, and it clears itself on Escape without
    // calling preventDefault, so the page that would have answered the key keeps the same exclusion.
    const search = document.createElement("input");
    search.type = "search";
    search.setAttribute("aria-label", "Search threads");
    document.body.append(search);
    const onClose = vi.fn();
    render(
      <ScreenBoundary screenKey="#/settings" onScreen={vi.fn()} onClose={onClose}>
        <NoChunk />
      </ScreenBoundary>,
    );
    fireEvent.keyDown(search, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
    search.remove();
  });

  it("leaves Escape to an open menu", () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const menu = document.createElement("div");
    menu.setAttribute("role", "menu");
    const item = document.createElement("button");
    menu.append(item);
    document.body.append(menu);
    const onClose = vi.fn();
    render(
      <ScreenBoundary screenKey="#/settings" onScreen={vi.fn()} onClose={onClose}>
        <NoChunk />
      </ScreenBoundary>,
    );
    fireEvent.keyDown(item, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
    menu.remove();
  });
});
