import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { AutopilotHeld } from "../../../../generated/AutopilotHeld";
import { useConnection } from "../../../../product/state/connection";
import { installMockFetch } from "../../../../test-support";
import { ApprovalsTab } from "./ApprovalsTab";
import { decidedLine, heldRow, phaseOf, titleOfCommand } from "./held";

const PASSWORD = "correct horse battery";
const ORDERS = /DELETE FROM orders/;

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false });
});

async function open(scenario: `crew-${string}`) {
  mock.reset(scenario);
  const user = userEvent.setup();
  render(<ApprovalsTab />);
  await screen.findByRole("region", { name: "Approvals" });
  await screen.findByText(/Needs you · \d/);
  return user;
}

const card = () => screen.getByRole("complementary", { name: /DELETE FROM orders/ });
const dialog = () => screen.getByRole("dialog", { name: "Confirm it’s you" });
const calls = (part: string) => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes(part));

async function openCard(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: ORDERS }));
  return card();
}

const item = (over: Partial<AutopilotHeld> = {}): AutopilotHeld => ({
  id: "h1", task_id: "task-1", crew_id: "crew-1", kind: "command", command: 'psql "$DB" -c "DELETE FROM orders"\nsecond line', reason: "The command is not in the brief", state: "held", readable: true, age_secs: 120, expires_in_secs: 480, ...over,
});

describe("a held command as a row", () => {
  const now = new Date(2026, 10, 12, 15, 4, 0).getTime();

  it("is the first line of the command, waiting since the time it was held, and decided by you only", () => {
    expect(titleOfCommand("  ls -la\nrm -rf x")).toBe("ls -la");
    expect(titleOfCommand("x".repeat(120))).toHaveLength(90);
    const row = heldRow(item(), 600, now, null, false);
    expect(row).toMatchObject({ id: "h1", time: "15:02", title: 'psql "$DB" -c "DELETE FROM orders"', result: "held_you", second: { state: "yours" }, hard: "guard" });
    expect(row?.block).toContain("second line");
    expect(row?.lead).toEqual({ state: "reason", reason: "The command is not in the brief", short: "The command is not in the brief" });
  });

  it("reads the state: expired, denied by you, approved by you; a dismissed one is not listed", () => {
    expect(heldRow(item({ state: "expired" }), 600, now, null, false)).toMatchObject({ result: "held_expired", second: { state: "nobody" } });
    expect(heldRow(item({ state: "held" }), 600, now, null, true)).toMatchObject({ result: "held_expired" });
    expect(heldRow(item({ state: "denied" }), 600, now, "15:20", false)).toMatchObject({ result: "denied_you", second: { state: "answered" }, held: { decidedAt: "15:20" } });
    // An approval lasts 600 s; 540 s left means it was given a minute ago.
    expect(heldRow(item({ state: "approved", expires_in_secs: 540 }), 600, now, null, false)).toMatchObject({ result: "approved_you", second: { state: "you" }, held: { decidedAt: "15:03" } });
    expect(heldRow(item({ state: "used", expires_in_secs: 0 }), 600, now, null, false)?.held?.decidedAt).toBeNull();
    expect(heldRow(item({ state: "dismissed" }), 600, now, null, false)).toBeNull();
  });

  it("says who decided and what, with the time only when it is known", () => {
    expect(decidedLine("denied", null)).toEqual({ sentence: "Denied by you on this device.", fact: "Denied by you on this device" });
    expect(decidedLine("denied", "15:20")).toEqual({ sentence: "Denied by you on this device at 15:20.", fact: "15:20 · Denied by you on this device" });
    expect(decidedLine("approved", "15:20").sentence).toBe("Approved by you at 15:20.");
    expect(decidedLine("held", null)).toEqual({ sentence: null, fact: null });
    const held = heldRow(item(), 600, now, null, false)?.held;
    if (!held) throw new Error("no row");
    expect(phaseOf(held)).toBe("open");
    expect(phaseOf(held, "expired")).toBe("expired");
    expect(phaseOf(held, "decided")).toBe("decided");
  });
});

describe("the held-command card (approval-confirm)", () => {
  it("shows the command and the worker, three answers in order and the restart line, and counts what needs you", async () => {
    const user = await open("crew-held-account");
    expect(screen.getByText("Needs you · 3")).toBeVisible();
    const table = screen.getByRole("table", { name: "Approval log" });
    expect(within(table).getAllByRole("row")[1]).toHaveTextContent(/DELETE FROM orders WHERE is_test = true"Task task-redirectGuardThe command is not in the briefThe decision is yoursHeld for you/);
    const d = await openCard(user);
    expect(d).toHaveTextContent("Held for youGuard");
    expect(d).toHaveTextContent("Task task-redirect");
    expect(d).toHaveTextContent("AutopilotFull: the guard holds this command for you");
    expect(d).toHaveTextContent("Waiting since");
    expect(d.querySelector("pre code")).toHaveTextContent('psql "$PROD_DATABASE_URL" -c "DELETE FROM orders WHERE is_test = true"');
    expect(d).toHaveTextContent("Why it is heldThe command is not in the brief. Nothing runs until you approve it.");
    expect(d).toHaveTextContent("Held requests are cleared if Orbyn restarts.");
    expect(d).toHaveTextContent("Workers run as you, with your machine’s permissions.");
    expect(within(d).getAllByRole("button").map((b) => b.textContent)).toEqual(["Approve once", "Deny", "Dismiss"]);
    for (const name of ["Approve once", "Deny", "Dismiss"]) expect(within(d).getByRole("button", { name })).toBeEnabled();
    expect(d).not.toHaveTextContent("Crew is not connected");
  });

  it("opens Confirm it’s you with the command, the password focused and Approve once off until something is typed", async () => {
    const user = await open("crew-held-account");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Approve once" }));
    const dlg = dialog();
    expect(within(dlg).getByLabelText("Password")).toHaveFocus();
    expect(dlg).toHaveTextContent("Enter your password to continue.");
    expect(dlg).toHaveTextContent("Task task-redirect");
    expect(within(dlg).getByText(/DELETE FROM orders WHERE is_test = true/)).toBeVisible();
    expect(dlg).toHaveTextContent("This confirms one action.");
    expect(dlg).not.toHaveTextContent(/nonce|10 minutes|ten minutes/i);
    expect(within(dlg).getByRole("button", { name: "Approve once" })).toBeDisabled();
    expect(within(dlg).getAllByRole("button").map((b) => b.textContent)).toEqual(["Cancel", "Approve once"]);
    expect(calls("approve-once")).toHaveLength(0);
  });

  it("Cancel, Escape and Enter on an empty field close it and nothing runs", async () => {
    const user = await open("crew-held-account");
    const d = await openCard(user);
    await user.click(within(d).getByRole("button", { name: "Approve once" }));
    await user.click(within(dialog()).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await user.click(within(card()).getByRole("button", { name: "Approve once" }));
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await user.click(within(card()).getByRole("button", { name: "Approve once" }));
    // Escape in a native dialog is its `cancel` event.
    fireEvent(dialog(), new Event("cancel", { cancelable: true }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("approve-once")).toHaveLength(0);
    expect(calls("/api/account/confirm")).toHaveLength(0);
    expect(card()).toHaveTextContent("Held for you");
  });

  it("a wrong password clears the field, says so, and the command stays held (approval-confirm-wrong)", async () => {
    const user = await open("crew-held-account");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Approve once" }));
    await user.type(within(dialog()).getByLabelText("Password"), "nope");
    await user.click(within(dialog()).getByRole("button", { name: "Approve once" }));
    expect(await screen.findByText("That isn’t your password. Check it and try again.")).toBeVisible();
    expect(within(dialog()).getByLabelText("Password")).toHaveValue("");
    expect(within(dialog()).getByRole("button", { name: "Approve once" })).toBeDisabled();
    expect(calls("approve-once")).toHaveLength(0);
    await user.click(within(dialog()).getByRole("button", { name: "Cancel" }));
    expect(card()).toHaveTextContent("Held for you");
    expect(within(card()).getByRole("button", { name: "Deny" })).toBeEnabled();
  });

  it("keeps the password and offers Try again when Orbyn cannot be reached; the hold stays (approval-confirm-network)", async () => {
    const user = await open("crew-held-account");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Approve once" }));
    mock.fail("/api/account/confirm", 1, 503, "account_offline");
    await user.type(within(dialog()).getByLabelText("Password"), PASSWORD);
    await user.click(within(dialog()).getByRole("button", { name: "Approve once" }));
    expect(await within(dialog()).findByText("Couldn’t reach Orbyn. Check your connection and try again. Nothing was changed. The request is still held.")).toBeVisible();
    expect(within(dialog()).getByLabelText("Password")).toHaveValue(PASSWORD);
    expect(within(dialog()).queryByRole("button", { name: "Retry" })).toBeNull();
    expect(within(dialog()).getByRole("button", { name: "Try again" })).toBeEnabled();
    expect(calls("approve-once")).toHaveLength(0);
    await user.click(within(dialog()).getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("approve-once")).toHaveLength(1);
  });

  it("approves once: the nonce goes in x-orbyn-confirm, the password goes nowhere else, the count drops", async () => {
    const user = await open("crew-held-account");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Approve once" }));
    await user.type(within(dialog()).getByLabelText("Password"), PASSWORD);
    await user.click(within(dialog()).getByRole("button", { name: "Approve once" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const confirm = calls("/api/account/confirm").filter(([url]) => !String(url).includes("cancel"));
    expect(confirm).toHaveLength(1);
    expect(JSON.parse(String(confirm[0]?.[1]?.body))).toEqual({ password: PASSWORD, action: "approve_device", device_id: "crew-approve-once:hold-orders", policy: null });
    const sent = calls("/api/crew/approvals/hold-orders/approve-once");
    expect(sent).toHaveLength(1);
    expect(sent[0]?.[1]?.method).toBe("POST");
    expect(new Headers(sent[0]?.[1]?.headers).get("x-orbyn-confirm")).toMatch(/^nonce-/);
    expect(String(sent[0]?.[1]?.body ?? "")).not.toContain(PASSWORD);
    expect(JSON.stringify(vi.mocked(fetch).mock.calls.filter(([url]) => !String(url).includes("/api/account/confirm")))).not.toContain(PASSWORD);
    await screen.findByText("Needs you · 2");
    expect(screen.queryByRole("complementary")).toBeNull();
    expect(screen.getByRole("button", { name: ORDERS }).closest('[role="row"]')).toHaveTextContent("Approved by you");
    // The nonce is not shown, kept or put in the address.
    expect(document.body.textContent).not.toContain("nonce-");
    expect(window.location.href).not.toContain("nonce");
    expect(Object.keys(localStorage).join()).not.toContain("nonce");
  });
});

describe("Deny and Dismiss", () => {
  it("never ask for a password: Deny answers at once and the row says Denied by you", async () => {
    const user = await open("crew-held-account");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Deny" }));
    await screen.findByText("Needs you · 2");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls("/api/account")).toHaveLength(calls("/api/account").filter(([url]) => String(url) === "/api/account").length);
    const sent = calls("/api/crew/approvals/hold-orders/deny");
    expect(sent).toHaveLength(1);
    expect(new Headers(sent[0]?.[1]?.headers).get("x-orbyn-confirm")).toBeNull();
    expect(screen.getByRole("button", { name: ORDERS }).closest('[role="row"]')).toHaveTextContent("Answered by youDenied by you");
  });

  it("Dismiss closes the request: it leaves the log and the count", async () => {
    const user = await open("crew-held-account");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Dismiss" }));
    await screen.findByText("Needs you · 2");
    expect(screen.queryByRole("button", { name: ORDERS })).toBeNull();
    expect(calls("/api/crew/approvals/hold-orders/dismiss")).toHaveLength(1);
  });

  it("opening an answered row afterwards says it was already decided, with who and when this window knows", async () => {
    const user = await open("crew-held-account");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Deny" }));
    await screen.findByText("Needs you · 2");
    const d = await openCard(user);
    expect(d).toHaveTextContent("Already decidedGuard");
    expect(d).toHaveTextContent(/Decided\d\d:\d\d · Denied by you on this device/);
    expect(within(d).getAllByRole("button").map((b) => b.textContent)).toEqual(["Close"]);
  });
});

describe("a signed-in account is needed to approve (approval-confirm-noaccount)", () => {
  it("turns Approve once off, says why with a Sign in button, and keeps Deny and Dismiss working", async () => {
    const user = await open("crew-held");
    const d = await openCard(user);
    expect(d).toHaveTextContent("Approving needs a signed-in account.");
    expect(d).toHaveTextContent("Deny and Dismiss still work without one.");
    expect(d).toHaveTextContent("Held for you");
    expect(d).not.toHaveTextContent("Why it is held");
    expect(within(d).getByRole("button", { name: "Approve once" })).toBeDisabled();
    expect(within(d).getByRole("button", { name: "Deny" })).toBeEnabled();
    expect(within(d).getByRole("button", { name: "Dismiss" })).toBeEnabled();
    expect(within(d).getByRole("button", { name: "Sign in" })).toBeEnabled();
    await user.click(within(d).getByRole("button", { name: "Deny" }));
    await screen.findByText("Needs you · 2");
  });

  it("shows the same state when the service refuses approving for the development stand-in account", async () => {
    const user = await open("crew-held-account-standin");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Approve once" }));
    await user.type(within(dialog()).getByLabelText("Password"), PASSWORD);
    await user.click(within(dialog()).getByRole("button", { name: "Approve once" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(card()).toHaveTextContent("Approving needs a signed-in account.");
    expect(within(card()).getByRole("button", { name: "Approve once" })).toBeDisabled();
    // The confirmation it made was given up, not left live.
    expect(calls("/api/account/confirm/cancel")).toHaveLength(1);
  });
});

describe("a request that ended or was decided", () => {
  it("expired: neutral, nothing ran, only Close, and the counts drop (approval-expired)", async () => {
    const user = await open("crew-held-account-expired");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Approve once" }));
    await user.type(within(dialog()).getByLabelText("Password"), PASSWORD);
    await user.click(within(dialog()).getByRole("button", { name: "Approve once" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const d = card();
    expect(d).toHaveTextContent("ExpiredGuard");
    expect(d).toHaveTextContent("This request expired. Nothing ran.");
    expect(d).toHaveTextContent("Held requests are cleared if Orbyn restarts.");
    expect(within(d).getAllByRole("button").map((b) => b.textContent)).toEqual(["Close"]);
    await screen.findByText("Needs you · 2");
    expect(screen.getByRole("button", { name: ORDERS }).closest('[role="row"]')).toHaveTextContent("Nobody answeredExpired");
    expect(calls("/api/account/confirm/cancel")).toHaveLength(1);
    await user.click(within(d).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("complementary")).toBeNull();
  });

  it("already decided: says who and what, neutral, only Close, and the counts drop (approval-decided)", async () => {
    const user = await open("crew-held-account-race");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Deny" }));
    await screen.findByText("Needs you · 2");
    const d = card();
    expect(d).toHaveTextContent("Already decidedGuard");
    expect(d).toHaveTextContent("Already decided.");
    expect(d).toHaveTextContent("Denied by you on this device. Nothing ran.");
    expect(within(d).getAllByRole("button").map((b) => b.textContent)).toEqual(["Close"]);
    expect(screen.getByRole("button", { name: ORDERS }).closest('[role="row"]')).toHaveTextContent("Denied by you");
  });
});

describe("a remote viewer", () => {
  it("cannot answer: the three buttons are off with the service's own words", async () => {
    useConnection.setState({ remote: true });
    const user = await open("crew-held-account");
    const d = await openCard(user);
    for (const name of ["Approve once", "Deny", "Dismiss"]) expect(within(d).getByRole("button", { name })).toBeDisabled();
    expect(d).toHaveTextContent("Do this from the computer the app runs on.");
  });
});

describe("Crew Preview off", () => {
  it("reads no held list and draws nothing of it", async () => {
    mock.reset("default");
    const { container } = render(<ApprovalsTab />);
    await waitFor(() => expect(calls("/api/crew/approvals/held").length).toBeGreaterThan(0));
    expect(screen.queryByText(/Needs you ·/)).toBeNull();
    expect(container.textContent).not.toContain("Held for you");
  });
});
