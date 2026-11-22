import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { installMockFetch } from "../../../test-support";
import { CommandPalette } from "./CommandPalette";

beforeEach(() => {
  installMockFetch();
  window.localStorage.clear();
});

async function search(text: string) {
  const user = userEvent.setup();
  render(<CommandPalette start="root" onClose={() => undefined} />);
  await user.type(await screen.findByRole("combobox"), text);
  return user;
}

describe("Settings in the command palette", () => {
  it("lists the Settings pages for the word settings, not only for a page's own name", async () => {
    await search("sett");
    expect(await screen.findByText("Settings")).toBeVisible();
    expect(screen.getByRole("button", { name: "General" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Keybindings" })).toBeVisible();
  });

  it("still finds a page by its own name, and keeps the Settings group first for that word", async () => {
    await search("keybindings");
    expect(await screen.findByRole("button", { name: "Keybindings" })).toBeVisible();
    expect(screen.getByText("Settings")).toBeVisible();
  });

  it("opens the page it lists when the row is picked", async () => {
    const user = await search("sett");
    await user.click(await screen.findByRole("button", { name: "Keybindings" }));
    expect(window.location.hash).toBe("#/settings/keybindings");
  });
});
