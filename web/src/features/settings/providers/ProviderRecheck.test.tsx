import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { EngineStatus } from "../../../api/types";
import type { ProviderEntry } from "../../../generated/ProviderEntry";
import { useConnection } from "../../../product/state/connection";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { holdFetch, wire } from "../../../wire.test-support";
import { ProvidersSection } from "./Providers";

const internal = "Orbyn hit an internal error. Check the service log.";
/** A program the service found on the computer, besides the one engine it lists. */
const found = (name: string): ProviderEntry => ({ id: "extra", name, group: "cant_start", labels: [], registered: false, openable: false });
const reads = () => wire().filter((call) => call.method === "GET" && call.path === "/api/providers").length;

let mock: MockApi;
beforeEach(async () => {
  mock = installMockFetch();
  mock.setEngines([{ id: "test", name: "Test engine", tier: "acp", installed: true, auth: "not_applicable", version: "1", capabilities: {} } as unknown as EngineStatus]);
  mock.setFoundPrograms([found("Old catalogue")]);
  await useWorkspace.getState().refreshEngines();
});

it("updates catalogue rows after repeated checks with unchanged engine status, including offline", async () => {
  useConnection.setState({ offline: true });
  const user = userEvent.setup(); render(<ProvidersSection />);
  expect(await screen.findByText("Old catalogue")).toBeVisible();
  mock.setFoundPrograms([found("New catalogue")]);
  await user.click(within(screen.getByText(/You’re offline/).parentElement!).getByRole("button", { name: "Check again" }));
  expect(await screen.findByText("New catalogue")).toBeVisible();
  await user.click(within(screen.getByRole("status")).getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(reads()).toBe(3));
});

it("keeps the prior catalogue and shows the existing error when a check fails", async () => {
  const user = userEvent.setup(); render(<ProvidersSection />);
  expect(await screen.findByText("Old catalogue")).toBeVisible();
  mock.fail("/api/engines/refresh", 1, 500, "internal", internal);
  await user.click(screen.getByRole("button", { name: "Check again" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(internal);
  expect(screen.getByText("Old catalogue")).toBeVisible();
});

it("retains the catalogue when its refresh fails", async () => {
  const user = userEvent.setup(); render(<ProvidersSection />);
  expect(await screen.findByText("Old catalogue")).toBeVisible();
  mock.fail("/api/providers", 1, 500, "internal", internal);
  await user.click(screen.getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Check again" })).toBeEnabled());
  expect(screen.getByText("Old catalogue")).toBeVisible();
});

it("ignores a catalogue reply after unmount and a newer page read", async () => {
  const user = userEvent.setup(); const page = render(<ProvidersSection />);
  expect(await screen.findByText("Old catalogue")).toBeVisible();
  // The next catalogue read is slow: the service answers it with what it knows now, and the reply is late.
  mock.setFoundPrograms([found("Stale catalogue")]);
  const slow = holdFetch("GET", "/api/providers", { times: 1 });
  await user.click(screen.getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(slow.signals).toHaveLength(1));
  page.unmount();
  mock.setFoundPrograms([found("Current catalogue")]);
  render(<ProvidersSection />);
  expect(await screen.findByText("Current catalogue")).toBeVisible();
  // The late reply is delivered and its continuation runs inside act before anything is asserted.
  // A zero-delay macrotask turn (not real time) drains that continuation: the thing asserted is an
  // absence, so there is no element to wait for, and the held fetch exposes no completion signal.
  await act(async () => {
    slow.release();
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  // Since the slow read was held: it and the new page's own read.
  expect(reads()).toBe(2);
  expect(screen.getByText("Current catalogue")).toBeVisible();
  expect(screen.queryByText("Stale catalogue")).toBeNull();
});
