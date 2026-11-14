import { render, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CrewTabs } from "./CrewTabs";

const TABS = [
  { id: "approvals", label: "Approvals" },
  { id: "trust", label: "Trust" },
  { id: "background", label: "Background" },
  { id: "inbox", label: "Inbox" },
  { id: "flow", label: "Flow" },
] as const;

/** jsdom has no layout: every tab is 80 px wide and the strip is `strip` px. */
function layout(strip: number) {
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("role") === "tab" ? 80 : 0;
  });
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
    return this.classList.contains("ap-tabs") ? strip : 0;
  });
}
afterEach(() => vi.restoreAllMocks());

const shown = () => screen.getAllByRole("tab").filter((tab) => !tab.hasAttribute("aria-hidden")).map((tab) => tab.textContent);

describe("the Crew view tabs at a narrow width", () => {
  it("shows every tab, and no ⋯, when they all fit", () => {
    layout(600);
    render(<CrewTabs tabs={TABS} active="approvals" onSelect={() => undefined} base="t" />);
    expect(shown()).toEqual(["Approvals", "Trust", "Background", "Inbox", "Flow"]);
    expect(screen.queryByRole("button", { name: "More Crew views" })).toBeNull();
  });

  it("keeps the active tab visible, puts the rest behind the ⋯, and choosing one from the menu selects it", async () => {
    layout(300);
    const onSelect = vi.fn();
    const user = userEvent.setup();
    const view = render(<CrewTabs tabs={TABS} active="flow" onSelect={onSelect} base="t" />);
    expect(shown()).toEqual(["Approvals", "Flow"]);
    expect(screen.getByRole("tab", { name: "Flow" })).toHaveAttribute("aria-selected", "true");
    // The ⋯ is a button beside the tab list, not an element inside it.
    const more = screen.getByRole("button", { name: "More Crew views" });
    expect(more).toHaveAttribute("aria-haspopup", "menu");
    expect(screen.getByRole("tablist", { name: "Crew" })).not.toContainElement(more);
    await user.click(more);
    const menu = await screen.findByRole("menu", { name: "More Crew views" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Trust", "Background", "Inbox"]);
    await user.click(within(menu).getByRole("menuitem", { name: "Inbox" }));
    expect(onSelect).toHaveBeenCalledWith("inbox");
    view.rerender(<CrewTabs tabs={TABS} active="inbox" onSelect={onSelect} base="t" />);
    expect(shown()).toContain("Inbox");
  });

  it("puts focus on the tab chosen from the menu, not on the page", async () => {
    layout(300);
    const user = userEvent.setup();
    function Harness() {
      const [active, setActive] = useState<(typeof TABS)[number]["id"]>("approvals");
      return <CrewTabs tabs={TABS} active={active} onSelect={setActive} base="t" />;
    }
    render(<Harness />);
    await user.click(screen.getByRole("button", { name: "More Crew views" }));
    await user.click(within(await screen.findByRole("menu", { name: "More Crew views" })).getByRole("menuitem", { name: "Flow" }));
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Flow" })));
  });

  it("moves between the visible tabs with the arrow keys", async () => {
    layout(300);
    const onSelect = vi.fn();
    const user = userEvent.setup();
    render(<CrewTabs tabs={TABS} active="approvals" onSelect={onSelect} base="t" />);
    screen.getByRole("tab", { name: "Approvals" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(onSelect).toHaveBeenLastCalledWith("trust");
    await user.keyboard("{End}");
    expect(onSelect).toHaveBeenLastCalledWith("trust");
  });
});
