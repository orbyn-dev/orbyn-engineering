// Settings › Crew › Daily digest, built to the design's Settings › Crew frame `settings-crew`: the row's label and help
// text as drawn, with a time field where the frame draws the value `17:00` (the frame draws no time control).
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import { installMockFetch } from "../../../../test-support";
import { CrewSettingsSection } from "./CrewSettings";
import { DigestTime } from "./DigestTime";

let mock: MockApi;
const SAVE = "/api/crew/autopilot/digest/schedule/set";
const writes = () => vi.mocked(fetch).mock.calls.filter(([url, init]) => String(url).includes(SAVE) && init?.method === "POST");
const field = () => screen.getByLabelText("Daily digest time");

beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
});

describe("Settings › Crew › Daily digest", () => {
  it("shows the stored send time with the design's row text", async () => {
    mock.reset("crew");
    render(<DigestTime />);
    await waitFor(() => expect(field()).toHaveValue("17:00"));
    expect(screen.getByText("Daily digest")).toBeVisible();
    expect(screen.getByText("What merged, what is waiting for your review and what was reverted, sent once a day in your local time.")).toBeVisible();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("shows the 17:00 default with no offset yet, and says nothing is sent until a time is saved", async () => {
    mock.reset("crew-digest-unsaved");
    render(<DigestTime />);
    expect(await screen.findByText(/Nothing is sent until you save a time/)).toBeVisible();
    expect(field()).toHaveValue("17:00");
    expect(writes()).toHaveLength(0);
  });

  it("saves the picked time with the person key and the page's own UTC offset", async () => {
    mock.reset("crew");
    render(<DigestTime />);
    await waitFor(() => expect(field()).toHaveValue("17:00"));
    fireEvent.change(field(), { target: { value: "09:30" } });
    fireEvent.blur(field());
    await waitFor(() => expect(writes()).toHaveLength(1));
    const [, init] = writes()[0]!;
    // The page sends `-new Date().getTimezoneOffset()`. In UTC that is `-0`, and JSON carries it as `0`,
    // so derive the same expression and normalise the sign instead of pinning one machine's offset.
    const utcOffsetMinutes = -new Date().getTimezoneOffset() || 0;
    expect(JSON.parse(String(init?.body))).toEqual({ time: "09:30", utc_offset_minutes: utcOffsetMinutes });
    expect(new Headers(init?.headers).get("x-orbyn-person")).toBeTruthy();
    await waitFor(() => expect(field()).toHaveValue("09:30"));
    expect(screen.queryByText(/Nothing is sent until you save a time/)).toBeNull();
  });

  it("says plainly that a save failed and leaves the stored time shown", async () => {
    mock.reset("crew-digest-save-fail");
    render(<DigestTime />);
    await waitFor(() => expect(field()).toHaveValue("17:00"));
    fireEvent.change(field(), { target: { value: "09:30" } });
    fireEvent.blur(field());
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not save the daily digest time.");
    expect(field()).toHaveValue("17:00");
  });

  it("writes nothing when the field is left as it was", async () => {
    mock.reset("crew");
    render(<DigestTime />);
    await waitFor(() => expect(field()).toHaveValue("17:00"));
    fireEvent.blur(field());
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(writes()).toHaveLength(0);
  });

  it("is drawn on the Crew settings page, after Autopilot", async () => {
    mock.reset("crew");
    render(<CrewSettingsSection />);
    const autopilot = await screen.findByRole("heading", { name: "Autopilot" });
    const digest = await screen.findByLabelText("Daily digest time");
    expect(digest).toBeVisible();
    expect(autopilot.compareDocumentPosition(digest) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
