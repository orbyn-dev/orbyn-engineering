import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { FlowReviewConsent } from "../../../../generated/FlowReviewConsent";
import type { CrewReviewConsentAnswer } from "../../../../generated/CrewReviewConsentAnswer";
import type { FlowTaskDetail } from "../../../../generated/FlowTaskDetail";
import { useConnection } from "../../../../product/state/connection";
import { ApiFailure } from "../../../../api/client";
import { flowMock } from "../../../../../mock/flow";
import { flowApi } from "./api";
import { ReviewConsent } from "./ReviewConsent";

let serial = 0;
const request = (): FlowReviewConsent => ({ id:`consent-test-${++serial}`,task:`task-${serial}`,lane:"security",round:1,commit:"a".repeat(40),engine:"alpha",model:"alpha-model",state:"waiting",expires_unix:4102444800,can_answer:true });
const receipt = (c: FlowReviewConsent, started = true): CrewReviewConsentAnswer => ({ task:started ? "review-task" : c.task,subject:c.task,request:c.id,state:started ? "running" : "wait",started });
const deferred = <T,>() => { let resolve!: (value: T) => void; const promise = new Promise<T>((done) => { resolve = done; }); return {promise,resolve}; };
beforeEach(() => { vi.restoreAllMocks(); useConnection.setState({remote:false}); window.location.hash = ""; });

it("submits a captured exact request once while busy, and retains its confirmed receipt", async () => {
  const c = request(); const result = deferred<CrewReviewConsentAnswer>();
  const send = vi.spyOn(flowApi, "consent").mockReturnValue(result.promise);
  const changed = vi.fn(); const user = userEvent.setup();
  render(<ul><ReviewConsent consent={c} onChanged={changed} /></ul>);
  await user.click(screen.getByRole("button", {name:"security: Needs you"}));
  await user.dblClick(screen.getByRole("button", {name:"Run on alpha once"}));
  expect(send).toHaveBeenCalledTimes(1);
  expect(send).toHaveBeenCalledWith(c.task, {request:c.id,choice:"once"});
  await act(async () => { result.resolve(receipt(c)); });
  expect(screen.getByText("Same-engine review started.")).toBeVisible();
  expect(changed).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("button", {name:"Run on alpha once"})).toBeDisabled();
});

it("does not attach an old completion or reload to a new selection", async () => {
  const a = request(); const b = request(); const result = deferred<CrewReviewConsentAnswer>();
  vi.spyOn(flowApi, "consent").mockReturnValue(result.promise);
  const oldChanged = vi.fn(); const newChanged = vi.fn(); const user = userEvent.setup();
  const view = render(<ul><ReviewConsent consent={a} onChanged={oldChanged} /></ul>);
  await user.click(screen.getByRole("button", {name:"security: Needs you"}));
  await user.click(screen.getByRole("button", {name:"Run on alpha once"}));
  view.rerender(<ul><ReviewConsent consent={b} onChanged={newChanged} /></ul>);
  await act(async () => { result.resolve(receipt(a)); });
  expect(oldChanged).not.toHaveBeenCalled(); expect(newChanged).not.toHaveBeenCalled();
  expect(screen.queryByText("Same-engine review started.")).toBeNull();
});

it("waits without launching and sign-in only opens the existing provider settings", async () => {
  const c = request(); const send = vi.spyOn(flowApi, "consent").mockResolvedValue(receipt(c, false));
  const user = userEvent.setup(); render(<ul><ReviewConsent consent={c} onChanged={vi.fn()} /></ul>);
  await user.click(screen.getByRole("button", {name:"security: Needs you"}));
  await user.click(screen.getByRole("button", {name:"Wait"}));
  expect(send).toHaveBeenCalledWith(c.task, {request:c.id,choice:"wait"});
  await user.click(screen.getByRole("button", {name:"Sign in to another engine"}));
  expect(window.location.hash).toBe("#/settings/providers"); expect(send).toHaveBeenCalledTimes(1);
});

it("requires a fresh server read after a failed/unknown outcome before another explicit submit", async () => {
  const c = request(); const send = vi.spyOn(flowApi, "consent").mockRejectedValue(new ApiFailure("invalid", "Consent is stale.", 400));
  const sample = flowMock("crew", "GET", "/api/crew/flow/tasks/task-redirect", new URLSearchParams())!.body as FlowTaskDetail;
  const read = vi.spyOn(flowApi, "task").mockResolvedValue({...sample,review_consents:[c]});
  const user = userEvent.setup(); render(<ul><ReviewConsent consent={c} onChanged={vi.fn()} /></ul>);
  await user.click(screen.getByRole("button", {name:"security: Needs you"}));
  await user.click(screen.getByRole("button", {name:"Run on alpha once"}));
  expect(await screen.findByText("Consent is stale.")).toBeVisible();
  expect(screen.getByRole("button", {name:"Run on alpha once"})).toBeDisabled();
  await user.click(screen.getByRole("button", {name:"Reload task"}));
  expect(read).toHaveBeenCalledWith(c.task); expect(send).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("button", {name:"Run on alpha once"})).toBeEnabled();
});

it("keeps remote and expired requests readable without a writable consent control", async () => {
  const c = request(); useConnection.setState({remote:true});
  const send = vi.spyOn(flowApi, "consent"); const user = userEvent.setup();
  render(<ul><ReviewConsent consent={{...c,state:"expired",can_answer:false}} onChanged={vi.fn()} /></ul>);
  await user.click(screen.getByRole("button", {name:"security: Needs you"}));
  expect(screen.getByRole("button", {name:"Run on alpha once"})).toBeDisabled();
  expect(screen.getByRole("button", {name:"Wait"})).toBeDisabled();
  expect(send).not.toHaveBeenCalled();
});
