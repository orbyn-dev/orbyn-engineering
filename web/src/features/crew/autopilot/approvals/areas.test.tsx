import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { Project } from "../../../../generated/Project";
import { useWorkspace } from "../../../../product/state/workspace";
import { installMockFetch } from "../../../../test-support";
import { CrewSettingsSection } from "./CrewSettings";
import { crewApi } from "./api";
import { stamp } from "./areas";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project], loading: false, error: null } });
});

const at = (day: number, hour: number, minute: number) => Math.floor(new Date(2026, 10, day, hour, minute).getTime() / 1000);

type Scenario = `crew-areas${"" | `-${string}`}`;
async function open(scenario: Scenario, page = "areas") {
  mock.reset(scenario);
  window.location.hash = `#/settings/crew/${page === "areas" ? "areas" : "areas/thresholds"}?repo=p-app`;
  const user = userEvent.setup();
  const view = render(<CrewSettingsSection />);
  await screen.findByRole("heading", { name: page === "areas" ? "Crew › Autopilot by area" : "Crew › Autopilot by area › Thresholds" });
  return { user, view };
}
const frame = () => document.querySelector("[data-frame]")?.getAttribute("data-frame");
const table = () => screen.getByRole("table", { name: "Autopilot by area" });
const rowOf = (area: string) => within(table()).getByText(area, { selector: "b" }).closest('[role="row"]') as HTMLElement;
const cells = (row: HTMLElement) => within(row).getAllByRole("cell").map((cell) => cell.textContent);

describe("autonomy-table", () => {
  it("lists each area with its ceiling, the mode it is on, the record and the sample, and says escapes are not measured", async () => {
    await open("crew-areas");
    expect(frame()).toBe("autonomy-table");
    expect(within(table()).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Area", "Now", "Reverted", "Escaped", "Window"]);
    expect(rowOf("docs/guides")).toHaveTextContent("docs/guidesFullFollows your ceiling0 of 12 revertedEscaped: Not measured14 days12 merges");
    // One ceiling per repository: the table has no ceiling column, and the page says it once.
    expect(within(table()).queryByRole("columnheader", { name: "Ceiling" })).toBeNull();
    expect(rowOf("crates/orbyn-keys")).toHaveTextContent("1 of 6 reverted");
    // Too few merges: no rate and no change.
    expect(cells(rowOf(".github/workflows"))[1]).toBe("FullNo change until there is data");
    expect(rowOf(".github/workflows")).toHaveTextContent("Reverted: Not measured");
    expect(screen.getByText(`As of ${stamp(at(6, 17, 0)).replace(/^(\w+ \d+ \w+),/, `$1 2026,`)}`)).toBeVisible();
    expect(screen.getByText("Repo: orbyn-engineering")).toBeVisible();
    expect(screen.getByText("Repo ceiling: Full · change it in Settings › Crew")).toBeVisible();
    expect(screen.getByText("Window: 14 days")).toBeVisible();
    expect(screen.getByText(/at least 5 merges in the window and at least 2 of them reverted/)).toBeVisible();
    expect(screen.getByText(/These thresholds are Proposed\./)).toBeVisible();
    expect(screen.getByText("Workers run as you, with your machine’s permissions.")).toBeVisible();
    // Nothing is a sample here: the data is the service's.
    expect(screen.queryByText("Sample data")).toBeNull();
    // Earning a higher mode is not built, so nothing offers it.
    expect(screen.queryByText(/earned|Earn upward/i)).toBeNull();
  });

  it("is not drawn while Crew has no repository to read", async () => {
    useWorkspace.setState({ projects: { data: [], loading: false, error: null } });
    mock.reset("crew-areas");
    window.location.hash = "#/settings/crew/areas";
    const { container } = render(<CrewSettingsSection />);
    await waitFor(() => expect(container.querySelector("[data-frame]")).toBeNull());
    expect(screen.queryByRole("table", { name: "Autopilot by area" })).toBeNull();
  });

  it("asks the service for the areas of the repository named in the address", async () => {
    const spy = vi.spyOn(crewApi, "areas");
    await open("crew-areas");
    expect(spy).toHaveBeenCalledWith("p-app", expect.anything());
  });
});

describe("autonomy-tripped", () => {
  it("shows the demotion in the row and says what it changes, what returns it and what the ceiling is", async () => {
    await open("crew-areas-tripped");
    expect(frame()).toBe("autonomy-tripped");
    const row = rowOf("web/auth");
    expect(cells(row)).toEqual(["web/auth", `OnDemoted ${stamp(at(7, 15, 36))}: 2 reverts in 14 days`, "2 of 9 reverted", "Escaped: Not measured", "14 days9 merges"]);
    const panel = screen.getByRole("region", { name: "Why web/auth moved from Full to On" });
    expect(panel).toHaveTextContent("2 reverts in 14 days, in 9 merges (a trip needs 5 merges and 2 reverted)");
    expect(panel).toHaveTextContent("A High risk diff in web/auth waits for you at the merge gate. Low and Medium still merge by themselves. The risk tier itself is unchanged.");
    expect(panel).toHaveTextContent("14 days have passed since the last revert with none, and 5 merges since then with none reverted (Proposed)");
    expect(panel).toHaveTextContent("Full: the circuit breaker only demotes an area whose ceiling is Full, to On, and it never goes above the ceiling");
    expect(within(panel).getByRole("button", { name: "Override…" })).toBeVisible();
    // Every other area keeps its mode.
    expect(cells(rowOf("web/app"))[1]).toBe("FullFollows your ceiling");
  });

  it("marks the lowered mode quietly and leaves the ceiling plain", async () => {
    await open("crew-areas-tripped");
    const chips = within(rowOf("web/auth")).getAllByText(/^(Full|On)$/);
    expect(chips.map((chip) => [chip.textContent, chip.className.includes("ap-chip-dim")])).toEqual([["On", true]]);
  });
});

describe("autonomy-demoted-ceiling-lowered", () => {
  it("shows the ceiling winning over the demotion, with the record still counted", async () => {
    await open("crew-areas-ceiling-off");
    expect(frame()).toBe("autonomy-demoted-ceiling-lowered");
    expect(screen.getByText("Repo ceiling: Off · change it in Settings › Crew")).toBeVisible();
    expect(cells(rowOf("web/auth"))[1]).toBe("OffYour ceiling is Off: it wins over the demotion");
    expect(cells(rowOf("web/app"))[1]).toBe("OffYour ceiling is Off: the record is shown, nothing changes");
    const panel = screen.getByRole("region", { name: "web/auth: the ceiling wins" });
    expect(panel).toHaveTextContent("Repo ceiling Off, Now Off. An area never goes above its ceiling, so the demotion to On changes nothing.");
    expect(panel).toHaveTextContent("2 of 9 merges reverted in 14 days: still shown, and still counted");
    expect(panel).toHaveTextContent("Everything in web/auth waits for you");
    expect(panel).toHaveTextContent("To Full: web/auth follows the record again, so it is on On until it recovers. To On: it stays On.");
    // The service does not say when or from what the ceiling was lowered, so the panel does not.
    expect(panel).not.toHaveTextContent("What you did");
  });
});

describe("autonomy-recovering", () => {
  it("counts the way back in plain numbers, never a bar", async () => {
    await open("crew-areas-recovering");
    expect(frame()).toBe("autonomy-recovering");
    expect(cells(rowOf("web/auth"))).toEqual(["web/auth", "OnRecovering: day 7 of 14 with no revert; 3 of 5 merges since, none reverted", "0 of 3 reverted", "Escaped: Not measured", "7 days3 merges"]);
    const panel = screen.getByRole("region", { name: "web/auth is recovering" });
    expect(panel).toHaveTextContent("7 of the 14 days with no revert.");
    expect(panel).toHaveTextContent("3 of the 5 merges needed since then, none reverted. Nothing returns before both counts are met.");
    expect(panel).toHaveTextContent("A new revert in this area starts the count again. A High risk diff in web/auth still waits for you at the merge gate.");
    expect(within(panel).queryByRole("progressbar")).toBeNull();
  });
});

describe("autonomy-criteria-met", () => {
  it("says it returns at the next merge, not on a clock", async () => {
    await open("crew-areas-criteria-met");
    expect(frame()).toBe("autonomy-criteria-met");
    expect(cells(rowOf("web/auth"))[1]).toBe("OnCriteria met; returns at the next merge");
    const panel = screen.getByRole("region", { name: "web/auth meets the criteria, and returns at the next merge" });
    expect(panel).toHaveTextContent("No revert for 14 of the 14 days.");
    expect(panel).toHaveTextContent("6 merges since then, none reverted (5 needed).");
    expect(panel).toHaveTextContent("the restore is checked at a merge, not on a clock. web/auth stays on On until the next merge in it");
  });
});

describe("autonomy-restored", () => {
  it("says when the area returned to its ceiling, from the trail, and that the history is kept", async () => {
    await open("crew-areas-restored");
    await waitFor(() => expect(frame()).toBe("autonomy-restored"));
    expect(cells(rowOf("web/auth"))[1]).toBe(`FullRestored ${stamp(at(21, 16, 10))} at a merge`);
    const panel = screen.getByRole("region", { name: "web/auth is back on Full" });
    expect(panel).toHaveTextContent("web/auth returned to your ceiling at the next merge, at 16:10");
    expect(panel).toHaveTextContent("The reverts that tripped it stay in the ledger and in the dashboard history. If the record worsens again, it drops again.");
  });
});

describe("autonomy-lowdata", () => {
  it("says each area has too few merges and shows Not measured, changing nothing", async () => {
    await open("crew-areas-lowdata");
    expect(frame()).toBe("autonomy-lowdata");
    for (const area of ["web/auth", "crates/orbyn-core", "web/app", ".github/workflows"]) {
      expect(cells(rowOf(area))[1]).toBe("FullNo change until there is data");
      expect(rowOf(area)).toHaveTextContent("Reverted: Not measured");
    }
    const panel = screen.getByRole("region", { name: "Not enough data: each area has fewer than 5 merges" });
    expect(panel).toHaveTextContent("A trip needs at least 5 merges in the window. Below that Crew shows the merges themselves and changes nothing");
  });
});

describe("autonomy-override", () => {
  it("offers three choices from the panel, with the current one checked, and the rule under them", async () => {
    const { user } = await open("crew-areas-tripped");
    await user.click(within(screen.getByRole("region", { name: "Why web/auth moved from Full to On" })).getByRole("button", { name: "Override…" }));
    expect(frame()).toBe("autonomy-override");
    const menu = await screen.findByRole("menu", { name: "web/auth override" });
    expect(menu).toHaveTextContent("web/auth is on On. What should it follow?");
    expect(within(menu).getAllByRole("menuitem").map((item) => item.getAttribute("aria-label"))).toEqual(["Follow the record", "Keep the ceiling", "Off"]);
    expect(within(menu).getByRole("menuitem", { name: "Follow the record" })).toHaveAttribute("aria-current", "true");
    expect(menu).toHaveTextContent("The default. It returns by itself when the record recovers");
    expect(menu).toHaveTextContent("Full, your ceiling. Needs an acknowledgement; a new trip clears it");
    expect(menu).toHaveTextContent("Everything in web/auth waits for you");
    expect(menu).toHaveTextContent("Only you can change this. Never above the ceiling.");
  });

  it("Off asks first: choosing it sends nothing until the box is ticked and the dialog confirmed", async () => {
    const { user } = await open("crew-areas-tripped");
    const spy = vi.spyOn(crewApi, "overrideArea");
    await user.click(within(screen.getByRole("region", { name: "Why web/auth moved from Full to On" })).getByRole("button", { name: "Override…" }));
    await user.click(await screen.findByRole("menuitem", { name: "Off" }));
    const dialog = await screen.findByRole("dialog", { name: "Turn web/auth off?" });
    expect(frame()).toBe("autonomy-override-off-warning");
    expect(spy).not.toHaveBeenCalled();
    const confirm = within(dialog).getByRole("button", { name: "Turn it off" });
    expect(confirm).toBeDisabled();
    await user.click(within(dialog).getByRole("checkbox", { name: "I understand that everything in web/auth will wait for me." }));
    await user.click(confirm);
    expect(spy).toHaveBeenCalledWith({ project: "p-app", area: "web/auth", choice: "off", acknowledged: true, seen: { merges: 9, reverts: 2, tripped_at: at(7, 15, 36) } });
    await waitFor(() => expect(cells(rowOf("web/auth"))[1]).toBe("OffA person set this area to off."));
  });

  it("Cancelling the Off dialog calls no route", async () => {
    const { user } = await open("crew-areas-tripped");
    const spy = vi.spyOn(crewApi, "overrideArea");
    await user.click(within(screen.getByRole("region", { name: "Why web/auth moved from Full to On" })).getByRole("button", { name: "Override…" }));
    await user.click(await screen.findByRole("menuitem", { name: "Off" }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Cancel" }));
    expect(spy).not.toHaveBeenCalled();
  });

  it.each([
    ["crew-areas-recovering", "web/auth is recovering"],
    ["crew-areas-criteria-met", "web/auth meets the criteria, and returns at the next merge"],
    ["crew-areas-ceiling-off", "web/auth: the ceiling wins"],
    ["crew-areas-restored", "web/auth is back on Full"],
  ] as const)("%s: the panel offers Override…", async (scenario, name) => {
    await open(scenario);
    await waitFor(() => expect(screen.getByRole("region", { name })).toBeVisible());
    expect(within(screen.getByRole("region", { name })).getByRole("button", { name: "Override…" })).toBeVisible();
  });

  it("Follow the record ends an override and the row goes back to the breaker's state", async () => {
    const { user } = await open("crew-areas-tripped");
    await user.click(within(screen.getByRole("region", { name: "Why web/auth moved from Full to On" })).getByRole("button", { name: "Override…" }));
    await user.click(await screen.findByRole("menuitem", { name: "Off" }));
    const off = await screen.findByRole("dialog", { name: "Turn web/auth off?" });
    await user.click(within(off).getByRole("checkbox"));
    await user.click(within(off).getByRole("button", { name: "Turn it off" }));
    await waitFor(() => expect(cells(rowOf("web/auth"))[1]).toBe("OffA person set this area to off."));
    // An overridden area has no panel of its own; the footer carries the way back.
    const follow = vi.spyOn(crewApi, "followArea");
    await user.click(screen.getByRole("button", { name: "Override…" }));
    const menu = await screen.findByRole("menu", { name: "web/auth override" });
    expect(within(menu).getByRole("menuitem", { name: "Off" })).toHaveAttribute("aria-current", "true");
    await user.click(within(menu).getByRole("menuitem", { name: "Follow the record" }));
    expect(follow).toHaveBeenCalledWith({ project: "p-app", area: "web/auth", seen: { merges: 9, reverts: 2, tripped_at: at(7, 15, 36) } });
    await waitFor(() => expect(cells(rowOf("web/auth"))[1]).toContain("Demoted"));
  });
});

describe("autonomy-override-warning", () => {
  async function warn() {
    const opened = await open("crew-areas-tripped");
    await opened.user.click(within(screen.getByRole("region", { name: "Why web/auth moved from Full to On" })).getByRole("button", { name: "Override…" }));
    await opened.user.click(await screen.findByRole("menuitem", { name: "Keep the ceiling" }));
    const dialog = await screen.findByRole("dialog", { name: "Keep the ceiling for web/auth?" });
    return { ...opened, dialog };
  }

  it("shows the record in numbers and what is at risk in words, and the button works only after the box is ticked", async () => {
    const { user, dialog } = await warn();
    expect(frame()).toBe("autonomy-override-warning");
    expect(dialog).toHaveTextContent("web/auth is on On because 2 of its last 9 merges were reverted in 14 days. If you keep the ceiling (Full), a High risk diff in web/auth merges by itself again, without waiting for you at the merge gate.");
    expect(dialog).toHaveTextContent("2 reverts in 14 days · 9 merges");
    expect(dialog).toHaveTextContent("A High risk diff in web/auth can reach main before you have looked at it. You review it afterwards, at the top of your Review queue.");
    expect(dialog).toHaveTextContent("Until a new trip. If web/auth is demoted again, Keep the ceiling is cleared and the area follows the record.");
    expect(dialog).toHaveTextContent("Only you can make this choice. It is logged with the time. It never raises an area above your ceiling, and the hard-denied list still applies.");
    const keep = within(dialog).getByRole("button", { name: "Keep the ceiling" });
    expect(keep).toBeDisabled();
    await user.click(within(dialog).getByRole("checkbox", { name: "I understand that High risk diffs in web/auth will merge without waiting for me." }));
    expect(keep).toBeEnabled();
  });

  it("sends the acknowledgement with the record the person saw, then the area reads as set by the person", async () => {
    const { user, dialog } = await warn();
    const spy = vi.spyOn(crewApi, "overrideArea");
    await user.click(within(dialog).getByRole("checkbox"));
    await user.click(within(dialog).getByRole("button", { name: "Keep the ceiling" }));
    expect(spy).toHaveBeenCalledWith({ project: "p-app", area: "web/auth", choice: "keep_ceiling", acknowledged: true, seen: { merges: 9, reverts: 2, tripped_at: at(7, 15, 36) } });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(cells(rowOf("web/auth"))[1]).toBe("FullA person set this area to keep ceiling."));
  });

  it("Cancel closes it and sends nothing", async () => {
    const { user, dialog } = await warn();
    const spy = vi.spyOn(crewApi, "overrideArea");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(spy).not.toHaveBeenCalled();
  });

  it("a record that moved while the person read it is refused with the service's words, and the dialog stays", async () => {
    mock.reset("crew-areas-stale");
    window.location.hash = "#/settings/crew/areas?repo=p-app";
    const user = userEvent.setup();
    render(<CrewSettingsSection />);
    await user.click(await within(await screen.findByRole("region", { name: "Why web/auth moved from Full to On" })).findByRole("button", { name: "Override…" }));
    await user.click(await screen.findByRole("menuitem", { name: "Keep the ceiling" }));
    const dialog = await screen.findByRole("dialog", { name: "Keep the ceiling for web/auth?" });
    await user.click(within(dialog).getByRole("checkbox"));
    await user.click(within(dialog).getByRole("button", { name: "Keep the ceiling" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("The record changed since you looked. Read it again before choosing.");
    expect(screen.getByRole("dialog", { name: "Keep the ceiling for web/auth?" })).toBeVisible();
  });
});

describe("autonomy-thresholds", () => {
  it("lists the breaker's constants read-only, each Proposed, with what counts", async () => {
    await open("crew-areas", "thresholds");
    expect(frame()).toBe("autonomy-thresholds");
    const list = screen.getByRole("list", { name: "Thresholds" });
    expect(within(list).getAllByRole("listitem").map((item) => item.textContent)).toEqual([
      "WindowHow far back Crew counts merges and reverts for an area.14 days · Proposed",
      "Merges neededBelow this many merges in the window an area says “Not enough data” and nothing changes.5 · Proposed",
      "Demote whenAn area whose ceiling is Full drops to On. A ceiling of On or Off is never changed.5 merges, 2 reverted · Proposed",
      "Return whenA demoted area goes back to Full once 14 days have passed since its last revert and 5 merges since then were none reverted. Checked at a merge; counts only.14 days, 5 merges · Proposed",
    ]);
    expect(screen.getByText("Proposed: Engineering sets the real values")).toBeVisible();
    expect(screen.getByRole("region", { name: "What counts" })).toHaveTextContent("An escapeNot measured");
    expect(screen.queryByText("Earn upward")).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("opens from the table, with the repository kept", async () => {
    const { user } = await open("crew-areas");
    await user.click(screen.getByRole("button", { name: "Open the thresholds" }));
    expect(window.location.hash).toBe("#/settings/crew/areas/thresholds?repo=p-app");
  });

  it("Open the ledger goes to the approvals tab, the one ledger", async () => {
    const { user } = await open("crew-areas");
    await user.click(screen.getByRole("button", { name: "Open the ledger" }));
    expect(window.location.hash).toBe("#/crew/approvals");
  });
});

describe("autonomy-narrow", () => {
  it("is the same rows: each carries its path, ceiling, mode, reason and record, and the footer offers Override…", async () => {
    await open("crew-areas-tripped");
    const row = rowOf("web/auth");
    expect(row).toHaveTextContent("web/authOnDemoted");
    expect(row).toHaveTextContent("2 of 9 revertedEscaped: Not measured14 days9 merges");
    expect(screen.getAllByRole("button", { name: "Override…" })).toHaveLength(2);
  });
});

describe("Crew Preview off", () => {
  it("answers 404 from every area route, so nothing is drawn", async () => {
    mock.reset("default");
    await expect(crewApi.areas("p-app")).rejects.toMatchObject({ status: 404 });
    await expect(crewApi.thresholds()).rejects.toMatchObject({ status: 404 });
  });
});
