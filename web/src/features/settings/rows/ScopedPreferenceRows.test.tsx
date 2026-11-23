import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure, request } from "../../../api/client";
import type { JsonValue, PreferenceView } from "../../../api/types";
import type { ScopedPreferencesChange } from "../../../generated/ScopedPreferencesChange";
import { PreferenceGroups } from "./PreferenceRows";
import type { PreferenceRow } from "./preferenceTypes";
import { scopedFixture } from "./scopedPreferenceFixture.test-support";

vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

/** The two rows issue #1121 failed on: a number field and a switch, in the General section's order. */
const ROWS: PreferenceRow[] = [
  { key: "automatic_git_fetch_minutes", title: "Git fetch interval", description: "Background git fetch period" },
  { key: "enable_provider_update_checks", title: "Check providers for updates" },
];
/** What the service said, one message per field, so the text names the row that failed too. */
const GIT_FETCH_REFUSED = "The service refused the git fetch interval.";
const DESKTOP_ONLY_REFUSED = "This action is only available in the desktop app.";
const REFUSED: Record<string, string> = {
  automatic_git_fetch_minutes: GIT_FETCH_REFUSED,
  enable_provider_update_checks: DESKTOP_ONLY_REFUSED,
};
const values: Record<string, JsonValue> = { automatic_git_fetch_minutes: 5, enable_provider_update_checks: true };
const requestMock = vi.mocked(request);
let refusedKeys: Set<string>;
let writes: string[];

function views(): PreferenceView[] {
  return ROWS.map((row) => ({
    key: row.key, value: values[row.key] ?? null, default: typeof values[row.key] === "boolean" ? true : 5,
    source: "default", project_scoped: false,
  }));
}
/** The row that draws a title, so a message can be asserted against the row that should own it. */
const rowOf = (title: string) => screen.getByText(title).closest(".setting-line") as HTMLElement;

beforeEach(() => {
  refusedKeys = new Set(); writes = [];
  requestMock.mockReset();
  requestMock.mockImplementation(async (method, path, body) => {
    if (method === "POST" && path === "/api/settings/preferences/query") return scopedFixture(views()) as never;
    if (method !== "PUT" || path !== "/api/settings/preferences") throw new Error(`unexpected ${method} ${path}`);
    const change = body as ScopedPreferencesChange;
    const keys = Object.keys(change.edits);
    writes.push(...keys);
    const refused = keys.find((key) => refusedKeys.has(key));
    if (refused !== undefined) throw new ApiFailure("unavailable", REFUSED[refused] ?? "Save failed.", 503);
    for (const key of keys) {
      const edit = change.edits[key];
      if (edit?.op === "set") values[key] = edit.value;
    }
    return scopedFixture(views()) as never;
  });
});

describe("a failed preference save says so on the row that failed", () => {
  it("draws the failure on the later row that failed, not on the first row", async () => {
    const user = userEvent.setup();
    refusedKeys = new Set(["enable_provider_update_checks"]);
    render(<PreferenceGroups groups={[{ title: "", rows: ROWS }]} />);
    await user.click(await screen.findByRole("switch", { name: "Check providers for updates" }));

    // The negative half is the point: the other row must stay clean, so a message that merely
    // appears somewhere on the page, or in a shared slot above the group, fails here.
    const failedRow = rowOf("Check providers for updates");
    expect(await within(failedRow).findByRole("alert")).toHaveTextContent(DESKTOP_ONLY_REFUSED);
    expect(within(rowOf("Git fetch interval")).queryByRole("alert")).toBeNull();
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(writes).toEqual(["enable_provider_update_checks"]);
    expect(values.enable_provider_update_checks).toBe(true);
  });

  it("draws a failure on the first row there too, and never on the second row", async () => {
    const user = userEvent.setup();
    refusedKeys = new Set(["automatic_git_fetch_minutes"]);
    render(<PreferenceGroups groups={[{ title: "", rows: ROWS }]} />);
    const field = await screen.findByRole("spinbutton", { name: "Git fetch interval" });
    await user.clear(field);
    await user.type(field, "9");
    await user.tab();

    const failedRow = rowOf("Git fetch interval");
    expect(await within(failedRow).findByRole("alert")).toHaveTextContent(GIT_FETCH_REFUSED);
    expect(within(rowOf("Check providers for updates")).queryByRole("alert")).toBeNull();
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(values.automatic_git_fetch_minutes).toBe(5);
  });

  it("moves the message when the next failed save is a different row", async () => {
    const user = userEvent.setup();
    refusedKeys = new Set(["enable_provider_update_checks"]);
    render(<PreferenceGroups groups={[{ title: "", rows: ROWS }]} />);
    await user.click(await screen.findByRole("switch", { name: "Check providers for updates" }));
    expect(await within(rowOf("Check providers for updates")).findByRole("alert")).toBeVisible();

    refusedKeys = new Set(["automatic_git_fetch_minutes"]);
    const field = screen.getByRole("spinbutton", { name: "Git fetch interval" });
    await user.clear(field);
    await user.type(field, "9");
    await user.tab();

    expect(await within(rowOf("Git fetch interval")).findByRole("alert")).toHaveTextContent(GIT_FETCH_REFUSED);
    expect(within(rowOf("Check providers for updates")).queryByRole("alert")).toBeNull();
    expect(screen.getAllByRole("alert")).toHaveLength(1);
  });
});
