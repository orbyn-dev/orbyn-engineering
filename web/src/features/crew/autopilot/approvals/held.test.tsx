import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { AutopilotHeld } from "../../../../generated/AutopilotHeld";
import { useConnection } from "../../../../product/state/connection";
import { installMockFetch } from "../../../../test-support";
import { inApp, randomNonce, trustedClick } from "./test-support";
import { ApprovalsTab } from "./ApprovalsTab";
import { decidedLine, heldRow, phaseOf, titleOfCommand } from "./held";

const ORDERS = /DELETE FROM orders/;

let mock: MockApi;
/** What the shell's `crew_presence` does in a test: the bridge the page calls, which a spec replaces per case. */
const invoke = vi.fn<(command: string, args?: Record<string, unknown>) => unknown>();
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  invoke.mockReset();
  invoke.mockImplementation(async () => randomNonce());
});

/** Opens the approval log in the desktop app (the default) or in a plain browser tab. */
async function open(scenario: `crew-${string}`, desktop = true) {
  mock.reset(scenario);
  await inApp(desktop, invoke);
  const user = userEvent.setup();
  render(<ApprovalsTab />);
  await screen.findByRole("region", { name: "Approvals" });
  await screen.findByText(/Needs you · \d/);
  return user;
}

const card = () => screen.getByRole("complementary", { name: /DELETE FROM orders/ });
const approveButton = () => within(card()).getByRole("button", { name: "Approve once" });
const REFUSED = "Orbyn couldn’t confirm it was you. Try again from the desktop app.";
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
    // Only a denial this window sent (it kept the time) is "by you"; the list does not say who else denied one.
    expect(heldRow(item({ state: "denied" }), 600, now, "15:20", false)).toMatchObject({ result: "denied_you", second: { state: "answered" }, held: { decidedAt: "15:20", byYou: true } });
    expect(heldRow(item({ state: "denied" }), 600, now, null, false)).toMatchObject({ result: "denied_held", second: { state: "decided" }, held: { byYou: false } });
    // An approval lasts 600 s; 540 s left means it was given a minute ago.
    expect(heldRow(item({ state: "approved", expires_in_secs: 540 }), 600, now, null, false)).toMatchObject({ result: "approved_you", second: { state: "you" }, held: { decidedAt: "15:03" } });
    expect(heldRow(item({ state: "used", expires_in_secs: 0 }), 600, now, null, false)?.held?.decidedAt).toBeNull();
    expect(heldRow(item({ state: "dismissed" }), 600, now, null, false)).toBeNull();
  });

  it("says who decided and what, with the time only when it is known", () => {
    expect(decidedLine("denied", null, false)).toEqual({ sentence: "Denied.", fact: "Denied" });
    expect(decidedLine("denied", "15:20", true)).toEqual({ sentence: "Denied by you on this device at 15:20.", fact: "15:20 · Denied by you on this device" });
    expect(decidedLine("denied", null, true).sentence).toBe("Denied by you on this device.");
    expect(decidedLine("approved", "15:20", true).sentence).toBe("Approved by you at 15:20.");
    expect(decidedLine("held", null, true)).toEqual({ sentence: null, fact: null });
    const held = heldRow(item(), 600, now, null, false)?.held;
    if (!held) throw new Error("no row");
    expect(phaseOf(held)).toBe("open");
    expect(phaseOf(held, "expired")).toBe("expired");
    expect(phaseOf(held, "decided")).toBe("decided");
  });
});

describe("the held-command card (approval-confirm)", () => {
  it("shows the command and the worker, three answers in order and the restart line, and counts what needs you", async () => {
    const user = await open("crew-held");
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
    expect(d).not.toHaveTextContent("desktop app");
  });

  it("asks the shell on a click the person made, once, and the page draws no dialog of its own", async () => {
    const user = await open("crew-held");
    await openCard(user);
    await trustedClick(approveButton());
    await waitFor(() => expect(screen.getByRole("button", { name: ORDERS }).closest('[role="row"]')).toHaveTextContent("Approved by you"));
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("crew_presence", { holdId: "hold-orders" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls("/api/account")).toHaveLength(0);
  });

  it("sends what the shell returned in x-orbyn-confirm, once, and shows it nowhere (approval-decided)", async () => {
    const nonce = randomNonce();
    invoke.mockImplementation(async () => nonce);
    const user = await open("crew-held");
    await openCard(user);
    await trustedClick(approveButton());
    await screen.findByText("Needs you · 2");
    const sent = calls("/api/crew/approvals/hold-orders/approve-once");
    expect(sent).toHaveLength(1);
    expect(sent[0]?.[1]?.method).toBe("POST");
    expect(new Headers(sent[0]?.[1]?.headers).get("x-orbyn-confirm")).toBe(nonce);
    expect(String(sent[0]?.[1]?.body ?? "")).not.toContain(nonce);
    expect(screen.queryByRole("complementary")).toBeNull();
    expect(screen.getByRole("button", { name: ORDERS }).closest('[role="row"]')).toHaveTextContent("Approved by you");
    // Opening it again says who decided and when, and offers only Close.
    const d = await openCard(user);
    expect(d).toHaveTextContent("Already decidedGuard");
    expect(d).toHaveTextContent("Approved by you");
    expect(within(d).getAllByRole("button").map((b) => b.textContent)).toEqual(["Close"]);
    expect(document.body.textContent).not.toContain(nonce);
    expect(window.location.href).not.toContain(nonce);
    expect(JSON.stringify({ ...localStorage, ...sessionStorage })).not.toContain(nonce);
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("does nothing for a click a script made (not trusted), however it is made", async () => {
    const user = await open("crew-held");
    await openCard(user);
    fireEvent.click(approveButton());
    approveButton().click();
    approveButton().dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await user.click(approveButton());
    expect(invoke).not.toHaveBeenCalled();
    expect(calls("approve-once")).toHaveLength(0);
    expect(card()).toHaveTextContent("Held for you");
    expect(card()).not.toHaveTextContent(REFUSED);
  });

  it("does nothing on a key: Enter, Space and the rest on the card or its button never call the shell", async () => {
    const user = await open("crew-held");
    await openCard(user);
    for (const key of ["Enter", " ", "Return"]) {
      fireEvent.keyDown(card(), { key });
      fireEvent.keyUp(card(), { key });
      fireEvent.keyDown(approveButton(), { key });
      fireEvent.keyUp(approveButton(), { key });
    }
    approveButton().focus();
    await user.keyboard("{Enter}");
    await user.keyboard(" ");
    expect(invoke).not.toHaveBeenCalled();
    expect(calls("approve-once")).toHaveLength(0);
  });

  it("counts a click the browser makes from a keyboard press as the person's own, and waits for Enter to come up first", async () => {
    // A focused button pressed with Enter or Space makes a trusted click with detail 0. That is the button's own activation, and blocking it
    // would shut out keyboard users, so it is allowed. Enter clicks on the press and repeats while held, and Return approves in the dialog,
    // so the page waits for the key to come up before it asks. Space clicks on release, so nothing is held then.
    const user = await open("crew-held");
    await openCard(user);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    await trustedClick(approveButton(), 0);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(invoke).not.toHaveBeenCalled();
    expect(approveButton()).toBeDisabled();
    await act(async () => { window.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter" })); });
    await screen.findByText("Needs you · 2");
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("does not wait for a keyboard click without Enter down (Space), nor for a mouse click", async () => {
    const user = await open("crew-held");
    await openCard(user);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    // A mouse press is not held back, whatever a key is doing.
    await trustedClick(approveButton(), 1);
    await screen.findByText("Needs you · 2");
    expect(invoke).toHaveBeenCalledTimes(1);
    window.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter" }));
    await user.click(screen.getByRole("button", { name: /aws s3 rb/ }));
    await trustedClick(within(screen.getByRole("complementary", { name: /aws s3 rb/ })).getByRole("button", { name: "Approve once" }), 0);
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
  });

  it("gives focus back to Approve once after a cancel and after a refusal", async () => {
    invoke.mockRejectedValueOnce("cancelled").mockRejectedValueOnce("timeout");
    const user = await open("crew-held");
    await openCard(user);
    await trustedClick(approveButton());
    await waitFor(() => expect(approveButton()).toHaveFocus());
    (document.activeElement as HTMLElement).blur();
    await trustedClick(approveButton());
    await waitFor(() => expect(card()).toHaveTextContent(REFUSED));
    await waitFor(() => expect(approveButton()).toHaveFocus());
  });

  it("announces a refusal to screen readers", async () => {
    invoke.mockRejectedValue("timeout");
    const user = await open("crew-held");
    await openCard(user);
    await trustedClick(approveButton());
    expect(await within(card()).findByRole("status")).toHaveTextContent(REFUSED);
  });

  it("turns every card's Approve once off while one request is under way, including after the dialog", async () => {
    const user = await open("crew-held");
    await openCard(user);
    const real = fetch;
    let release: () => void = () => undefined;
    vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
      if (String(input).includes("/approve-once")) await new Promise<void>((resolve) => { release = resolve; });
      return real(input, init);
    }));
    await trustedClick(approveButton());
    await waitFor(() => expect(calls("approve-once")).toHaveLength(1));
    await user.click(screen.getByRole("button", { name: /aws s3 rb/ }));
    expect(within(screen.getByRole("complementary", { name: /aws s3 rb/ })).getByRole("button", { name: "Approve once" })).toBeDisabled();
    await act(async () => release());
    await screen.findByText("Needs you · 2");
  });

  it("the mock service refuses approve-once without the confirmation header", async () => {
    await open("crew-held");
    const sent = await fetch("/api/crew/approvals/hold-orders/approve-once", { method: "POST" });
    expect(sent.status).toBe(403);
    expect(((await sent.json()) as { code: string }).code).toBe("presence_required");
  });

  it("asks one at a time: the button is off while the dialog is open, and a second click starts nothing", async () => {
    let answer: (value: string) => void = () => undefined;
    invoke.mockImplementation(() => new Promise<string>((resolve) => { answer = resolve; }));
    const user = await open("crew-held");
    await openCard(user);
    await trustedClick(approveButton());
    await waitFor(() => expect(approveButton()).toBeDisabled());
    // The handler itself refuses, not only the disabled attribute.
    await trustedClick(approveButton());
    await trustedClick(approveButton());
    expect(invoke).toHaveBeenCalledTimes(1);
    // Other holds wait too: one dialog at a time for the whole window.
    await user.click(screen.getByRole("button", { name: /aws s3 rb/ }));
    expect(within(screen.getByRole("complementary", { name: /aws s3 rb/ })).getByRole("button", { name: "Approve once" })).toBeDisabled();
    answer(randomNonce());
    await screen.findByText("Needs you · 2");
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(calls("approve-once")).toHaveLength(1);
  });

  it("Cancel in the dialog says nothing: the held card is back, with Approve once ready, and nothing is sent", async () => {
    invoke.mockRejectedValue("cancelled");
    const user = await open("crew-held");
    await openCard(user);
    await trustedClick(approveButton());
    await waitFor(() => expect(approveButton()).toBeEnabled());
    expect(card()).toHaveTextContent("Held for you");
    expect(card()).toHaveTextContent("Why it is held");
    expect(card()).not.toHaveTextContent("couldn’t confirm");
    expect(card()).not.toHaveTextContent("Nothing ran");
    expect(screen.queryByRole("alert")).toBeNull();
    expect(calls("approve-once")).toHaveLength(0);
    // No retry of its own, however long it waits.
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it.each(["busy", "timeout", "unavailable", "invalid_hold", "invalid_answer", "forbidden", "dialog_failed", "nonce_failed", "presence_refused:mismatch", "presence_refused:gone", "something_new"])(
    "a refusal from the shell (%s) says it could not confirm it was you, the card stays held and nothing is retried (approval-refused-presence)",
    async (code) => {
      invoke.mockRejectedValue(code);
      const user = await open("crew-held");
      await openCard(user);
      await trustedClick(approveButton());
      const d = await screen.findByRole("complementary", { name: /DELETE FROM orders/ });
      await waitFor(() => expect(d).toHaveTextContent(REFUSED));
      expect(d).toHaveTextContent("Nothing ran. The command is still held.");
      expect(d).toHaveTextContent("Held for you");
      expect(d).not.toHaveTextContent("Why it is held");
      expect(d).not.toHaveTextContent(code.replace(/_/g, " "));
      expect(within(d).getAllByRole("button").map((b) => b.textContent)).toEqual(["Approve once", "Deny", "Dismiss"]);
      for (const name of ["Approve once", "Deny", "Dismiss"]) expect(within(d).getByRole("button", { name })).toBeEnabled();
      expect(calls("approve-once")).toHaveLength(0);
      await new Promise((resolve) => setTimeout(resolve, 50));
      expect(invoke).toHaveBeenCalledTimes(1);
      expect(screen.getByText("Needs you · 3")).toBeVisible();
    },
  );

  it("a bridge that gives no nonce, or something that is not one, is a refusal too", async () => {
    invoke.mockResolvedValue("");
    const user = await open("crew-held");
    await openCard(user);
    await trustedClick(approveButton());
    await waitFor(() => expect(card()).toHaveTextContent(REFUSED));
    invoke.mockResolvedValue({ nonce: "x" });
    await trustedClick(approveButton());
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
    expect(card()).toHaveTextContent(REFUSED);
    expect(calls("approve-once")).toHaveLength(0);
  });

  it("the service not seeing the shell's confirmation (403 presence_required) reads the same, the card stays held and nothing is retried", async () => {
    const user = await open("crew-held-unconfirmed");
    await openCard(user);
    await trustedClick(approveButton());
    await waitFor(() => expect(card()).toHaveTextContent(REFUSED));
    expect(card()).toHaveTextContent("Held for you");
    expect(within(card()).getByRole("button", { name: "Approve once" })).toBeEnabled();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(calls("approve-once")).toHaveLength(1);
    expect(screen.getByText("Needs you · 3")).toBeVisible();
  });

  it("asks again only when the person clicks again, and the new answer clears the message", async () => {
    invoke.mockRejectedValueOnce("timeout");
    const user = await open("crew-held");
    await openCard(user);
    await trustedClick(approveButton());
    await waitFor(() => expect(card()).toHaveTextContent(REFUSED));
    await trustedClick(approveButton());
    await screen.findByText("Needs you · 2");
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(calls("approve-once")).toHaveLength(1);
  });
});

describe("approving in a browser tab (approval-browser)", () => {
  it("does not offer Approve once, says the desktop app is needed, and keeps Deny and Dismiss working", async () => {
    const user = await open("crew-held", false);
    const d = await openCard(user);
    expect(d).toHaveTextContent("Approving once needs the Orbyn desktop app.");
    expect(d).toHaveTextContent("Deny and Dismiss still work here.");
    expect(d).toHaveTextContent("Held for you");
    expect(d).toHaveTextContent("Held requests are cleared if Orbyn restarts.");
    expect(within(d).queryByRole("button", { name: "Approve once" })).toBeNull();
    expect(within(d).getAllByRole("button").map((b) => b.textContent)).toEqual(["Deny", "Dismiss"]);
    for (const name of ["Deny", "Dismiss"]) expect(within(d).getByRole("button", { name })).toBeEnabled();
    expect(invoke).not.toHaveBeenCalled();
    await user.click(within(d).getByRole("button", { name: "Deny" }));
    await screen.findByText("Needs you · 2");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("a window with the service's word that it is the desktop app but no bridge is a browser tab too", async () => {
    mock.reset("crew-held");
    await inApp(true);
    render(<ApprovalsTab />);
    await screen.findByText(/Needs you · \d/);
    await userEvent.setup().click(screen.getByRole("button", { name: ORDERS }));
    expect(card()).toHaveTextContent("Approving once needs the Orbyn desktop app.");
    expect(within(card()).queryByRole("button", { name: "Approve once" })).toBeNull();
  });
});

describe("the old password flow is gone", () => {
  it("asks for no password and no account, and calls no account route", async () => {
    const user = await open("crew-held");
    await openCard(user);
    await trustedClick(approveButton());
    await screen.findByText("Needs you · 2");
    expect(screen.queryByLabelText("Password")).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.body.textContent).not.toMatch(/Confirm it’s you|signed-in account|Sign in/);
    expect(calls("/api/account")).toHaveLength(0);
  });
});

describe("Deny and Dismiss", () => {
  it("never ask for a password: Deny answers at once and the row says Denied by you", async () => {
    const user = await open("crew-held");
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
    const user = await open("crew-held");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Dismiss" }));
    await screen.findByText("Needs you · 2");
    expect(screen.queryByRole("button", { name: ORDERS })).toBeNull();
    expect(calls("/api/crew/approvals/hold-orders/dismiss")).toHaveLength(1);
  });

  it("opening an answered row afterwards says it was already decided, with who and when this window knows", async () => {
    const user = await open("crew-held");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Deny" }));
    await screen.findByText("Needs you · 2");
    const d = await openCard(user);
    expect(d).toHaveTextContent("Already decidedGuard");
    expect(d).toHaveTextContent(/Decided\d\d:\d\d · Denied by you on this device/);
    expect(within(d).getAllByRole("button").map((b) => b.textContent)).toEqual(["Close"]);
  });
});

describe("a request that ended or was decided", () => {
  it("expired: neutral, nothing ran, only Close, and the counts drop (approval-expired)", async () => {
    const user = await open("crew-held-expired");
    await openCard(user);
    await trustedClick(approveButton());
    await waitFor(() => expect(card()).toHaveTextContent("This request expired. Nothing ran."));
    const d = card();
    expect(d).toHaveTextContent("ExpiredGuard");
    expect(d).toHaveTextContent("Held requests are cleared if Orbyn restarts.");
    expect(d).not.toHaveTextContent(REFUSED);
    expect(within(d).getAllByRole("button").map((b) => b.textContent)).toEqual(["Close"]);
    await screen.findByText("Needs you · 2");
    expect(screen.getByRole("button", { name: ORDERS }).closest('[role="row"]')).toHaveTextContent("Nobody answeredExpired");
    await user.click(within(d).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("complementary")).toBeNull();
  });

  it("already decided: says who and what, neutral, only Close, and the counts drop (approval-decided)", async () => {
    const user = await open("crew-held-race");
    await user.click(within(await openCard(user)).getByRole("button", { name: "Deny" }));
    await screen.findByText("Needs you · 2");
    const d = card();
    expect(d).toHaveTextContent("Already decidedGuard");
    expect(d).toHaveTextContent("Already decided.");
    // Denied from another window: the list does not say who, so the card does not say "by you".
    expect(d).toHaveTextContent("Denied. Nothing ran.");
    expect(d).not.toHaveTextContent("by you");
    expect(within(d).getAllByRole("button").map((b) => b.textContent)).toEqual(["Close"]);
    expect(screen.getByRole("button", { name: ORDERS }).closest('[role="row"]')).toHaveTextContent("DecidedDenied");
    expect(screen.getByRole("button", { name: ORDERS }).closest('[role="row"]')).not.toHaveTextContent("by you");
  });

  it("already decided when Approve once is pressed (409): nothing is approved", async () => {
    const user = await open("crew-held-race");
    await openCard(user);
    await trustedClick(approveButton());
    await waitFor(() => expect(card()).toHaveTextContent("Already decided."));
    expect(card()).toHaveTextContent("Denied. Nothing ran.");
    expect(within(card()).getAllByRole("button").map((b) => b.textContent)).toEqual(["Close"]);
    await screen.findByText("Needs you · 2");
  });

  it("refused by the always-denied list at approval time: the hold is closed as denied and nothing ran", async () => {
    const user = await open("crew-held-hardrule");
    await openCard(user);
    await trustedClick(approveButton());
    await waitFor(() => expect(card()).toHaveTextContent("Already decided."));
    const d = card();
    expect(d).toHaveTextContent("Denied. Nothing ran.");
    expect(d).not.toHaveTextContent("by you");
    expect(within(d).getAllByRole("button").map((b) => b.textContent)).toEqual(["Close"]);
    await screen.findByText("Needs you · 2");
    expect(screen.getByRole("button", { name: ORDERS }).closest('[role="row"]')).toHaveTextContent("Denied");
  });

  describe("the approve route fails after the shell gave a nonce", () => {
    /** Fails the first approve-once request at the network; `landed` lets it reach the service first, as a lost answer does. */
    function dropApprove(landed: boolean) {
      const real = fetch;
      let dropped = false;
      vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
        if (String(input).includes("/approve-once") && !dropped) {
          dropped = true;
          if (landed) await real(input, init);
          throw new TypeError("Failed to fetch");
        }
        return real(input, init);
      }));
    }

    it("still held: says so, is not the refusal text, and nothing asks again by itself", async () => {
      const user = await open("crew-held");
      await openCard(user);
      dropApprove(false);
      await trustedClick(approveButton());
      expect(await within(card()).findByText("Can’t reach the Orbyn service. Nothing was changed. The request is still held.")).toBeVisible();
      expect(card()).not.toHaveTextContent(REFUSED);
      expect(card()).toHaveTextContent("Held for you");
      expect(invoke).toHaveBeenCalledTimes(1);
      expect(within(card()).getByRole("button", { name: "Approve once" })).toBeEnabled();
      await new Promise((resolve) => setTimeout(resolve, 50));
      expect(invoke).toHaveBeenCalledTimes(1);
      expect(calls("approve-once")).toHaveLength(1);
    });

    it("the request landed and only the answer was lost: reads the list and shows it approved, not still held", async () => {
      const user = await open("crew-held");
      await openCard(user);
      dropApprove(true);
      await trustedClick(approveButton());
      await screen.findByText("Needs you · 2");
      expect(screen.getByRole("button", { name: ORDERS }).closest('[role="row"]')).toHaveTextContent("Approved by you");
      expect(screen.queryByText(/still held/)).toBeNull();
    });

    it("the list cannot be read either: does not claim it is still held", async () => {
      const user = await open("crew-held");
      await openCard(user);
      const real = fetch;
      vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
        const url = String(input);
        if (url.includes("/approve-once") || url.endsWith("/approvals/held")) throw new TypeError("Failed to fetch");
        return real(input, init);
      }));
      await trustedClick(approveButton());
      expect(await within(card()).findByText(/Couldn’t tell whether it was approved/)).toBeVisible();
      expect(card()).not.toHaveTextContent("still held");
    });
  });
});

describe("a remote viewer", () => {
  it("cannot answer: the three buttons are off with the service's own words", async () => {
    useConnection.setState({ remote: true });
    const user = await open("crew-held");
    const d = await openCard(user);
    for (const name of ["Approve once", "Deny", "Dismiss"]) expect(within(d).getByRole("button", { name })).toBeDisabled();
    expect(d).toHaveTextContent("Do this from the computer the app runs on.");
  });
});

describe("Crew Preview off", () => {
  it("reads the held list, finds nothing and draws nothing of it", async () => {
    mock.reset("default");
    const { container } = render(<ApprovalsTab />);
    await waitFor(() => expect(calls("/api/crew/approvals/held").length).toBeGreaterThan(0));
    expect(screen.queryByText(/Needs you ·/)).toBeNull();
    expect(container.textContent).not.toContain("Held for you");
  });
});
