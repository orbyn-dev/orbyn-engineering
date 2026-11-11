import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import { installMockFetch } from "../../test-support";
import { accountApi } from "./api";
import { AccountSection } from "./AccountSection";
import { ApprovalCard } from "./ApprovalCard";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

const PASSWORD = "correct horse battery";

async function openSignIn(knobs: () => void = () => undefined) {
  mock.reset("account");
  knobs();
  const user = userEvent.setup();
  render(<AccountSection />);
  await user.click(await screen.findByRole("button", { name: "Sign in" }));
  return user;
}

async function signIn(user: ReturnType<typeof userEvent.setup>, email: string) {
  await user.type(screen.getByLabelText("Email"), email);
  await user.type(screen.getByLabelText("Password"), PASSWORD);
  await user.click(screen.getByRole("button", { name: "Sign in" }));
}

const SLOW = { timeout: 6000 };

describe("a new device waiting for a trusted one", () => {
  it("shows the digits and how long is left, then signs in when it is allowed", { timeout: 20000 }, async () => {
    const user = await openSignIn();
    await signIn(user, "approve@example.com");
    expect(await screen.findByText("Approve this sign-in on another device")).toBeVisible();
    expect(screen.getByText("418 207")).toBeVisible();
    expect(screen.getByText(/Waiting for approval · this request expires in \d+:\d\d/)).toBeVisible();
    // Allowed: a spinner and words, no bar, then the signed-in page.
    expect(await screen.findByText("Signed in on this device · This computer", undefined, SLOW)).toBeVisible();
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("every close from the other side reads the same, whether it was a refusal or a mismatch", { timeout: 30000 }, async () => {
    for (const email of ["deny@example.com", "mismatch@example.com"]) {
      const user = await openSignIn();
      await signIn(user, email);
      expect(await screen.findByText("Denied or cancelled.", undefined, SLOW)).toBeVisible();
      expect(screen.queryByText("The codes didn’t match")).toBeNull();
      await user.click(screen.getByRole("button", { name: "Back to Account" }));
      expect(await screen.findByText("Use an account to sync your settings")).toBeVisible();
      document.body.innerHTML = "";
    }
  });

  it("ten minutes without an answer end it, and Try again goes back to the password", async () => {
    const user = await openSignIn();
    await signIn(user, "late@example.com");
    await screen.findByText("Approve this sign-in on another device");
    mock.setApproval({ answer: "timed_out" });
    expect(await screen.findByText("No one approved this sign-in in time", undefined, SLOW)).toBeVisible();
    expect(screen.getByText(/A request expires after 10 minutes\./)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByLabelText("Password")).toBeVisible();
  });

  it("the person's own tap on They don't match is the one state that says the codes differed", async () => {
    const user = await openSignIn();
    await signIn(user, "late@example.com");
    await screen.findByText("Approve this sign-in on another device");
    const cancel = vi.spyOn(accountApi, "cancelSignIn");
    await user.click(screen.getByRole("button", { name: "They don’t match" }));
    expect(await screen.findByText("The codes didn’t match")).toBeVisible();
    expect(cancel).toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Back to Account" }));
    expect(await screen.findByText("Use an account to sync your settings")).toBeVisible();
    vi.restoreAllMocks();
  });

  it("Cancel gives the request up and returns to sign-in", async () => {
    const user = await openSignIn();
    await signIn(user, "late@example.com");
    await screen.findByText("Approve this sign-in on another device");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByLabelText("Password")).toBeVisible();
    expect(screen.queryByText("418 207")).toBeNull();
  });
});

describe("a trusted device answering", () => {
  async function devicesPage() {
    mock.reset("account");
    await fetch("/api/account/sign-in", { method: "POST", body: JSON.stringify({ email: "dev@example.com", password: PASSWORD }) });
    const user = userEvent.setup();
    render(<AccountSection />);
    await user.click(await screen.findByRole("button", { name: "Manage devices" }));
    return user;
  }

  it("lists a device that asked, and Don't allow refuses it without a password", async () => {
    const user = await devicesPage();
    const row = await screen.findByRole("listitem", { name: "iPad asks to sign in" });
    expect(within(row).getByText(/Waiting for approval · Near Melbourne/)).toBeVisible();
    expect(within(row).getByText("Orbyn Planner")).toBeVisible();
    await user.click(within(row).getByRole("button", { name: "Don’t allow" }));
    await waitFor(() => expect(screen.queryByRole("listitem", { name: "iPad asks to sign in" })).toBeNull());
    expect(screen.queryByRole("dialog", { name: "Confirm it’s you" })).toBeNull();
  });

  it("letting it in needs the password typed for that request", async () => {
    const user = await devicesPage();
    await user.click(await screen.findByRole("button", { name: "Compare codes" }));
    const prompt = await screen.findByRole("dialog", { name: "New sign-in" });
    expect(prompt).toHaveTextContent("Orbyn Planner on iPad");
    expect(prompt).toHaveTextContent("418 207");
    await user.click(within(prompt).getByRole("button", { name: "Codes match · Allow" }));
    const confirm = await screen.findByRole("dialog", { name: "Confirm it’s you" });
    expect(within(confirm).getByText("To let iPad sign in")).toBeVisible();
    expect(within(confirm).getByText("This confirms one action.")).toBeVisible();
    await user.type(within(confirm).getByLabelText("Password"), PASSWORD);
    await user.click(within(confirm).getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(screen.queryByRole("listitem", { name: "iPad asks to sign in" })).toBeNull());
  });

  it("Codes don't match refuses it", async () => {
    const user = await devicesPage();
    await user.click(await screen.findByRole("button", { name: "Compare codes" }));
    const prompt = await screen.findByRole("dialog", { name: "New sign-in" });
    await user.click(within(prompt).getByRole("button", { name: "Codes don’t match" }));
    await waitFor(() => expect(screen.queryByRole("listitem", { name: "iPad asks to sign in" })).toBeNull());
  });

  it("a request that is gone says Denied or cancelled", async () => {
    const user = await devicesPage();
    const row = await screen.findByRole("listitem", { name: "iPad asks to sign in" });
    vi.spyOn(accountApi, "deny").mockRejectedValueOnce(new Error("gone"));
    await user.click(within(row).getByRole("button", { name: "Don’t allow" }));
    expect(await screen.findByText("Denied or cancelled.")).toBeVisible();
    vi.restoreAllMocks();
  });
});

describe("joining this computer's settings", () => {
  async function signedIn(user: ReturnType<typeof userEvent.setup>) {
    await signIn(user, "dev@example.com");
    return screen.findByRole("dialog", { name: "Sync your settings?" });
  }

  it("asks once, with Combine them as the default, and no password for it", async () => {
    const user = await openSignIn(() => mock.setApproval({ connectNeeded: true }));
    const dialog = await signedIn(user);
    expect(within(dialog).getByLabelText(/Combine them/)).toBeChecked();
    expect(within(dialog).getByText("Will sync")).toBeVisible();
    await user.click(within(dialog).getByRole("button", { name: "Connect" }));
    expect(await screen.findByRole("dialog", { name: "Settings connected" })).toHaveTextContent("24 settings synced.");
    expect(screen.queryByRole("dialog", { name: "Confirm it’s you" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Settings connected" })).toBeNull());
  });

  it("the two choices that overwrite ask for the password for that choice", async () => {
    const user = await openSignIn(() => mock.setApproval({ connectNeeded: true }));
    const dialog = await signedIn(user);
    await user.click(within(dialog).getByLabelText(/Use this device’s settings/));
    await user.click(within(dialog).getByRole("button", { name: "Connect" }));
    const confirm = await screen.findByRole("dialog", { name: "Confirm it’s you" });
    expect(within(confirm).getByText("To use this device’s settings everywhere")).toBeVisible();
    await user.type(within(confirm).getByLabelText("Password"), PASSWORD);
    await user.click(within(confirm).getByRole("button", { name: "Confirm" }));
    expect(await screen.findByRole("dialog", { name: "Settings connected" })).toBeVisible();
  });

  it("Not now keeps the settings here, says so, and Connect settings asks again", async () => {
    const user = await openSignIn(() => mock.setApproval({ connectNeeded: true }));
    const dialog = await signedIn(user);
    await user.click(within(dialog).getByRole("button", { name: "Not now" }));
    expect(await screen.findByText(/^Not connected · this computer’s settings stay here/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Connect settings" }));
    expect(await screen.findByRole("dialog", { name: "Sync your settings?" })).toBeVisible();
  });

  it("a computer with no settings of its own is not asked", async () => {
    const user = await openSignIn();
    await signIn(user, "dev@example.com");
    expect(await screen.findByText("Signed in on this device · This computer")).toBeVisible();
    expect(screen.queryByRole("dialog", { name: "Sync your settings?" })).toBeNull();
  });
});

describe("a session the service ended", () => {
  it("says so neutrally, once, and Sign in again opens the sign-in", async () => {
    mock.reset("account");
    mock.setAccount({ sync: ["signed_out", "session_ended"] });
    const user = userEvent.setup();
    render(<AccountSection />);
    const dialog = await screen.findByRole("dialog", { name: "You were signed out" });
    expect(dialog).toHaveTextContent("This device may have been removed, or your sessions ended.");
    expect(dialog).not.toHaveTextContent(/someone/i);
    await user.click(within(dialog).getByRole("button", { name: "Sign in again" }));
    expect(await screen.findByLabelText("Email")).toBeVisible();
    expect(screen.queryByRole("dialog", { name: "You were signed out" })).toBeNull();
    act(() => undefined);
  });
});

describe("an allowed request", () => {
  it("is a spinner and the words, never a bar", () => {
    render(<ApprovalCard approval={{ state: "approved", digits: "418 207", expires_in_secs: 0 }} onChanged={() => undefined} onBack={() => undefined} onMismatch={() => undefined} />);
    expect(screen.getByText("Approved")).toBeVisible();
    expect(screen.getByText("Signing you in…")).toBeVisible();
    expect(screen.queryByRole("progressbar")).toBeNull();
  });
});
