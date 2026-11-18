import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { MOCK_CREW, MOCK_CREW_TWO } from "../../../../mock/flow";
import { useConnection } from "../../../product/state/connection";
import { installMockFetch } from "../../../test-support";
import { MapView } from "./MapView";
import { CrewPick } from "./CrewPick";

/**
 * The crew picker the Map and the Lanes sit on, and the map it moves to another crew. The map and the lanes themselves are
 * map.test.tsx.
 */
let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  window.location.hash = "";
});

const urls = () => vi.mocked(fetch).mock.calls.map(([url]) => String(url));
const calls = (part: string) => urls().filter((url) => url.includes(part));
const writes = () => vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method && init.method !== "GET");

/** The picker with a stand-in child, which draws the crew it was handed and nothing else. */
function pick(label = "map") {
  render(
    <CrewPick label={label} id="mp-crew-pick">
      {(crew) => <p>{crew}</p>}
    </CrewPick>,
  );
}

describe("the crew picker", () => {
  it("lists each crew with its tasks and finished count, picks the first, and hands the one chosen to its screen", async () => {
    mock.reset("crew");
    const user = userEvent.setup();
    pick();
    const select = await screen.findByRole("combobox", { name: "Crew" });
    const options = screen.getAllByRole("option").map((one) => one.textContent ?? "");
    expect(options).toHaveLength(2);
    for (const option of options) expect(option).toMatch(/ · \d+ tasks?, \d+ finished$/);
    // One task is a task, not tasks.
    expect(options[1]).toMatch(/ · 1 task, 0 finished$/);
    expect(options[1]).not.toMatch(/1 tasks/);
    // The label the service gives, never the crew's id.
    expect(options[1]).toContain(MOCK_CREW_TWO.slice(0, 12));
    expect(options[1]).not.toContain(MOCK_CREW_TWO);
    // The first crew is the one drawn before anyone chooses.
    expect(screen.getByText(MOCK_CREW)).toBeVisible();
    await user.selectOptions(select, MOCK_CREW_TWO);
    expect(screen.getByText(MOCK_CREW_TWO)).toBeVisible();
    expect(screen.queryByText(MOCK_CREW)).toBeNull();
    expect(calls("/api/crew/flow/crews")).toHaveLength(1);
    expect(calls("/api/crew/flow/map")).toHaveLength(0);
    expect(writes()).toHaveLength(0);
  });

  it("says it is reading the map until the crew list answers, and draws no picker before it", async () => {
    mock.reset("crew");
    const real = vi.mocked(fetch).getMockImplementation()!;
    let answer = () => {};
    const answered = new Promise<void>((resolve) => (answer = resolve));
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (String(input).includes("/api/crew/flow/crews")) await answered;
      return real(input, init);
    });
    pick();
    expect(await screen.findByText("Reading the map")).toBeVisible();
    expect(screen.queryByRole("combobox", { name: "Crew" })).toBeNull();
    answer();
    expect(await screen.findByRole("combobox", { name: "Crew" })).toBeVisible();
    expect(calls("/api/crew/flow/crews")).toHaveLength(1);
    expect(writes()).toHaveLength(0);
  });

  it("names a crew list it cannot read, and reads it again on Retry", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/flow/crews", 1);
    const user = userEvent.setup();
    pick();
    expect(screen.queryByRole("combobox", { name: "Crew" })).toBeNull();
    await user.click(await screen.findByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("combobox", { name: "Crew" })).toBeVisible();
    expect(calls("/api/crew/flow/crews")).toHaveLength(2);
    expect(writes()).toHaveLength(0);
  });

  it("says No crews running when the service lists none", async () => {
    mock.reset("crew-empty");
    pick("lanes");
    expect(await screen.findByText("No crews running")).toBeVisible();
    expect(screen.queryByRole("combobox", { name: "Crew" })).toBeNull();
    expect(writes()).toHaveLength(0);
  });
});

describe("the Crew map's picker", () => {
  it("reads the map chosen in the picker, and reads the first again when it is chosen back", async () => {
    mock.reset("crew");
    const user = userEvent.setup();
    render(<MapView />);
    const select = await screen.findByRole("combobox", { name: "Crew" });
    await screen.findByRole("list", { name: "Crew" });
    expect(calls("/api/crew/flow/map")).toHaveLength(1);
    expect(calls(`crew=${MOCK_CREW}`).some((url) => url.includes("/flow/map"))).toBe(true);
    await user.selectOptions(select, MOCK_CREW_TWO);
    await waitFor(() => expect(calls(`crew=${MOCK_CREW_TWO}`).some((url) => url.includes("/flow/map"))).toBe(true));
    // The first crew's tree is gone at once, whatever the second crew's map read answers.
    await waitFor(() => expect(screen.queryByText("Add tests for the redirect, including //evil.com")).toBeNull());
    await user.selectOptions(select, MOCK_CREW);
    expect(await screen.findByRole("list", { name: "Crew" })).toBeVisible();
    const maps = (crew: string) => calls(`crew=${crew}`).filter((url) => url.includes("/flow/map"));
    expect(maps(MOCK_CREW)).toHaveLength(2);
    expect(maps(MOCK_CREW_TWO)).toHaveLength(1);
    expect(writes()).toHaveLength(0);
  });
});
