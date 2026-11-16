import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { CrewReviewControlView } from "../../../../generated/CrewReviewControlView";
import type { CrewLaneRemovalAnswer } from "../../../../generated/CrewLaneRemovalAnswer";
import { ApiFailure } from "../../../../api/client";
import * as origin from "../../../../api/origin";
import { useConnection } from "../../../../product/state/connection";
import { reviewControlApi } from "../../shared/reviewControlApi";
import { RequiredLanes } from "./RemoveLane";

const controls = (task: string): CrewReviewControlView => ({ task, error: null, lanes: [
  { lane: "licence", source: "Repository rule", locked: false, can_remove: true, removal: null },
  { lane: "security", source: "Repository rule", locked: true, can_remove: false, removal: null },
] });
beforeEach(() => { useConnection.setState({ remote: false }); vi.spyOn(reviewControlApi, "lanes").mockImplementation(async (task) => controls(task)); });
afterEach(() => vi.restoreAllMocks());

it("validates trimmed Unicode code points without truncating and Cancel discards the temporary reason", async () => {
  const send = vi.spyOn(reviewControlApi, "remove"); const user = userEvent.setup();
  render(<RequiredLanes task="remove-validation" name="Review dependency licences" onChanged={vi.fn()} />);
  await user.click(await screen.findByRole("button", { name: "Remove…" }));
  const field = screen.getByRole("textbox", { name: "Reason" }); expect(field).toHaveFocus();
  await user.click(screen.getByRole("button", { name: "Remove lane" })); expect(screen.getByText("Enter a reason to remove this lane.")).toBeVisible();
  fireEvent.change(field, { target: { value: "😀".repeat(1001) } }); await user.click(screen.getByRole("button", { name: "Remove lane" }));
  expect(field).toHaveValue("😀".repeat(1001)); expect(field).toHaveAttribute("aria-invalid", "true");
  fireEvent.change(field, { target: { value: "😀".repeat(1000) } }); expect(field).toHaveAttribute("aria-invalid", "false"); expect(send).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Cancel" })); await user.click(screen.getByRole("button", { name: "Remove…" })); expect(screen.getByRole("textbox", { name: "Reason" })).toHaveValue("");
});
it("reconciles unknown results and displays the first authoritative reason instead of the submitted draft", async () => {
  const task = "remove-unknown", user = userEvent.setup();
  vi.spyOn(reviewControlApi, "remove").mockRejectedValue(new ApiFailure("unknown", "Reply lost", 0));
  vi.mocked(reviewControlApi.lanes).mockResolvedValueOnce(controls(task)).mockImplementation(async () => ({ ...controls(task), lanes: [{ ...controls(task).lanes[0]!, can_remove: false, removal: { id: "first-receipt", lane: "licence", reason: "Earlier original reason" } }] }));
  render(<RequiredLanes task={task} name="Review dependency licences" onChanged={vi.fn()} />);
  await user.click(await screen.findByRole("button", { name: "Remove…" })); await user.type(screen.getByRole("textbox", { name: "Reason" }), "My different reason"); await user.click(screen.getByRole("button", { name: "Remove lane" }));
  expect(await screen.findByText("Recorded reason: Earlier original reason")).toBeVisible(); expect(reviewControlApi.remove).toHaveBeenCalledTimes(1);
  expect(screen.getByText(/Removed by person/)).toBeVisible();
});
it("keeps unknown outcomes read-only when reconciliation is unavailable", async () => {
  const user = userEvent.setup(); const send = vi.spyOn(reviewControlApi, "remove").mockRejectedValue(new ApiFailure("unknown", "lost", 0));
  vi.mocked(reviewControlApi.lanes).mockResolvedValueOnce(controls("remove-read-failed")).mockRejectedValue(new Error("Read failed"));
  render(<RequiredLanes task="remove-read-failed" name="Task" onChanged={vi.fn()} />);
  await user.click(await screen.findByRole("button", { name: "Remove…" })); await user.type(screen.getByRole("textbox", { name: "Reason" }), "Reviewed manually."); await user.click(screen.getByRole("button", { name: "Remove lane" }));
  expect(await screen.findByText(/Couldn’t check whether the lane was removed/)).toBeVisible(); expect(screen.queryByRole("button", { name: "Remove lane" })).toBeNull();
  await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Check again" })); expect(send).toHaveBeenCalledTimes(1);
});
it("never retargets a pending removal when selection changes", async () => {
  let resolve!: (value: CrewLaneRemovalAnswer) => void;
  const send = vi.spyOn(reviewControlApi, "remove").mockImplementation(() => new Promise((done) => { resolve = done; }));
  const changed = vi.fn(), user = userEvent.setup(); const view = render(<RequiredLanes task="remove-old" name="Old task" onChanged={changed} />);
  await user.click(await screen.findByRole("button", { name: "Remove…" })); await user.type(screen.getByRole("textbox", { name: "Reason" }), "Old reason"); await user.click(screen.getByRole("button", { name: "Remove lane" }));
  view.rerender(<RequiredLanes task="remove-new" name="New task" onChanged={changed} />);
  await act(async () => { resolve({ task: "remove-old", receipt: { id: "old", lane: "licence", reason: "Old reason" } }); });
  await waitFor(() => expect(screen.queryByText("Recorded reason: Old reason")).toBeNull()); expect(changed).not.toHaveBeenCalled(); expect(send).toHaveBeenCalledWith("remove-old", "licence", "Old reason");
});
it("offers no removal to a remote viewer and never offers one for a locked lane", async () => {
  useConnection.setState({ remote: true }); const send = vi.spyOn(reviewControlApi, "remove");
  render(<RequiredLanes task="remove-remote" name="Task" onChanged={vi.fn()} />);
  expect(await screen.findByText(/Only a person on this computer/)).toBeVisible(); expect(screen.queryByRole("button", { name: "Remove…" })).toBeNull(); expect(send).not.toHaveBeenCalled();
});
it("never refreshes a different service when a captured write finishes", async () => {
  let resolve!: (value: CrewLaneRemovalAnswer) => void;
  vi.spyOn(reviewControlApi, "remove").mockImplementation(() => new Promise((done) => { resolve = done; }));
  const changed = vi.fn(), user = userEvent.setup(); render(<RequiredLanes task="remove-service-change" name="Task" onChanged={changed} />);
  await user.click(await screen.findByRole("button", { name: "Remove…" })); await user.type(screen.getByRole("textbox", { name: "Reason" }), "Original service reason"); await user.click(screen.getByRole("button", { name: "Remove lane" }));
  vi.spyOn(origin, "serviceUrl").mockReturnValue("http://other.fixture.invalid/api/crew/settings/engine-tiers");
  await act(async () => { resolve({ task: "remove-service-change", receipt: { id: "original-service-receipt", lane: "licence", reason: "Original service reason" } }); });
  expect(changed).not.toHaveBeenCalled(); expect(reviewControlApi.lanes).toHaveBeenCalledTimes(1);
});
