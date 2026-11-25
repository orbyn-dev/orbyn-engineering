import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { installMockFetch } from "../../test-support";
import { SettingsPage } from "./SettingsPage";

beforeEach(() => {
  installMockFetch();
  window.location.hash = "";
  Element.prototype.scrollIntoView = vi.fn();
});

describe("the phone settings chip row (pb-k55-chips)", () => {
  it("reveals the chip that keyboard focus reaches, not only the open page", async () => {
    render(<SettingsPage section="general" onClose={() => undefined} />);
    const nav = await screen.findByRole("navigation", { name: "Settings sections" });
    const last = within(nav).getByRole("link", { name: "Archive" });
    vi.mocked(Element.prototype.scrollIntoView).mockClear();
    last.focus();
    // The browser scrolls a focused chip only when it is wholly outside the row; a chip the end of the row
    // cuts in half stays cut, so the row reveals the chip itself.
    expect(Element.prototype.scrollIntoView).toHaveBeenCalledTimes(1);
    expect(Element.prototype.scrollIntoView).toHaveBeenLastCalledWith({ block: "nearest", inline: "nearest" });
  });
});
