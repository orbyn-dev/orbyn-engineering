// The guard's states as the service writes them to the ledger (Crew Designs frames `guard-*`), read through the mock's `crew-guard` scenarios:
// the row each state becomes, the card behind it, and the one path that approves a held command (the desktop shell's own dialog).
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import { useConnection } from "../../../../product/state/connection";
import { installMockFetch } from "../../../../test-support";
import { crewApi } from "./api";
import { ApprovalsTab } from "./ApprovalsTab";
import { ApprovalCards } from "./ApprovalRows";
import { learn, rowOf, type Exit, type GuardBody } from "./ledger";
import { inApp, randomNonce, trustedClick } from "./test-support";

let mock: MockApi;
const invoke = vi.fn<(command: string, args?: Record<string, unknown>) => unknown>();
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  invoke.mockReset();
  invoke.mockImplementation(async () => randomNonce());
});

async function open(scenario: `crew-${string}`, desktop = true) {
  mock.reset(scenario);
  await inApp(desktop, invoke);
  const user = userEvent.setup();
  render(<ApprovalsTab />);
  await screen.findByRole("region", { name: "Approvals" });
  await screen.findByRole("table", { name: "Approval log" });
  return user;
}

const calls = (part: string) => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes(part));
const rowOf_ = (name: string | RegExp, at = 0) => screen.getAllByRole("button", { name })[at]?.closest('[role="row"]') as HTMLElement;
const frame = () => document.querySelector("[data-frame^='guard-']")?.getAttribute("data-frame");
const ORDERS = /DELETE FROM orders/;
const TRUNCATE = /TRUNCATE audit_log/;
const AUDIT = /DELETE FROM audit_log/;
const SESSIONS = /DELETE FROM sessions/;
const DROP = /DROP COLUMN legacy_id/;

describe("a command held for you (guard-held)", () => {
  it("is one amber row, with the target the held row of the ledger names, and answers on the card", async () => {
    const user = await open("crew-guard");
    // The hold is the request: the guard's own denial under On is not listed beside it.
    expect(screen.getAllByRole("button", { name: ORDERS })).toHaveLength(1);
    expect(rowOf_(ORDERS)).toHaveTextContent("Held for you");
    await user.click(screen.getByRole("button", { name: ORDERS }));
    const card = screen.getByRole("complementary", { name: ORDERS });
    expect(frame()).toBe("guard-held");
    expect(card).toHaveTextContent("Targetprod-db-1 · remote database · guarded");
    expect(card).toHaveTextContent("AutopilotOn: the guard holds this command for you");
    expect(within(card).getAllByRole("button").map((b) => b.textContent)).toEqual(["Approve once", "Deny", "Dismiss"]);
  });

  it("in a browser tab says approving once needs the desktop app, and keeps Deny and Dismiss", async () => {
    const user = await open("crew-guard", false);
    await user.click(screen.getByRole("button", { name: ORDERS }));
    const card = screen.getByRole("complementary", { name: ORDERS });
    expect(card).toHaveTextContent("Approving once needs the Orbyn desktop app.");
    expect(within(card).getAllByRole("button").map((b) => b.textContent)).toEqual(["Deny", "Dismiss"]);
  });
});

describe("denied because no second vendor is signed in (guard-denied-no-second, guard-approve-myself)", () => {
  it("is one row for the command, not two: the row takes the live hold", async () => {
    await open("crew-guard-full");
    expect(screen.getAllByRole("button", { name: TRUNCATE })).toHaveLength(1);
    expect(rowOf_(TRUNCATE)).toHaveTextContent("Denied · no second key");
    expect(screen.queryByText("Held for you")).toBeNull();
    // The hold waits once: the denied row took it, and the count says one.
    expect(screen.getByText("Needs you · 1")).toBeVisible();
  });

  it("shows the chain with the second key unavailable, and offers to sign in and to approve it yourself", async () => {
    const user = await open("crew-guard-full");
    await user.click(screen.getByRole("button", { name: TRUNCATE }));
    const card = screen.getByRole("complementary", { name: TRUNCATE });
    expect(frame()).toBe("guard-denied-no-second");
    expect(card).toHaveTextContent("Denied · no second keyGuard");
    expect(card).toHaveTextContent("Hard rule: the guardTwo keys are needed for this target.");
    expect(card).toHaveTextContent("Key 1: the lead said yesIt is a cleanup in the brief.");
    expect(card).toHaveTextContent("Key 2: not available");
    expect(within(card).getAllByRole("button").map((b) => b.textContent)).toEqual(["Sign in to a second engine", "Approve it myself", "Deny", "Dismiss", "Close"]);
  });

  it("hands Approve it myself to the shell for the live hold, then sends the nonce once, in the header", async () => {
    const nonce = randomNonce();
    let release: (value: string) => void = () => undefined;
    invoke.mockImplementation(() => new Promise<string>((resolve) => { release = resolve; }));
    const user = await open("crew-guard-full");
    await user.click(screen.getByRole("button", { name: TRUNCATE }));
    await trustedClick(within(screen.getByRole("complementary", { name: TRUNCATE })).getByRole("button", { name: "Approve it myself" }));
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("crew_presence", { holdId: "hold-audit-2" });
    // The dialog is the shell's: while it is open the card stays, in its own frame, and nothing was sent.
    expect(frame()).toBe("guard-approve-myself");
    expect(calls("/approve-once")).toHaveLength(0);
    release(nonce);
    await waitFor(() => expect(calls("/api/crew/approvals/hold-audit-2/approve-once")).toHaveLength(1));
    const sent = calls("/api/crew/approvals/hold-audit-2/approve-once")[0];
    expect(new Headers(sent?.[1]?.headers).get("x-orbyn-confirm")).toBe(nonce);
    expect(document.body.textContent).not.toContain(nonce);
  });

  it("in a browser tab keeps the other buttons and says it needs the desktop app, with no way to approve", async () => {
    const user = await open("crew-guard-full", false);
    await user.click(screen.getByRole("button", { name: TRUNCATE }));
    const card = screen.getByRole("complementary", { name: TRUNCATE });
    expect(card).toHaveTextContent("Approving once needs the Orbyn desktop app.");
    expect(within(card).queryByRole("button", { name: "Approve it myself" })).toBeNull();
    expect(within(card).getByRole("button", { name: "Sign in to a second engine" })).toBeVisible();
    expect(invoke).not.toHaveBeenCalled();
  });
});

/** Makes the next answers to approve-once what a test says: a refusal of the service, or a lost network. */
function answerApproveWith(how: "presence_required" | "policy_denied" | "network") {
  const real = fetch;
  vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
    if (!String(input).includes("/approve-once")) return real(input, init);
    if (how === "network") throw new TypeError("Failed to fetch");
    return new Response(JSON.stringify({ code: how, message: how === "policy_denied" ? "Denied by Autopilot (N4)." : "Approving needs the person's confirmation." }), { status: 403 });
  }));
}

describe("answering the hold behind a denied row (guard-denied-no-second)", () => {
  const openJoined = async (user: ReturnType<typeof userEvent.setup>) => {
    await user.click(screen.getByRole("button", { name: TRUNCATE }));
    return screen.getByRole("complementary", { name: TRUNCATE });
  };

  it("says the shell could not confirm it was you, on this row, and keeps the hold answerable", async () => {
    const user = await open("crew-guard-full");
    answerApproveWith("presence_required");
    const card = await openJoined(user);
    await trustedClick(within(card).getByRole("button", { name: "Approve it myself" }));
    expect(await within(card).findByText("Orbyn couldn’t confirm it was you. Try again from the desktop app.")).toBeVisible();
    expect(within(card).getByRole("button", { name: "Approve it myself" })).toBeEnabled();
    expect(within(card).getByRole("button", { name: "Deny" })).toBeEnabled();
  });

  it("says nothing was changed when the network is lost, and that the request is still held", async () => {
    const user = await open("crew-guard-full");
    answerApproveWith("network");
    const card = await openJoined(user);
    await trustedClick(within(card).getByRole("button", { name: "Approve it myself" }));
    expect(await within(card).findByRole("alert")).toHaveTextContent("Can’t reach the Orbyn service. Nothing was changed. The request is still held.");
  });

  it("says it is already decided when the always-denied list refused it now, and offers no answer for a closed hold", async () => {
    const user = await open("crew-guard-full");
    answerApproveWith("policy_denied");
    const card = await openJoined(user);
    await trustedClick(within(card).getByRole("button", { name: "Approve it myself" }));
    expect(await within(card).findByText("Already decided.")).toBeVisible();
    expect(within(card).queryByRole("button", { name: /Approve it myself|Deny|Dismiss/ })).toBeNull();
  });

  it("answers the hold on Deny, by the hold's id", async () => {
    const user = await open("crew-guard-full");
    const card = await openJoined(user);
    await user.click(within(card).getByRole("button", { name: "Deny" }));
    await waitFor(() => expect(calls("/api/crew/approvals/hold-audit-2/deny")).toHaveLength(1));
  });

  it("in a window that reaches the app from another computer shows the answers off, with the reason", async () => {
    const user = await open("crew-guard-full");
    useConnection.setState({ remote: true });
    const card = await openJoined(user);
    expect(within(card).getByRole("button", { name: "Approve it myself" })).toBeDisabled();
    expect(within(card).getByRole("button", { name: "Deny" })).toBeDisabled();
    expect(card).toHaveTextContent("Do this from the computer the app runs on.");
  });
});

describe("denied by the second key (guard-denied)", () => {
  it("is one row, though the service stores a disagreement and the denial it caused", async () => {
    await open("crew-guard-full");
    expect(screen.getAllByRole("button", { name: DROP })).toHaveLength(1);
    expect(rowOf_(DROP)).toHaveTextContent("Denied by the second key");
  });

  it("shows both keys with the second key's reason, and no way to approve: no hold was made", async () => {
    const user = await open("crew-guard-full");
    await user.click(screen.getByRole("button", { name: DROP }));
    const card = screen.getByRole("complementary", { name: DROP });
    expect(frame()).toBe("guard-denied");
    expect(card).toHaveTextContent("Key 1: the lead said yesThe column isn’t used in the diff.");
    expect(card).toHaveTextContent("Key 2: nocodex, a different vendor: a report still reads users.legacy_id.");
    expect(card).toHaveTextContent("Denied, because one key said no");
    expect(within(card).queryByRole("button", { name: "Approve it myself" })).toBeNull();
  });
});

describe("approved (guard-approved, guard-approved-person, guard-approved-person-unknown)", () => {
  it("by two keys: both reasons side by side, and the exit status the later command_exit row gave", async () => {
    const user = await open("crew-guard-full");
    expect(rowOf_(SESSIONS)).toHaveTextContent("Approved by two keys");
    await user.click(screen.getByRole("button", { name: SESSIONS }));
    const card = screen.getByRole("complementary", { name: SESSIONS });
    expect(frame()).toBe("guard-approved");
    expect(card).toHaveTextContent("Targetprod-db-1 · remote database, listed in this device’s prod_targets");
    expect(card).toHaveTextContent("Ran at14:05 · exit 0");
    expect(card).toHaveTextContent("Key 1 · the leadYes: Expired sessions, and the brief names them.");
    expect(card).toHaveTextContent("Key 2 · a second vendorcodexYes: the filter matches rows past expires_at.");
  });

  it("by you: the one key is yours, and it says you are not a second key", async () => {
    const user = await open("crew-guard");
    expect(rowOf_(AUDIT)).toHaveTextContent("Approved by you");
    expect(rowOf_(AUDIT)).toHaveTextContent("You · once");
    await user.click(screen.getByRole("button", { name: AUDIT }));
    const card = screen.getByRole("complementary", { name: AUDIT });
    expect(frame()).toBe("guard-approved-person");
    expect(card).toHaveTextContent("Approved at16:02 · exit 0");
    expect(card).toHaveTextContent("YouApproved once, on this computer");
    expect(card).not.toHaveTextContent("The lead");
    expect(card).toHaveTextContent("You are the final authority; this isn’t counted as a second key.");
    expect(card).not.toHaveTextContent("Exit status unknown");
  });

  it("by you with no exit status: says so in neutral words, never a number", async () => {
    const user = await open("crew-guard-unknown");
    await user.click(screen.getByRole("button", { name: AUDIT }));
    const card = screen.getByRole("complementary", { name: AUDIT });
    expect(frame()).toBe("guard-approved-person-unknown");
    expect(card).toHaveTextContent("Approved by youGuardExit status unknown");
    expect(card).toHaveTextContent("Approved at16:02 · exit status unknown");
    expect(card).toHaveTextContent("The engine never reported an exit status for it");
    expect(card).not.toHaveTextContent(/exit 0/);
  });
});

describe("a guarded command in the 320 px panel (guard-narrow-panel)", () => {
  it("is a small card with its time, result and command for each request", async () => {
    mock.reset("crew-guard-full");
    const page = await crewApi.approvals({ offset: 0, limit: 50 });
    const exits = new Map<number, Exit>();
    const holds = new Map<string, { guard: GuardBody; at: string }>();
    learn(page.rows, exits, holds);
    const rows = page.rows.flatMap((one) => rowOf(one, [], [], { exits, holds }) ?? []);
    const opened: string[] = [];
    render(<ApprovalCards rows={rows.slice(0, 4)} onSelect={(id) => opened.push(id)} />);
    const cards = screen.getAllByRole("button");
    expect(cards.map((c) => c.textContent)).toEqual([
      expect.stringContaining("16:02"),
      expect.stringContaining("14:05"),
      expect.stringContaining("14:20"),
      expect.stringContaining("13:58"),
    ]);
    expect(cards[0]).toHaveTextContent("Approved by you");
    expect(cards[1]).toHaveTextContent("Approved by two keys");
    expect(cards[2]).toHaveTextContent("Denied by the second key");
    expect(cards[3]).toHaveTextContent("Denied · no second key");
    await userEvent.click(cards[2] as HTMLElement);
    expect(opened).toEqual([String(rows[2]?.id)]);
  });
});
