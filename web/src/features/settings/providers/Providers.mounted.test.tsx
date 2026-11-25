import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { EngineStatus } from "../../../api/types";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { holdFetch, wire } from "../../../wire.test-support";
import { ProvidersSection } from "./Providers";

/**
 * The page's mounted guard: a check reads the catalogue again only while the page is still on
 * screen. The risk is the other way round: a check that outlives the page must not start a
 * catalogue read for a page that is gone.
 */
let mock: MockApi;
/** Every catalogue read the page sent through the stubbed fetch. */
const reads = () => wire().filter((call) => call.method === "GET" && call.path === "/api/providers").length;

beforeEach(async () => {
  mock = installMockFetch();
  mock.setEngines([{ id: "test", name: "Test engine", tier: "acp", installed: true, auth: "not_applicable", version: "1", capabilities: {} } as unknown as EngineStatus]);
  await useWorkspace.getState().refreshEngines();
});

it("reads the catalogue again when a check finishes while the page is mounted", async () => {
  const user = userEvent.setup();
  render(<ProvidersSection />);
  // The list replaces its rows when the catalogue arrives, so wait for the page's own controls instead.
  await screen.findByRole("button", { name: "Check again" });
  await waitFor(() => expect(reads()).toBe(1));

  await user.click(screen.getByRole("button", { name: "Check again" }));

  // The check refreshed the engines and then read the catalogue again: the mounted guard let it through.
  await waitFor(() => expect(reads()).toBe(2));
});

it("starts no catalogue read when the page is unmounted during a check", async () => {
  const user = userEvent.setup();
  const page = render(<ProvidersSection />);
  await screen.findByRole("button", { name: "Check again" });
  await waitFor(() => expect(reads()).toBe(1));

  // The service has not answered the check yet: the page is unmounted while the read is in flight.
  const slow = holdFetch("POST", "/api/engines/refresh");
  await user.click(screen.getByRole("button", { name: "Check again" }));
  // A write carries no abort signal, so the held request itself says it is in flight.
  await waitFor(() => expect(wire()).toContainEqual({ method: "POST", path: "/api/engines/refresh" }));
  page.unmount();
  // Deliver the late answer and drain its continuation inside act: the assertion is an absence,
  // so there is no element to wait for and the held fetch exposes no completion signal.
  await act(async () => {
    slow.release();
    await new Promise((resolve) => setTimeout(resolve, 0));
  });

  // The held stub sees every request from the click on: the guard started no catalogue read for the gone page.
  expect(reads()).toBe(0);
});
