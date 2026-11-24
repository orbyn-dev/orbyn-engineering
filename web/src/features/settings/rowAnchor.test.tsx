import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { installMockFetch } from "../../test-support";
import { SettingsPage } from "./SettingsPage";

beforeEach(() => {
  installMockFetch();
  window.location.hash = "";
  Element.prototype.scrollIntoView = vi.fn();
});

const ringed = () => document.querySelector<HTMLElement>('[data-anchored="true"]');

describe("a deep link to a row (pb-k55-row-anchor)", () => {
  it("opens the page, scrolls the row to the middle, focuses it and rings it", async () => {
    render(<SettingsPage section="chat" row="search-engine" onClose={() => undefined} />);
    await waitFor(() => expect(ringed()).not.toBeNull());
    const row = ringed()!;
    expect(row.id).toBe("chat-search-engine");
    expect(row).toHaveFocus();
    expect(Element.prototype.scrollIntoView).toHaveBeenCalledWith({ block: "center" });
    expect(screen.getByRole("link", { name: "Chat" })).toHaveAttribute("aria-current", "page");
  });

  it("keeps the ring with no timer, and takes it away on the next click", async () => {
    const user = userEvent.setup();
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      render(<SettingsPage section="chat" row="search-engine" onClose={() => undefined} />);
      await waitFor(() => expect(ringed()).not.toBeNull());
      await act(async () => { vi.advanceTimersByTime(60_000); });
      expect(ringed()).not.toBeNull();
      await user.click(document.body);
      expect(ringed()).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("takes the ring away on the next key", async () => {
    const user = userEvent.setup();
    render(<SettingsPage section="chat" row="search-engine" onClose={() => undefined} />);
    await waitFor(() => expect(ringed()).not.toBeNull());
    await user.keyboard("{Shift}");
    expect(ringed()).toBeNull();
  });

  it("takes the ring away when the row loses focus", async () => {
    render(<SettingsPage section="chat" row="search-engine" onClose={() => undefined} />);
    await waitFor(() => expect(ringed()).not.toBeNull());
    act(() => ringed()!.blur());
    expect(ringed()).toBeNull();
  });

  it("rings nothing for a row the page does not have, or a name that is only a property name", async () => {
    for (const row of ["no-such-row", "constructor", "search-engine-x"]) {
      const { unmount } = render(<SettingsPage section="chat" row={row} onClose={() => undefined} />);
      await screen.findByRole("navigation", { name: "Breadcrumb" });
      // Real wait: asserts that nothing more happens after a pause; there is no event to wait for.
      await new Promise((resolve) => setTimeout(resolve, 30));
      expect(ringed()).toBeNull();
      unmount();
    }
  });

  it("does not ring a row of another page", async () => {
    render(<SettingsPage section="general" row="search-engine" onClose={() => undefined} />);
    await screen.findByRole("navigation", { name: "Breadcrumb" });
    // Real wait: asserts that nothing more happens after a pause; there is no event to wait for.
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(ringed()).toBeNull();
  });
});
