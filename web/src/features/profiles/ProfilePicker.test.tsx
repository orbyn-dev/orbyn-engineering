import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createProfilesMock } from "../../../mock/profiles";
import type { Thread } from "../../api/types";
import { useWorkspace } from "../../product/state/workspace";
import { forgetApplications, ProfilePicker } from "./ProfilePicker";
import { setProfilesApi } from "./api";

const thread = (patch: Record<string, unknown> = {}) => ({ id: "t1", engine: "claude", model: "claude-opus", lifecycle: { permission_mode: "supervised" }, ...patch }) as unknown as Thread;
let refreshed: ReturnType<typeof vi.fn>;
beforeEach(() => {
  refreshed = vi.fn(async () => undefined);
  useWorkspace.setState({ refreshThread: refreshed as never, threads: { t1: thread() } });
});
afterEach(() => { forgetApplications(); setProfilesApi(null); });

async function open(onError = vi.fn(), disabled = false) {
  const user = userEvent.setup();
  const view = render(<ProfilePicker thread={thread()} disabled={disabled} onError={onError} />);
  const trigger = await screen.findByRole("button", { name: /^Profile:/ });
  return { user, trigger, onError, ...view };
}
const pick = async (user: ReturnType<typeof userEvent.setup>, trigger: HTMLElement, name: string) => {
  await user.click(trigger);
  await user.click(await screen.findByRole("menuitem", { name: new RegExp(`^${name}`) }));
};

it("draws nothing and asks only once when the service has no profiles route", async () => {
  const fetching = vi.fn(async () => new Response(JSON.stringify({ code: "not_found", message: "x" }), { status: 404 }));
  vi.stubGlobal("fetch", fetching);
  const first = render(<ProfilePicker thread={thread()} disabled={false} onError={vi.fn()} />);
  await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
  expect(first.container).toBeEmptyDOMElement();
  const second = render(<ProfilePicker thread={thread()} disabled={false} onError={vi.fn()} />);
  await act(async () => { await Promise.resolve(); });
  expect(second.container).toBeEmptyDOMElement();
  expect(fetching).toHaveBeenCalledTimes(1);
});

it("reads No profile on a thread that has none, and lists the profiles with their settings", async () => {
  setProfilesApi(createProfilesMock());
  const { user, trigger } = await open();
  expect(trigger).toHaveTextContent("No profile");
  await user.click(trigger);
  const menu = await screen.findByRole("menu", { name: "Profiles" });
  expect(within(menu).getAllByRole("menuitem")).toHaveLength(6);
  expect(within(menu).getByText(/Choosing a profile sets the engine, model and permission mode/)).toBeVisible();
});

it("applies a profile that grants nothing directly, with no ack_grants and no confirm, then reads the thread again", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  const { user, trigger } = await open();
  await pick(user, trigger, "Careful");
  await waitFor(() => expect(mock.writes).toEqual(["apply:thread:t1:builtin:careful:ack="]));
  expect(refreshed).toHaveBeenCalledWith("t1");
  expect(screen.queryByRole("dialog")).toBeNull();
  await waitFor(() => expect(screen.getByRole("button", { name: /^Profile:/ })).toHaveTextContent("Careful"));
});

it("asks for one confirm listing the grants of a full-access profile, and sends them acknowledged", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  const { user, trigger } = await open();
  await pick(user, trigger, "Hands off");
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).getByText("Full access: runs any command without asking.")).toBeVisible();
  expect(within(dialog).getByText("Autopilot Full: merges when checks pass.")).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
  expect(mock.writes).toEqual([]);
  await user.click(within(dialog).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(mock.writes).toEqual(["apply:thread:t1:01j0000000000000000000a003:ack=full_access+autopilot_full"]));
});

it("writes nothing when the confirm is cancelled, and asks again next time", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  const { user, trigger } = await open();
  await pick(user, trigger, "Hands off");
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Cancel" }));
  expect(mock.writes).toEqual([]);
  await pick(user, trigger, "Hands off");
  expect(await screen.findByRole("dialog")).toBeVisible();
});

it("shows the confirm on a server confirm_required for a profile the page thought was safe", async () => {
  const stale = createProfilesMock();
  const real = stale.applyToThread;
  let calls = 0;
  stale.applyToThread = async (id, body) => { calls += 1; if (calls === 1) { const { ApiFailure } = await import("../../api/client"); throw new ApiFailure("confirm_required", "x", 409, { grants: ["full_access"] }); } return real(id, body); };
  setProfilesApi(stale);
  const { user, trigger } = await open();
  await pick(user, trigger, "Careful");
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).getByText("Full access: runs any command without asking.")).toBeVisible();
  expect(within(dialog).queryByText(/Autopilot Full/)).toBeNull();
});

it("disables a profile whose engine is not startable, with its reason, and never applies it", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  const { user, trigger } = await open();
  await user.click(trigger);
  const item = await screen.findByRole("menuitem", { name: /^Codex builder/ });
  expect(item).toBeDisabled();
  expect(item).toHaveAttribute("title", expect.stringContaining("Codex is not installed."));
  await user.click(item);
  expect(mock.writes).toEqual([]);
});

it("reports a refusal in words and keeps the thread as it was", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  const { user, trigger, onError } = await open();
  mock.failNext("engine_unavailable");
  await pick(user, trigger, "Fast");
  await waitFor(() => expect(onError).toHaveBeenLastCalledWith(expect.stringContaining("Couldn’t apply Fast.")));
  expect(refreshed).not.toHaveBeenCalled();
  expect(trigger).toHaveTextContent("No profile");
});

it("explains a run in progress", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  const { user, trigger, onError } = await open();
  mock.failNext("run_active");
  await pick(user, trigger, "Fast");
  await waitFor(() => expect(onError).toHaveBeenLastCalledWith(expect.stringMatching(/can.t change during a run/)));
});

it("is off while a turn runs", async () => {
  setProfilesApi(createProfilesMock());
  const { trigger } = await open(vi.fn(), true);
  expect(trigger).toBeDisabled();
});

it("reads Name, edited once the thread's model differs from what the profile set", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  const user = userEvent.setup();
  const view = render(<ProfilePicker thread={thread()} disabled={false} onError={vi.fn()} />);
  const trigger = await screen.findByRole("button", { name: /^Profile:/ });
  await pick(user, trigger, "Careful");
  await waitFor(() => expect(screen.getByRole("button", { name: /^Profile:/ })).toHaveTextContent(/^Careful$/));
  view.rerender(<ProfilePicker thread={thread({ model: "claude-haiku" })} disabled={false} onError={vi.fn()} />);
  expect(screen.getByRole("button", { name: /^Profile:/ })).toHaveTextContent("Careful, edited");
});
