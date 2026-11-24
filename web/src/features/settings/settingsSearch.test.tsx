import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import { useRoute } from "../../product/state/route";
import { installMockFetch } from "../../test-support";
import { SettingsPage } from "./SettingsPage";
import { SETTINGS, searchSettings } from "./settingsSearch";

it("finds a specific label regardless of word order and excludes desktop controls on web", () => {
  expect(searchSettings("prefix BRANCH", false).map((item) => item.title)).toEqual(["Branch prefix"]);
  expect(searchSettings("command line tool", false)).toEqual([]);
  expect(searchSettings("command line tool", true)).toHaveLength(1);
  expect(searchSettings("  ", true)).toEqual([]);
  expect(searchSettings("browser profiles", false).map((item) => item.section)).toEqual(["browser"]);
  expect(searchSettings("Accessibility", true, new Set(["cli"]))).toEqual([]);
  expect(searchSettings("Accessibility", true, new Set(["accessibility"]))).toHaveLength(1);
  expect(searchSettings("App updates", true, new Set())).toEqual([]);
  expect(searchSettings("Worktree location", true)).toEqual([]);
  expect(searchSettings("free disk space", false).map((item) => item.section)).toEqual(["crew"]);
  expect(new Set(SETTINGS.map((item) => item.id)).size).toBe(SETTINGS.length);
});

it("finds the macOS-only quit confirmation on macOS and nowhere else", () => {
  expect(searchSettings("quit confirmation", true, undefined, true).map((item) => item.id)).toEqual(["general-quit-confirmation"]);
  expect(searchSettings("quit confirmation", true, undefined, false)).toEqual([]);
  expect(searchSettings("quit confirmation", true)).toEqual([]);
});

it("navigates from an exact result to an asynchronously loaded row", async () => {
  const user = userEvent.setup();
  installMockFetch();
  window.location.hash = "#/settings/general";
  function Harness() {
    const route = useRoute();
    return <SettingsPage section={route.name === "settings" ? route.section : null} onClose={() => undefined} />;
  }
  render(<Harness />);
  await user.type(screen.getByRole("searchbox", { name: "Search settings" }), "Branch prefix");
  await user.click(screen.getByRole("link", { name: "Branch prefix · Worktrees" }));
  const control = await screen.findByRole("textbox", { name: "Branch prefix" });
  await waitFor(() => expect(control.closest(".setting-line")).toHaveFocus());
  await user.clear(screen.getByRole("searchbox", { name: "Search settings" }));
  await user.type(screen.getByRole("searchbox", { name: "Search settings" }), "unmatched setting");
  expect(screen.getByRole("status")).toHaveTextContent("No settings found");
});
