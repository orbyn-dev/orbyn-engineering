import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import { installMockFetch } from "../../../../test-support";
import { ApprovalsTab } from "./ApprovalsTab";
import { GuardHold, GuardNeedsYouItem, YouPanel } from "./YouPanel";
import type { YouState } from "./model";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

async function open(scenario: "crew" | `crew-${string}`) {
  mock.reset(scenario);
  const user = userEvent.setup();
  render(<ApprovalsTab />);
  await screen.findByRole("region", { name: "Approvals" });
  return user;
}
const drawer = (name: string) => screen.getByRole("complementary", { name });

describe("the guard under On or Off", () => {
  it("holds a destructive command against a production-linked target for you", async () => {
    const user = await open("crew-guard-on");
    expect(screen.getByText("Autopilot On · the guard holds guarded commands for you")).toBeVisible();
    expect(screen.getByText("Guard · production-linked targets")).toBeVisible();
    const row = screen.getByRole("button", { name: /Delete test orders in prod-db-1/ }).closest('[role="row"]');
    expect(row).toHaveTextContent("GuardYes: it matches the brief’s cleanup of test ordersThe decision is yoursHeld for you");
    await user.click(screen.getByRole("button", { name: /Delete test orders/ }));
    const d = drawer("Delete test orders in prod-db-1");
    expect(d).toHaveTextContent("Held for youGuard");
    expect(d).toHaveTextContent("Targetprod-db-1 · remote database · guarded");
    expect(d).toHaveTextContent("AutopilotOn: the guard holds this command for you");
    expect(d).toHaveTextContent("Waiting since15:02");
    expect(within(d).getByText(/psql "\$PROD_DATABASE_URL" -c "DELETE FROM orders WHERE is_test = true"/)).toBeVisible();
    expect(d).toHaveTextContent("Why it is held");
    expect(d).toHaveTextContent("A held command is never approved in a batch");
    expect(d).toHaveTextContent("Workers run as you, with your machine’s permissions.");
  });

  it("answers one held command at a time: Approve once takes it out of the list", async () => {
    const user = await open("crew-guard-on");
    await user.click(screen.getByRole("button", { name: /Delete test orders/ }));
    await user.click(within(drawer("Delete test orders in prod-db-1")).getByRole("button", { name: "Approve once" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: /Delete test orders/ })).toBeNull());
    expect(screen.queryByRole("complementary")).toBeNull();
  });

  it("shows a command you approved yourself next to the lead's yes, and says you are not a second key", async () => {
    const user = await open("crew-guard-on");
    await user.click(screen.getByRole("button", { name: /Delete expired sessions/ }));
    const d = drawer("Delete expired sessions in prod-db-1");
    expect(d).toHaveTextContent("Approved by youGuard");
    expect(d).toHaveTextContent("Approved at14:05 · exit 0");
    expect(d).toHaveTextContent("The leadClaude Code · Sonnet 5.5 · AnthropicYes: expired sessions, in the brief");
    expect(d).toHaveTextContent("YouApproved once, on this computerYou read the command and the target, and approved this one command.");
    expect(d).toHaveTextContent("this isn’t counted as a second key");
  });
});

describe("the guard under Full", () => {
  it("holds a command for a second key and shows the whole chain", async () => {
    const user = await open("crew-guard");
    await user.click(screen.getByRole("button", { name: /Delete test orders/ }));
    const d = drawer("Delete test orders in prod-db-1");
    expect(d).toHaveTextContent("Held for a second keyGuard");
    const steps = within(within(d).getByRole("list", { name: "Decision chain" })).getAllByRole("listitem");
    expect(steps[0]).toHaveTextContent("Hard rule: the guardA destructive command against a production-linked target. It needs two keys.");
    expect(steps[1]).toHaveTextContent("Key 1: the lead said yesIt matches the brief’s cleanup of test orders, and the filter names test rows.");
    expect(steps[2]).toHaveTextContent("Key 2: waitingOpenCode · DeepSeek V4, a different vendor from the lead, is reviewing.");
    expect(d).toHaveTextContent("It can’t be turned off.");
  });

  it("denies when the second key says no, with its reason, and offers to approve it yourself", async () => {
    const user = await open("crew-guard");
    await user.click(screen.getByRole("button", { name: /Drop users\.legacy_id/ }));
    const d = drawer("Drop users.legacy_id on prod-db-1");
    expect(d).toHaveTextContent("Denied by the second keyGuard");
    expect(d).toHaveTextContent("Targetprod-db-1 · production-linked, marked by the rule prod-targets");
    expect(d).toHaveTextContent("Key 2: noCodex · GPT-5.5, a different vendor: a report still reads users.legacy_id.");
    expect(d).toHaveTextContent("Denied, because one key said no");
    expect(within(d).getByRole("button", { name: "Approve it myself" })).toBeVisible();
  });

  it("denies for now when no second vendor is signed in, next to Approve it myself", async () => {
    const user = await open("crew-guard");
    await user.click(screen.getByRole("button", { name: /Truncate audit_log/ }));
    const d = drawer("Truncate audit_log on prod-db-1");
    expect(d).toHaveTextContent("Denied · no second keyGuard");
    expect(d).toHaveTextContent("Key 2: not availableA second key needs a different vendor from the lead. Claude Code is the one vendor signed in.");
    expect(d).toHaveTextContent("Denied for now, and you can approve it yourself");
    expect(within(d).getByRole("button", { name: "Sign in to a second engine" })).toBeVisible();
    expect(within(d).getByRole("button", { name: "Approve it myself" })).toBeVisible();
  });

  it("shows two keys side by side when both said yes", async () => {
    const user = await open("crew-guard");
    await user.click(screen.getByRole("button", { name: /Delete expired sessions/ }));
    const d = drawer("Delete expired sessions in prod-db-1");
    expect(d).toHaveTextContent("Approved by two keysGuard");
    expect(d).toHaveTextContent("Ran at14:05 · exit 0");
    expect(d).toHaveTextContent("Key 1 · the leadClaude Code · Opus 5.5 · AnthropicYes: expired sessions, and the brief names them.");
    expect(d).toHaveTextContent("Key 2 · a second vendorCodex · GPT-5.5 · OpenAIYes: the filter matches rows past expires_at.");
  });

  it("approves one command yourself in a dialog that names the command, the target and the reason", async () => {
    const user = await open("crew-guard");
    await user.click(screen.getByRole("button", { name: /Truncate audit_log/ }));
    await user.click(within(drawer("Truncate audit_log on prod-db-1")).getByRole("button", { name: "Approve it myself" }));
    const dialog = await screen.findByRole("dialog", { name: "Approve this command yourself?" });
    expect(dialog).toHaveTextContent('psql "$PROD_DATABASE_URL" -c "TRUNCATE audit_log"');
    expect(dialog).toHaveTextContent("Targetprod-db-1 · production-linked · guarded");
    expect(dialog).toHaveTextContent("CommandTRUNCATE audit_log");
    expect(dialog).toHaveTextContent("ReasonThis command deletes data in a production-linked target.");
    expect(dialog).toHaveTextContent("CoversThis one command, on this target, once");
    expect(dialog).toHaveTextContent("It isn’t a second key, and nothing else is approved with it.");
    // The dialog is a confirmation: Cancel has focus, so one key does not approve a deletion.
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.click(within(dialog).getByRole("button", { name: "Approve once" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(screen.queryByRole("button", { name: /Truncate audit_log/ })).toBeNull());
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
