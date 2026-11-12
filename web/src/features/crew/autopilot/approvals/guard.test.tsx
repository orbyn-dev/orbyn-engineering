import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ApprovalTable } from "./ApprovalRows";
import { ApproveMyself } from "./ApproveMyself";
import { DecisionDrawer, type DrawerActions } from "./DecisionDrawer";
import { GuardHold, GuardNeedsYouItem, YouPanel } from "./YouPanel";
import type { ApprovalRow, EngineRef, Mode, YouState } from "./model";

// The guard has no route yet, so these rows are built by hand: what the service will answer with once it does.
const LEAD: EngineRef = { engine: "Claude Code", model: "Opus 5.5", vendor: "Anthropic" };
const CODEX: EngineRef = { engine: "Codex", model: "GPT-5.5", vendor: "OpenAI" };
const OPENCODE: EngineRef = { engine: "OpenCode", model: "DeepSeek V4", vendor: "DeepSeek" };
const CLAUDE: EngineRef = { engine: "Claude Code", model: "Sonnet 5.5", vendor: "Anthropic" };

const target = (name: string, kind: string, at: string, short: string, exit: number | null = null, markedBy: string | null = "prod-targets") => ({
  short, target: name, kind, markedBy, reason: "This command deletes data in a production-linked target.", at, exit,
});

const base: ApprovalRow = {
  id: "g", time: "15:02", at: null, title: "", asker: CODEX, taskId: null, branch: null, risk: null, notes: [], facts: [], block: null, note: null, needsYou: null,
  hard: "guard_two_keys", rule: null, lead: { state: "not_asked" }, leadBy: LEAD, second: { state: "not_asked" }, result: "held_second_key", decidedInMs: null, guard: null,
};
const held: ApprovalRow = { ...base, title: "Delete test orders in prod-db-1", hard: "guard", notes: ["remote database", "guarded"], block: 'psql "$PROD_DATABASE_URL" -c "DELETE FROM orders WHERE is_test = true"', lead: { state: "yes", reason: "It matches the brief’s cleanup of test orders.", short: "it matches the brief’s cleanup of test orders" }, second: { state: "yours" }, result: "held_you", guard: target("prod-db-1", "remote database", "15:02", "DELETE FROM orders WHERE is_test = true", null, null) };
const secondKey: ApprovalRow = { ...base, title: "Delete test orders in prod-db-1", block: held.block, lead: { state: "yes", reason: "It matches the brief’s cleanup of test orders, and the filter names test rows.", short: "it matches the brief’s cleanup of test orders" }, second: { state: "reviewing", by: OPENCODE }, guard: target("prod-db-1", "production-linked", "15:02", "DELETE FROM orders WHERE is_test = true") };
const denied: ApprovalRow = { ...base, id: "d", title: "Drop users.legacy_id on prod-db-1", asker: CLAUDE, branch: "crew/redirect-impl", result: "denied_second_key", block: 'psql "$PROD_DATABASE_URL" -c "ALTER TABLE users DROP COLUMN legacy_id"', lead: { state: "yes", reason: "The column isn’t used in the diff.", short: "the column isn’t used in the diff" }, second: { state: "no", by: CODEX, reason: "a report still reads users.legacy_id." }, note: "The worker was told the second key’s reason and can ask again with a narrower command or after the report changes.", guard: target("prod-db-1", "production-linked", "14:20", "ALTER TABLE users DROP COLUMN legacy_id") };
const noSecond: ApprovalRow = { ...base, id: "n", title: "Truncate audit_log on prod-db-1", asker: CLAUDE, result: "no_second_key", block: 'psql "$PROD_DATABASE_URL" -c "TRUNCATE audit_log"', lead: { state: "yes", reason: "It is a cleanup in the brief.", short: "it is a cleanup in the brief" }, second: { state: "unavailable" }, guard: target("prod-db-1", "production-linked", "13:58", "TRUNCATE audit_log") };
const twoKeys: ApprovalRow = { ...base, id: "t", title: "Delete expired sessions in prod-db-1", asker: OPENCODE, result: "approved_two_keys", block: 'psql "$PROD_DATABASE_URL" -c "DELETE FROM sessions WHERE expires_at < now()"', lead: { state: "yes", reason: "expired sessions, and the brief names them.", short: "expired sessions, in the brief" }, second: { state: "yes", by: CODEX, reason: "the filter matches rows past expires_at." }, guard: target("prod-db-1", "production-linked", "14:05", "DELETE FROM sessions WHERE expires_at < now()", 0) };
const byYou: ApprovalRow = { ...twoKeys, id: "y", hard: "guard", result: "approved_you", leadBy: CLAUDE, second: { state: "you" }, guard: target("prod-db-1", "remote database", "14:05", "DELETE FROM sessions WHERE expires_at < now()", 0) };

function show(row: ApprovalRow, mode: Mode = "full", over: Partial<DrawerActions> = {}) {
  const act: DrawerActions = { approveMyself: () => undefined, signInSecondEngine: () => undefined, close: () => undefined, ...over };
  render(<DecisionDrawer row={row} act={act} mode={mode} />);
  return screen.getByRole("complementary", { name: row.title });
}

describe("the guard under On or Off", () => {
  it("holds a destructive command against a production-linked target for you, in an amber row", () => {
    render(<ApprovalTable rows={[held]} selected={null} onSelect={() => undefined} />);
    const row = screen.getByRole("button", { name: /Delete test orders in prod-db-1/ }).closest('[role="row"]');
    expect(row).toHaveTextContent("GuardYes: it matches the brief’s cleanup of test ordersThe decision is yoursHeld for you");
  });

  it("shows the exact command, the target and why, with the answers waiting for the route", () => {
    const d = show(held, "on");
    expect(d).toHaveTextContent("Held for youGuard");
    expect(d).toHaveTextContent("Targetprod-db-1 · remote database · guarded");
    expect(d).toHaveTextContent("AutopilotOn: the guard holds this command for you");
    expect(d).toHaveTextContent("Waiting since15:02");
    expect(within(d).getByText(/psql "\$PROD_DATABASE_URL" -c "DELETE FROM orders WHERE is_test = true"/)).toBeVisible();
    expect(d).toHaveTextContent("Why it is held");
    expect(d).toHaveTextContent("A held command is never approved in a batch");
    expect(d).toHaveTextContent("Workers run as you, with your machine’s permissions.");
    for (const name of ["Approve once", "Deny"]) {
      expect(within(d).getByRole("button", { name })).toBeDisabled();
      expect(within(d).getByRole("button", { name })).toHaveAttribute("title", "Crew is not connected to this app yet");
    }
  });

  it("shows a command you approved yourself next to the lead's yes, and says you are not a second key", () => {
    const d = show(byYou, "on");
    expect(d).toHaveTextContent("Approved by youGuard");
    expect(d).toHaveTextContent("Approved at14:05 · exit 0");
    expect(d).toHaveTextContent("The leadClaude Code · Sonnet 5.5 · AnthropicYes: expired sessions, and the brief names them.");
    expect(d).toHaveTextContent("YouApproved once, on this computerYou read the command and the target, and approved this one command.");
    expect(d).toHaveTextContent("this isn’t counted as a second key");
  });
});

describe("the guard under Full", () => {
  it("holds a command for a second key and shows the whole chain", () => {
    const d = show(secondKey);
    expect(d).toHaveTextContent("Held for a second keyGuard");
    const steps = within(within(d).getByRole("list", { name: "Decision chain" })).getAllByRole("listitem");
    expect(steps[0]).toHaveTextContent("Hard rule: the guardA destructive command against a production-linked target. It needs two keys.");
    expect(steps[1]).toHaveTextContent("Key 1: the lead said yesIt matches the brief’s cleanup of test orders, and the filter names test rows.");
    expect(steps[2]).toHaveTextContent("Key 2: waitingOpenCode · DeepSeek V4, a different vendor from the lead, is reviewing.");
    expect(d).toHaveTextContent("It can’t be turned off.");
  });

  it("denies when the second key says no, with its reason, and offers to approve it yourself", () => {
    const d = show(denied);
    expect(d).toHaveTextContent("Denied by the second keyGuard");
    expect(d).toHaveTextContent("Targetprod-db-1 · production-linked, marked by the rule prod-targets");
    expect(d).toHaveTextContent("Key 2: noCodex · GPT-5.5, a different vendor: a report still reads users.legacy_id.");
    expect(d).toHaveTextContent("Denied, because one key said no");
    expect(within(d).getByRole("button", { name: "Approve it myself" })).toBeVisible();
  });

  it("denies for now when no second vendor is signed in, next to Approve it myself", () => {
    const d = show(noSecond);
    expect(d).toHaveTextContent("Denied · no second keyGuard");
    expect(d).toHaveTextContent("Key 2: not availableA second key needs a different vendor from the lead. Claude Code is the one vendor signed in.");
    expect(d).toHaveTextContent("Denied for now, and you can approve it yourself");
    expect(within(d).getByRole("button", { name: "Sign in to a second engine" })).toBeVisible();
    expect(within(d).getByRole("button", { name: "Approve it myself" })).toBeVisible();
  });

  it("shows two keys side by side when both said yes", () => {
    const d = show(twoKeys);
    expect(d).toHaveTextContent("Approved by two keysGuard");
    expect(d).toHaveTextContent("Ran at14:05 · exit 0");
    expect(d).toHaveTextContent("Key 1 · the leadClaude Code · Opus 5.5 · AnthropicYes: expired sessions, and the brief names them.");
    expect(d).toHaveTextContent("Key 2 · a second vendorCodex · GPT-5.5 · OpenAIYes: the filter matches rows past expires_at.");
  });

  it("names the command, the target and the reason in a dialog; approving waits for the route and Escape cancels", async () => {
    const user = userEvent.setup();
    const myself = vi.fn();
    const d = show(noSecond, "full", { approveMyself: myself });
    await user.click(within(d).getByRole("button", { name: "Approve it myself" }));
    expect(myself).toHaveBeenCalledWith(noSecond);
  });
});

describe("the guard in Flow and Needs you", () => {
  const held = { command: "Delete test orders in prod-db-1", reason: "This command deletes data in a production-linked target.", key1: "The lead said yes.", key2: "Waiting on OpenCode · DeepSeek V4, a different vendor.", forYou: false };

  it("says Build is waiting on a key, with nothing failed and nothing for you to do", () => {
    const open = vi.fn();
    render(<GuardHold held={held} onOpenLog={open} />);
    const card = screen.getByRole("region", { name: "A held command" });
    expect(card).toHaveTextContent("Build is holding one command for a second key");
    expect(card).toHaveTextContent("Key 1The lead said yes.");
    expect(card).toHaveTextContent("Key 2Waiting on OpenCode · DeepSeek V4, a different vendor.");
    expect(card).toHaveTextContent("Keeps going on everything else. It reaches Checks when the command has run or been denied.");
    expect(card).not.toHaveClass("ap-box-am");
    expect(screen.queryByRole("button", { name: "Approve once" })).toBeNull();
    screen.getByRole("button", { name: "Open in the approval log" }).click();
    expect(open).toHaveBeenCalled();
  });

  it("under On or Off the same card is held for you, with Approve once and Deny", async () => {
    const approve = vi.fn();
    const deny = vi.fn();
    const user = userEvent.setup();
    render(<GuardHold held={{ ...held, forYou: true }} onApprove={approve} onDeny={deny} />);
    const card = screen.getByRole("region", { name: "A held command" });
    expect(card).toHaveClass("ap-box-am");
    expect(card).toHaveTextContent("Held for you");
    await user.click(screen.getByRole("button", { name: "Approve once" }));
    await user.click(screen.getByRole("button", { name: "Deny" }));
    expect(approve).toHaveBeenCalledTimes(1);
    expect(deny).toHaveBeenCalledTimes(1);
  });

  it("lists a denied-for-now command in Needs you with the two ways forward", async () => {
    const signIn = vi.fn();
    const myself = vi.fn();
    const user = userEvent.setup();
    render(<GuardNeedsYouItem onSignIn={signIn} onApproveMyself={myself} onDismiss={() => undefined} />);
    const item = screen.getByRole("article", { name: /Sign in to a second engine to allow a command/ });
    expect(item).toHaveTextContent("Risk · High · Hard to undo");
    expect(item).toHaveTextContent("Nothing is blocked; sign in to a second vendor, or approve the command yourself, once.");
    await user.click(within(item).getByRole("button", { name: "Sign in to a second engine" }));
    await user.click(within(item).getByRole("button", { name: "Approve it myself" }));
    expect(signIn).toHaveBeenCalled();
    expect(myself).toHaveBeenCalled();
  });
});

describe("the You stage of Flow", () => {
  const lines = [
    { label: "Checks", text: "3 of 3 · commit a41c9e2" },
    { label: "Code review", text: "2 lanes · 0 confirmed" },
  ];
  const state = (over: YouState) => over;

  it("is neutral after the merge under Full: nothing waits on you, and the three Review actions are there", async () => {
    const looks = vi.fn();
    render(<YouPanel act={{ looksGood: looks, comment: () => undefined, revert: () => undefined }} state={state({ kind: "after_merge", mergedAt: "14:32 on Sat 7 Nov 2026", risk: "medium", reviewBy: "Sun 8 Nov, 14:32", lines, notVerified: "Other callers of redirect() outside auth/." })} />);
    const panel = screen.getByRole("complementary", { name: "You" });
    expect(panel).toHaveTextContent("You: review what reached mainReview by Sun 8 Nov, 14:32");
    expect(panel).toHaveTextContent("Merged by Crew at 14:32 on Sat 7 Nov 2026 · Medium risk");
    expect(panel).toHaveTextContent("Nothing here waits on you. The merge already happened");
    expect(panel).not.toHaveClass("ap-you-wait");
    await userEvent.click(within(panel).getByRole("button", { name: "Looks good" }));
    expect(looks).toHaveBeenCalled();
    expect(within(panel).getAllByRole("button").map((b) => b.textContent)).toEqual(["Looks good", "Comment", "Revert"]);
  });

  it("says a change merged under a standing rule, and never as if you approved it", () => {
    render(<YouPanel state={state({ kind: "rule", rule: "docs-direct", setBy: "Anh, in .orbyn/crew.json", stages: "Checks, Code review", mergedAt: "14:12 on Sat 7 Nov 2026", text: "This repo gives standing merge authority to changes to documentation alone when every required stage is green." })} act={{ openPr: () => undefined }} />);
    const panel = screen.getByRole("complementary", { name: "You" });
    expect(panel).toHaveTextContent("Merged under a standing ruleMergedRule: docs-direct");
    expect(panel).toHaveTextContent("Set byAnh, in .orbyn/crew.json");
    expect(panel).toHaveTextContent("Required stagesChecks, Code review");
    expect(panel).toHaveTextContent("Bots and agents never merge in the skills library");
    // The rule editor is not built, so Edit and Turn off are not drawn.
    expect(screen.queryByRole("button", { name: "Edit the rule" })).toBeNull();
    expect(screen.getByRole("button", { name: "Open the PR" })).toBeVisible();
  });

  it("says there is nothing to review before the stages are green", () => {
    render(<YouPanel state={state({ kind: "blocked", waiting: 4, of: 5, lines: [{ label: "Security review", text: "running · 1m 40s" }] })} act={{ addReviewer: () => undefined, sendBack: () => undefined }} />);
    const panel = screen.getByRole("complementary", { name: "You" });
    expect(panel).toHaveTextContent("Nothing to review yetNot merged");
    expect(panel).toHaveTextContent("4 of 5 stages before the merge are not green yet");
    expect(panel).toHaveTextContent("You can still read the diff, add a reviewer or send the task back.");
  });

  it("waits for you in amber under On, with the reasons and a one-button return to Full", async () => {
    const full = vi.fn();
    render(<YouPanel state={state({ kind: "waiting_on_you", risk: "high", paths: "auth/login.ts, keys/rotate.ts", size: "2 files · +64 −12 · coverage 71%", lines, notVerified: "key rotation under load." })} act={{ approveAndMerge: () => undefined, sendBack: () => undefined, switchToFull: full }} />);
    const panel = screen.getByRole("complementary", { name: "You" });
    expect(panel).toHaveClass("ap-you-wait");
    expect(panel).toHaveTextContent("High risk: waiting for you before the mergeNeeds you");
    expect(panel).toHaveTextContent("Autopilot is On for this repo");
    expect(panel).toHaveTextContent("Pathsauth/login.ts, keys/rotate.ts");
    expect(panel).toHaveTextContent("Not verified: key rotation under load.");
    await userEvent.click(within(panel).getByRole("button", { name: "Switch this repo to Full" }));
    expect(full).toHaveBeenCalled();
  });

  it("waits for you whatever the risk under Off, and says the risk does not change what waits", () => {
    render(<YouPanel state={state({ kind: "off", risk: "low", riskNote: "A documentation change. Shown so you can review quickly; it does not change what waits.", lines })} act={{ approveAndMerge: () => undefined, approveDontMerge: () => undefined, sendBack: () => undefined }} />);
    const panel = screen.getByRole("complementary", { name: "You" });
    expect(panel).toHaveTextContent("Every change waits for you before the mergeNeeds you");
    expect(panel).toHaveTextContent("Autopilot is Off for this repo");
    expect(panel).toHaveTextContent("Risk · Low");
    expect(within(panel).getAllByRole("button").map((b) => b.textContent)).toEqual(["Approve and merge", "Approve, don’t merge", "Send back to Build"]);
  });
});

describe("approving one command yourself", () => {
  it("names the exact command, the target and the reason; Cancel has focus and Approve once waits for the route", () => {
    const close = vi.fn();
    render(<ApproveMyself row={noSecond} onClose={close} />);
    const dialog = screen.getByRole("dialog", { name: "Approve this command yourself?" });
    expect(dialog).toHaveTextContent('psql "$PROD_DATABASE_URL" -c "TRUNCATE audit_log"');
    expect(dialog).toHaveTextContent("Targetprod-db-1 · production-linked · guarded");
    expect(dialog).toHaveTextContent("CommandTRUNCATE audit_log");
    expect(dialog).toHaveTextContent("ReasonThis command deletes data in a production-linked target.");
    expect(dialog).toHaveTextContent("CoversThis one command, on this target, once");
    expect(dialog).toHaveTextContent("It isn’t a second key, and nothing else is approved with it.");
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    expect(within(dialog).getByRole("button", { name: "Approve once" })).toBeDisabled();
    expect(dialog).toHaveTextContent("Crew is not connected to this app yet");
  });
});
