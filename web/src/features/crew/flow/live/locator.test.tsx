import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flowMock } from "../../../../../mock/flow";
import { ApiFailure } from "../../../../api/client";
import type { FlowTaskDetail } from "../../../../generated/FlowTaskDetail";
import { installMockFetch } from "../../../../test-support";
import { CrewSurface } from "../../autopilot/approvals/CrewSurface";
import { flowApi } from "./api";
import { FlowView } from "./FlowView";

const detail = (id: string, name: string, crew = "crew-docs-0002"): FlowTaskDetail => {
  const sample = flowMock("crew", "GET", "/api/crew/flow/tasks/task-redirect", new URLSearchParams())!.body as FlowTaskDetail;
  return { ...sample, crew, row: { ...sample.row, id, name } };
};
let mock: ReturnType<typeof installMockFetch>;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
  window.localStorage.clear();
});
afterEach(() => vi.restoreAllMocks());
const writes = () => vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method && init.method !== "GET");

it("opens a task outside the current page through the public Crew surface and uses its stored crew", async () => {
  mock.reset("crew");
  const read = vi.spyOn(flowApi, "task").mockResolvedValue(detail("older-task", "Older task"));
  const user = userEvent.setup();
  render(<CrewSurface tab="flow" task="older-task" />);
  expect(await screen.findByRole("heading", {name:"Older task"})).toBeVisible();
  expect(read).toHaveBeenCalledWith("older-task", expect.any(AbortSignal));
  expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).includes("/api/crew/flow/crews"))).toBe(false);
  await user.click(screen.getByRole("button", {name:"All tasks"}));
  expect(await screen.findByRole("combobox", {name:"Crew"})).toHaveValue("crew-docs-0002");
  expect(writes()).toHaveLength(0);
});

it("uses the existing missing task view without selecting another task", async () => {
  mock.reset("crew");
  vi.spyOn(flowApi, "task").mockRejectedValue(new ApiFailure("not_found", "task", 404));
  render(<FlowView task="deleted-task" />);
  expect(await screen.findByText("This task isn’t in Flow")).toBeVisible();
  expect(screen.queryByRole("list", {name:"Tasks"})).toBeNull();
  expect(writes()).toHaveLength(0);
});

it("makes no task read while Preview is off", async () => {
  mock.reset("default");
  const read = vi.spyOn(flowApi, "task");
  const {container} = render(<CrewSurface tab="flow" task="older-task" />);
  await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).includes("/api/crew/preview"))).toBe(true));
  expect(container).toBeEmptyDOMElement();
  expect(read).not.toHaveBeenCalled();
  expect(writes()).toHaveLength(0);
});

it("never lets an earlier locator response replace the new task or crew", async () => {
  mock.reset("crew");
  let finishOld!: (value: FlowTaskDetail) => void;
  const old = new Promise<FlowTaskDetail>((resolve) => { finishOld = resolve; });
  const read = vi.spyOn(flowApi, "task").mockImplementation((id) => id === "old-task" ? old : Promise.resolve(detail("new-task", "New task")));
  const user = userEvent.setup();
  const view = render(<FlowView task="old-task" />);
  await waitFor(() => expect(read).toHaveBeenCalledWith("old-task", expect.any(AbortSignal)));
  view.rerender(<FlowView task="new-task" />);
  expect(await screen.findByRole("heading", {name:"New task"})).toBeVisible();
  await act(async () => { finishOld(detail("old-task", "Old task", "crew-redirect-0001")); });
  expect(screen.queryByRole("heading", {name:"Old task"})).toBeNull();
  await user.click(screen.getByRole("button", {name:"All tasks"}));
  expect(await screen.findByRole("combobox", {name:"Crew"})).toHaveValue("crew-docs-0002");
  expect(writes()).toHaveLength(0);
});
