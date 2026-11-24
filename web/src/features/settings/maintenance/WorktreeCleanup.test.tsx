import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { ScopedPreferencesChange } from "../../../generated/ScopedPreferencesChange";
import { installMockFetch } from "../../../test-support";
import { wireWrites } from "../../../wire.test-support";
import { seedPreference } from "../rows/seedPreference.test-support";
import { SettingsScopeProvider } from "../rows/SettingsScopeContext";
import { daysWhenTurnedOn, WorktreeCleanup } from "./WorktreeCleanup";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  // One project's checkout is selected, as on a project's page in Settings.
  window.location.hash = "#/settings/worktrees?project=mock%3Ap-app&checkout=p-app";
});
const page = () => <SettingsScopeProvider section="worktrees"><WorktreeCleanup /></SettingsScopeProvider>;

/** Whether a save of this value reached the service. */
const sent = (value: unknown) => expect(wireWrites().some((call) => call.method === "PUT" && call.path === "/api/settings/preferences"
  && JSON.stringify((call.body as ScopedPreferencesChange).edits) === JSON.stringify({ worktree_cleanup: { op: "set", value } }))).toBe(true);

it("is Off with no Days field until set to On, which starts at seven days", async () => {
  const user = userEvent.setup();
  render(page());
  const select = await screen.findByRole("combobox", { name: "Automatic worktree cleanup" });
  expect(select).toHaveValue("off");
  expect(screen.queryByRole("spinbutton")).toBeNull();
  await user.selectOptions(select, "on");
  await waitFor(() => sent({ mode: "custom", rules: { settled_after_days: 7 } }));
  expect(await screen.findByRole("spinbutton", { name: "Automatic worktree cleanup: days" })).toHaveValue(7);
});

it("shows the saved days beside On, saves a new number on leaving it, and Off hides it", async () => {
  seedPreference(mock, "worktree_cleanup", { mode: "custom", rules: { settled_after_days: 14 } });
  const user = userEvent.setup();
  render(page());
  const days = await screen.findByRole("spinbutton", { name: "Automatic worktree cleanup: days" });
  await waitFor(() => expect(days).toBeEnabled());
  expect(days).toHaveValue(14);
  // One change event rather than keystrokes: typing into a field that is briefly disabled drops characters under load.
  days.focus();
  fireEvent.change(days, { target: { value: "3" } });
  await user.tab();
  await waitFor(() => sent({ mode: "custom", rules: { settled_after_days: 3 } }));
  // The controls rest while the save and the read after it are under way; a change made then is not taken.
  const select = screen.getByRole("combobox", { name: "Automatic worktree cleanup" });
  await waitFor(() => expect(select).toBeEnabled());
  await user.selectOptions(select, "off");
  await waitFor(() => sent({ mode: "off" }));
  await waitFor(() => expect(screen.queryByRole("spinbutton")).toBeNull());
});

it("turns on from Off at seven days whatever the field still says, and keeps a set number otherwise", () => {
  expect(daysWhenTurnedOn(null, "12")).toBe(7);
  expect(daysWhenTurnedOn(null, "")).toBe(7);
  expect(daysWhenTurnedOn(14, "30")).toBe(30);
  expect(daysWhenTurnedOn(14, "")).toBe(7);
});
