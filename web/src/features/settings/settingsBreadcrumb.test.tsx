import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { installMockFetch } from "../../test-support";
import { SettingsPage } from "./SettingsPage";

beforeEach(() => {
  installMockFetch();
  window.location.hash = "";
});

const crumbs = () => screen.getByRole("navigation", { name: "Breadcrumb" });

describe("the Settings breadcrumb (K55)", () => {
  it("names the open page under Settings, the current page marked and not a link", async () => {
    render(<SettingsPage section="chat" onClose={() => undefined} />);
    const nav = await screen.findByRole("navigation", { name: "Breadcrumb" });
    const settings = within(nav).getByRole("link", { name: "Settings" });
    expect(settings).toHaveAttribute("href", "#/settings/general");
    const here = within(nav).getByText("Chat");
    expect(here).toHaveAttribute("aria-current", "page");
    expect(within(nav).queryByRole("link", { name: "Chat" })).toBeNull();
    // The section list keeps its own landmark and its own current page.
    expect(screen.getByRole("navigation", { name: "Settings sections" })).toBeVisible();
    expect(screen.getByRole("link", { name: "Chat" })).toHaveAttribute("aria-current", "page");
  });

  it("is a reachable link by keyboard and goes to the first page", async () => {
    const user = userEvent.setup();
    render(<SettingsPage section="chat" onClose={() => undefined} />);
    const link = within(await screen.findByRole("navigation", { name: "Breadcrumb" })).getByRole("link", { name: "Settings" });
    link.focus();
    expect(link).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(window.location.hash).toBe("#/settings/general");
  });

  it("shows the whole path on a sub-page, each ancestor a link back, and no back link on a wide window", async () => {
    render(<SettingsPage section="appearance" sub="theme-editor" onClose={() => undefined} />);
    const nav = await screen.findByRole("navigation", { name: "Breadcrumb" });
    expect(within(nav).getByRole("link", { name: "Settings" })).toBeVisible();
    expect(within(nav).getByRole("link", { name: "Appearance" })).toHaveAttribute("href", "#/settings/appearance");
    expect(within(nav).getByText("Theme editor")).toHaveAttribute("aria-current", "page");
    expect(screen.queryByRole("link", { name: /^Back to/ })).toBeNull();
    // The editor itself draws its controls without its own back link.
    expect(await screen.findByRole("button", { name: "Import…" })).toBeVisible();
  });

  it("opens the Theme editor from the Appearance row and leaves it through the breadcrumb", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<SettingsPage section="appearance" onClose={() => undefined} />);
    expect(within(await screen.findByRole("navigation", { name: "Breadcrumb" })).getByText("Appearance")).toHaveAttribute("aria-current", "page");
    await user.click(await screen.findByRole("button", { name: "Edit…" }));
    expect(window.location.hash).toBe("#/settings/appearance/theme-editor");
    rerender(<SettingsPage section="appearance" sub="theme-editor" onClose={() => undefined} />);
    await user.click(within(crumbs()).getByRole("link", { name: "Appearance" }));
    expect(window.location.hash).toBe("#/settings/appearance");
  });

  it("keeps the back link, and no breadcrumb, on a sub-page in a narrow window", async () => {
    const original = globalThis.ResizeObserver;
    class Narrow {
      constructor(private readonly cb: ResizeObserverCallback) {}
      observe() { this.cb([{ contentRect: { width: 600 } } as ResizeObserverEntry], this as unknown as ResizeObserver); }
      disconnect() {}
      unobserve() {}
    }
    vi.stubGlobal("ResizeObserver", Narrow);
    try {
      render(<SettingsPage section="appearance" sub="theme-editor" onClose={() => undefined} />);
      const back = await screen.findByRole("link", { name: "Back to Appearance" });
      expect(back).toHaveAttribute("href", "#/settings/appearance");
      expect(screen.queryByRole("navigation", { name: "Breadcrumb" })).toBeNull();
    } finally {
      vi.stubGlobal("ResizeObserver", original);
    }
  });
});

describe("an address Settings does not have (K55)", () => {
  it("opens General with a note that names the page, and the breadcrumb names General", async () => {
    render(<SettingsPage section={null} unknown="voice" onClose={() => undefined} />);
    const note = await screen.findByRole("status");
    expect(note).toHaveTextContent("There is no setting called “voice” in this app. Showing General instead.");
    expect(within(crumbs()).getByText("General")).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");
  });

  it("is not left empty, and Dismiss only closes the note", async () => {
    const user = userEvent.setup();
    render(<SettingsPage section={null} unknown="voice" onClose={() => undefined} />);
    await user.click(await screen.findByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText(/There is no setting called/)).toBeNull();
    expect(screen.getByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");
  });

  it("draws a hostile or long page name as plain, shortened text", async () => {
    render(<SettingsPage section={null} unknown={`<img src=x onerror=alert(1)>${"a".repeat(80)}`} onClose={() => undefined} />);
    const note = await screen.findByRole("status");
    expect(note.querySelector("img")).toBeNull();
    expect(note.textContent).toContain("<img src=x");
    expect(note.textContent!.length).toBeLessThan(140);
  });

  it("shows no note for the bare Settings address", async () => {
    render(<SettingsPage section={null} onClose={() => undefined} />);
    await screen.findByRole("navigation", { name: "Breadcrumb" });
    expect(screen.queryByText(/There is no setting called/)).toBeNull();
  });
});

describe("an address naming a page that is off on this device (pb-k55-gated)", () => {
  it("opens General with a note that says the page is off, not that it does not exist", async () => {
    render(<SettingsPage section="crew" onClose={() => undefined} />);
    const note = await screen.findByRole("status");
    expect(note).toHaveTextContent("That page is off on this device: “Crew”. Showing General instead.");
    expect(screen.queryByText(/There is no setting called/)).toBeNull();
    expect(within(crumbs()).getByText("General")).toHaveAttribute("aria-current", "page");
    expect(screen.queryByRole("link", { name: "Crew" })).toBeNull();
  });

  it("says the same for Account while sign-in is off, and Dismiss only closes the note", async () => {
    const user = userEvent.setup();
    render(<SettingsPage section="account" onClose={() => undefined} />);
    expect(await screen.findByRole("status")).toHaveTextContent("That page is off on this device: “Account”.");
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText(/That page is off/)).toBeNull();
    expect(screen.getByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");
  });

  it("shows no note for a page that is on", async () => {
    render(<SettingsPage section="chat" onClose={() => undefined} />);
    await screen.findByRole("navigation", { name: "Breadcrumb" });
    expect(screen.queryByText(/off on this device/)).toBeNull();
  });
});
