import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import { installMockFetch } from "../../test-support";
import { ApiFailure } from "../../api/client";
import { accountApi } from "./api";
import { announceAccountChange } from "./changes";
import { AccountSection } from "./AccountSection";
import { ApprovalCard } from "./ApprovalCard";
import { ConnectProgress } from "./ConnectDialogs";

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
    expect(screen.getByRole("status")).toHaveTextContent(/^Waiting for approval$/);
    expect(screen.getByText(/this request expires in \d+:\d\d/)).toBeVisible();
    expect(screen.getByLabelText("Code 4 1 8, 2 0 7")).toBeVisible();
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
    mock.setAccount({ answer: "timed_out" });
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
    vi.spyOn(accountApi, "deny").mockRejectedValueOnce(new ApiFailure("request_gone", "That request has ended.", 404));
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
    const user = await openSignIn(() => mock.setAccount({ connectNeeded: true }));
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
    const user = await openSignIn(() => mock.setAccount({ connectNeeded: true }));
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
    const user = await openSignIn(() => mock.setAccount({ connectNeeded: true }));
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

describe("answering a request, the edges", () => {
  async function devicesPage() {
    mock.reset("account");
    await fetch("/api/account/sign-in", { method: "POST", body: JSON.stringify({ email: "dev@example.com", password: PASSWORD }) });
    const user = userEvent.setup();
    render(<AccountSection />);
    await user.click(await screen.findByRole("button", { name: "Manage devices" }));
    return user;
  }

  it("a request that ended before the password was confirmed reads Denied or cancelled.", async () => {
    const user = await devicesPage();
    await user.click(await screen.findByRole("button", { name: "Compare codes" }));
    const prompt = await screen.findByRole("dialog", { name: "New sign-in" });
    await user.click(within(prompt).getByRole("button", { name: "Codes match · Allow" }));
    const confirm = await screen.findByRole("dialog", { name: "Confirm it’s you" });
    mock.setAccount({ dropIncoming: "req-ipad" });
    await user.type(within(confirm).getByLabelText("Password"), PASSWORD);
    await user.click(within(confirm).getByRole("button", { name: "Confirm" }));
    expect(await within(confirm).findByText("Denied or cancelled.")).toBeVisible();
    expect(within(confirm).queryByText("That request has ended.")).toBeNull();
  });

  it("the prompt closes when its request leaves the list", async () => {
    const user = await devicesPage();
    await user.click(await screen.findByRole("button", { name: "Compare codes" }));
    await screen.findByRole("dialog", { name: "New sign-in" });
    mock.setAccount({ dropIncoming: "req-ipad" });
    act(() => announceAccountChange());
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "New sign-in" })).toBeNull());
  });

  it("a double click refuses once, and a refusal that needs the password does not read as a close", async () => {
    const user = await devicesPage();
    const deny = vi.spyOn(accountApi, "deny");
    const row = await screen.findByRole("listitem", { name: "iPad asks to sign in" });
    const button = within(row).getByRole("button", { name: "Don’t allow" });
    await user.dblClick(button);
    await waitFor(() => expect(screen.queryByRole("listitem", { name: "iPad asks to sign in" })).toBeNull());
    expect(deny).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("Denied or cancelled.")).toBeNull();
    vi.restoreAllMocks();
  });

  it("a refusal the service answers with reauth_required says what it says", async () => {
    const user = await devicesPage();
    vi.spyOn(accountApi, "deny").mockRejectedValueOnce(new ApiFailure("reauth_required", "Sign in to answer this.", 403));
    const row = await screen.findByRole("listitem", { name: "iPad asks to sign in" });
    await user.click(within(row).getByRole("button", { name: "Don’t allow" }));
    expect(await screen.findByText("Sign in to answer this.")).toBeVisible();
    expect(screen.queryByText("Denied or cancelled.")).toBeNull();
    vi.restoreAllMocks();
  });
});

describe("the waiting screen over time", () => {
  it("counts down between looks and takes the service's time again on each look", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const view = { state: "waiting" as const, digits: "418 207", expires_in_secs: 600 };
      const poll = vi.spyOn(accountApi, "pollApproval").mockResolvedValue({ ...view, expires_in_secs: 580 });
      render(<ApprovalCard approval={view} onChanged={() => undefined} onBack={() => undefined} onMismatch={() => undefined} />);
      expect(screen.getByText(/expires in 10:00/)).toBeVisible();
      act(() => { vi.advanceTimersByTime(1000); });
      expect(screen.getByText(/expires in 9:59/)).toBeVisible();
      // At the first look the service says 580: the screen follows it, not its own count.
      await act(async () => { vi.advanceTimersByTime(1000); });
      await waitFor(() => expect(screen.getByText(/expires in 9:40/)).toBeVisible());
      expect(poll).toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
      vi.restoreAllMocks();
    }
  });

  it("after three failed looks in a row says so quietly, with Cancel", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      vi.spyOn(accountApi, "pollApproval").mockRejectedValue(new Error("down"));
      render(<ApprovalCard approval={{ state: "waiting", digits: "418 207", expires_in_secs: 600 }} onChanged={() => undefined} onBack={() => undefined} onMismatch={() => undefined} />);
      await act(async () => { vi.advanceTimersByTime(6500); });
      expect(await screen.findByRole("alert")).toHaveTextContent("Couldn’t check this request");
      expect(screen.getByRole("heading", { name: "Couldn’t check this request" })).not.toHaveFocus();
      expect(screen.getByRole("button", { name: "Cancel" })).toBeVisible();
    } finally {
      vi.useRealTimers();
      vi.restoreAllMocks();
    }
  });

  it("keeps the clock out of the live region and gives a closed card an alert and the focus", async () => {
    render(<ApprovalCard approval={{ state: "denied", digits: "418 207", expires_in_secs: 0 }} onChanged={() => undefined} onBack={() => undefined} onMismatch={() => undefined} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Denied or cancelled.");
    expect(screen.getByRole("heading", { name: "Denied or cancelled." })).toHaveFocus();
  });
});

describe("the first sync, while it runs", () => {
  it("is a spinner and words with no count, no bar and no Cancel", () => {
    render(<ConnectProgress onDismiss={() => undefined} />);
    expect(screen.getByRole("dialog", { name: "Connecting your settings" })).toBeVisible();
    expect(screen.getByText("Combining settings…")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
    expect(screen.queryByRole("progressbar")).toBeNull();
    expect(screen.queryByText(/\d+ of \d+/)).toBeNull();
    expect(screen.getByRole("button", { name: "Keep working" })).toBeVisible();
  });

  it("shows the progress dialog while the first sync takes its time, then the summary", { timeout: 20000 }, async () => {
    const user = await openSignIn();
    await signIn(user, "connect@example.com");
    const choice = await screen.findByRole("dialog", { name: "Sync your settings?" });
    await user.click(within(choice).getByRole("button", { name: "Connect" }));
    expect(await screen.findByRole("dialog", { name: "Connecting your settings" })).toBeVisible();
    expect(screen.queryByRole("dialog", { name: "Sync your settings?" })).toBeNull();
    expect(await screen.findByRole("dialog", { name: "Settings connected" }, { timeout: 8000 })).toBeVisible();
  });
});

describe("a session the service ended, told once", () => {
  it("is not said again after it was dismissed, even in a new window", async () => {
    mock.reset("account");
    mock.setAccount({ sync: ["signed_out", "session_ended"] });
    const user = userEvent.setup();
    const first = render(<AccountSection />);
    const dialog = await screen.findByRole("dialog", { name: "You were signed out" });
    await user.click(within(dialog).getByRole("button", { name: "Not now" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "You were signed out" })).toBeNull());
    first.unmount();
    render(<AccountSection />);
    expect(await screen.findByText("Use an account to sync your settings")).toBeVisible();
    expect(screen.queryByRole("dialog", { name: "You were signed out" })).toBeNull();
  });
});
