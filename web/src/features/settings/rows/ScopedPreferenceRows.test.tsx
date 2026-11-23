import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { ScopedPreferencesResult } from "../../../generated/ScopedPreferencesResult";
import { installMockFetch } from "../../../test-support";
import { PreferenceGroups } from "./PreferenceRows";
import type { PreferenceRow } from "./preferenceTypes";
import { sentEdits } from "./preferenceWrites.test-support";

/** The two rows issue #1121 failed on: a number field and a switch, in the General section's order. */
const ROWS: PreferenceRow[] = [
  { key: "automatic_git_fetch_minutes", title: "Git fetch interval", description: "Background git fetch period" },
  { key: "enable_provider_update_checks", title: "Check providers for updates" },
];
/** What the service says, one message per field, so the text names the row that failed too. */
const GIT_FETCH_REFUSED = "The shortest background interval is 1 minute.";
const DESKTOP_ONLY_REFUSED = "This action is only available in the desktop app.";
let mock: MockApi;

const storedValue = (key: string) => {
  const read = mock.handle("POST", "/api/settings/preferences/query", { scope: {}, keys: [key] });
  return (read.body as ScopedPreferencesResult).rows[0]?.value;
};
/** The row that draws a title, so a message can be asserted against the row that should own it. */
const rowOf = (title: string) => screen.getByText(title).closest(".setting-line") as HTMLElement;

beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
});

describe("a failed preference save says so on the row that failed", () => {
  it("draws the failure on the later row that failed, not on the first row", async () => {
    const user = userEvent.setup();
    render(<PreferenceGroups groups={[{ title: "", rows: ROWS }]} />);
    const toggle = await screen.findByRole("switch", { name: "Check providers for updates" });
    mock.fail("/api/settings/preferences", 1, 403, "desktop_required", DESKTOP_ONLY_REFUSED, "PUT");
    await user.click(toggle);

    // The negative half is the point: the other row must stay clean, so a message that merely
    // appears somewhere on the page, or in a shared slot above the group, fails here.
    const failedRow = rowOf("Check providers for updates");
    expect(await within(failedRow).findByRole("alert")).toHaveTextContent(DESKTOP_ONLY_REFUSED);
    expect(within(rowOf("Git fetch interval")).queryByRole("alert")).toBeNull();
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(sentEdits().map((edits) => Object.keys(edits))).toEqual([["enable_provider_update_checks"]]);
    expect(storedValue("enable_provider_update_checks")).toBe(true);
  });

  it("draws a failure on the first row there too, and never on the second row", async () => {
    const user = userEvent.setup();
    render(<PreferenceGroups groups={[{ title: "", rows: ROWS }]} />);
    const field = await screen.findByRole("spinbutton", { name: "Git fetch interval" });
    await user.clear(field);
    await user.type(field, "9");
    mock.fail("/api/settings/preferences", 1, 400, "invalid_input", GIT_FETCH_REFUSED, "PUT");
    await user.tab();

    const failedRow = rowOf("Git fetch interval");
    expect(await within(failedRow).findByRole("alert")).toHaveTextContent(GIT_FETCH_REFUSED);
    expect(within(rowOf("Check providers for updates")).queryByRole("alert")).toBeNull();
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(storedValue("automatic_git_fetch_minutes")).toBe(5);
  });

  it("moves the message when the next failed save is a different row", async () => {
    const user = userEvent.setup();
    render(<PreferenceGroups groups={[{ title: "", rows: ROWS }]} />);
    const toggle = await screen.findByRole("switch", { name: "Check providers for updates" });
    mock.fail("/api/settings/preferences", 1, 403, "desktop_required", DESKTOP_ONLY_REFUSED, "PUT");
    await user.click(toggle);
    expect(await within(rowOf("Check providers for updates")).findByRole("alert")).toBeVisible();

    const field = screen.getByRole("spinbutton", { name: "Git fetch interval" });
    await user.clear(field);
    await user.type(field, "9");
    mock.fail("/api/settings/preferences", 1, 400, "invalid_input", GIT_FETCH_REFUSED, "PUT");
    await user.tab();

    expect(await within(rowOf("Git fetch interval")).findByRole("alert")).toHaveTextContent(GIT_FETCH_REFUSED);
    expect(within(rowOf("Check providers for updates")).queryByRole("alert")).toBeNull();
    expect(screen.getAllByRole("alert")).toHaveLength(1);
  });
});
