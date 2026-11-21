import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { installMockFetch } from "../../test-support";
import { SETTINGS_SECTIONS } from "../../product/state/route";
import { SettingsPage, labels } from "./SettingsPage";

beforeEach(() => {
  installMockFetch();
  window.location.hash = "";
});

/** Pages that are off on this device open General instead, with a note that says so (pb-k55-gated). */
const OFF = new Set(["account", "crew"]);

const headings = () => screen.getAllByRole("heading");

describe("one h1 per Settings page", () => {
  it.each(SETTINGS_SECTIONS)("%s owns the only h1, and it is the page title", async (section) => {
    render(<SettingsPage section={section} onClose={() => undefined} />);
    const h1s = await screen.findAllByRole("heading", { level: 1 });
    expect(h1s).toHaveLength(1);
    const title = h1s[0];
    expect(title).toBeDefined();
    if (!title) return;
    expect(title).toHaveAttribute("data-page-title");
    expect(title).not.toHaveTextContent("");
    expect(title).toHaveTextContent(OFF.has(section) ? labels.general : labels[section]);
    // The page title is the settings page's own, not a section's and not a heading left over behind it.
    expect(title.closest(".settings-main")).not.toBeNull();
    expect(title.closest(".settings-body")).toBeNull();
    expect(title.closest("[inert], [hidden]")).toBeNull();
  });

  it.each(SETTINGS_SECTIONS)("%s keeps every other heading at h2 or deeper", async (section) => {
    render(<SettingsPage section={section} onClose={() => undefined} />);
    await screen.findAllByRole("heading", { level: 1 });
    for (const heading of headings()) {
      if (heading.tagName === "H1") continue;
      expect(Number(heading.tagName.slice(1)), `${section}: ${heading.textContent}`).toBeGreaterThanOrEqual(2);
    }
  });

  it("names the open sub-page as the h1, and leaves the page it came from with no h1 of its own", async () => {
    render(<SettingsPage section="appearance" sub="theme-editor" onClose={() => undefined} />);
    const h1s = await screen.findAllByRole("heading", { level: 1 });
    expect(h1s).toHaveLength(1);
    expect(h1s[0]?.textContent).toBe("Theme editor");
  });});
