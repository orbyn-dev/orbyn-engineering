import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { useUi } from "../../../product/state/ui";
import { installMockFetch } from "../../../test-support";
import { ChatSection } from "./ChatSection";
import { GeneralSection } from "./GeneralSection";
import { sentEdits, preferenceWrites } from "../rows/preferenceWrites.test-support";

beforeEach(() => {
  installMockFetch();
  window.location.hash = "";
});

it("sets what Enter does while a turn runs from a select", async () => {
  const user = userEvent.setup();
  render(<ChatSection />);
  const select = await screen.findByRole("combobox", { name: "While a turn runs, Enter" });
  expect(screen.getByText("Steer sends it into the turn that is running.")).toBeVisible();
  await user.selectOptions(select, "Steers the running turn");
  expect(useUi.getState().followUpBehavior).toBe("steer");
});

it("reads and writes Show answers at the device scope with no inheritance icon", async () => {
  const user = userEvent.setup();
  render(<ChatSection />);
  await user.selectOptions(await screen.findByRole("combobox", { name: "Show answers" }), "When the turn ends");
  await waitFor(() => expect(sentEdits()).toEqual([{ response_streaming_mode: { op: "set", value: "turn" } }]));
  expect(preferenceWrites()[0]?.scope).toEqual({});
  expect(screen.queryByRole("button", { name: /Set for this project|From this device/ })).toBeNull();
});

it("picks the base directory with the folder browser", async () => {
  const user = userEvent.setup();
  render(<GeneralSection />);
  await user.click(await screen.findByRole("button", { name: "Choose…" }));
  const use = await screen.findByRole("button", { name: "Use this folder" });
  await waitFor(() => expect(use).toBeEnabled());
  await user.click(use);
  await waitFor(() => expect(sentEdits()).toEqual([{ add_project_base_directory: { op: "set", value: "/Users/dev" } }]));
  expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).startsWith("/api/fs/browse"))).toBe(true);
});
