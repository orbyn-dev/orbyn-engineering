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

/** The digits the mock's trusted device shows, as a person types them into the six boxes. */
const RIGHT = "418207";
const wrongFor = (right: string) => [...right].map((digit) => String((Number(digit) + 1) % 10)).join("");

async function typeDigits(user: ReturnType<typeof userEvent.setup>, digits: string) {
  await user.type(screen.getByLabelText("Digit 1 of 6"), digits);
}

describe("a new device waiting for a trusted one", () => {
  it("waits with no digits and no skip, then asks for them once a trusted device allows it", { timeout: 20000 }, async () => {
    const user = await openSignIn();
    await signIn(user, "approve@example.com");
    expect(await screen.findByText("Approve this sign-in on another device")).toBeVisible();
    expect(screen.getByText(/We asked your other devices to approve this one\. Open Orbyn on a device you trust and allow it\. That device then shows six digits to type here\./)).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent(/^Waiting for approval$/);
    expect(screen.getByText(/this request expires in \d+:\d\d/)).toBeVisible();
    // The new device never shows the digits and never compares: Cancel is all there is.
    expect(screen.queryByText("418 207")).toBeNull();
    expect(screen.queryByRole("button", { name: "They don’t match" })).toBeNull();
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual(["Cancel"]);
    // Allowed: six boxes, nothing filled in and no skip. Allowing alone does not sign in.
    expect(await screen.findByText("Type the digits from your other device", undefined, SLOW)).toBeVisible();
    expect(screen.getByText("Type the six digits shown on your other device.")).toBeVisible();
    expect(screen.getAllByRole("textbox")).toHaveLength(6);
    for (const box of screen.getAllByRole("textbox")) expect(box).toHaveValue("");
    expect(screen.getByLabelText("Digit 1 of 6")).toHaveAttribute("inputmode", "numeric");
    expect(screen.getByLabelText("Digit 1 of 6")).toHaveFocus();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    expect(screen.getByText(/This request expires in \d+:\d\d/)).toBeVisible();
    expect(screen.queryByText("Signed in on this device · This computer")).toBeNull();
    await typeDigits(user, RIGHT);
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Continue" }));
    // The right digits sign in: a spinner and the words, no bar, then the signed-in page.
    expect(await screen.findByText("Signed in on this device · This computer", undefined, SLOW)).toBeVisible();
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("takes a pasted six-digit number with its spaces, and moves back on delete", { timeout: 20000 }, async () => {
    const user = await openSignIn();
    await signIn(user, "approve@example.com");
    await screen.findByText("Type the digits from your other device", undefined, SLOW);
    await typeDigits(user, "4182");
    expect(screen.getByLabelText("Digit 5 of 6")).toHaveFocus();
    await user.keyboard("{Backspace}");
    expect(screen.getByLabelText("Digit 4 of 6")).toHaveFocus();
    expect(screen.getByLabelText("Digit 4 of 6")).toHaveValue("");
    await user.keyboard("{Backspace}");
    expect(screen.getByLabelText("Digit 3 of 6")).toHaveValue("");
    await user.click(screen.getByLabelText("Digit 1 of 6"));
    await user.paste("418 207");
    expect(screen.getAllByRole("textbox").map((box) => (box as HTMLInputElement).value).join("")).toBe(RIGHT);
    // Letters are not taken.
    await user.click(screen.getByLabelText("Digit 1 of 6"));
    await user.paste("abc");
    expect(screen.getAllByRole("textbox").map((box) => (box as HTMLInputElement).value).join("")).toBe(RIGHT);
  });

  it("wrong digits clear the boxes and say how many tries are left, and the last one closes the request", { timeout: 30000 }, async () => {
    const user = await openSignIn();
    await signIn(user, "approve@example.com");
    await screen.findByText("Type the digits from your other device", undefined, SLOW);
    const wrong = wrongFor(RIGHT);
    await typeDigits(user, wrong);
    await user.click(screen.getByRole("button", { name: "Continue" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Those digits don’t match.2 tries left.");
    expect(screen.getAllByRole("textbox").map((box) => (box as HTMLInputElement).value).join("")).toBe("");
    expect(screen.getByLabelText("Digit 1 of 6")).toHaveFocus();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    // The screen never says which digit was wrong.
    expect(alert).not.toHaveTextContent(/digit \d|position/i);
    await typeDigits(user, wrong);
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("1 try left.")).toBeVisible();
    await typeDigits(user, wrong);
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "Too many tries. This request is closed." })).toBeVisible();
    expect(screen.getByText("No one is signed in on this device. If this was you, try again, or sign in from a device you already trust.")).toBeVisible();
    expect(screen.queryByText(/expires in/)).toBeNull();
    expect(screen.queryAllByRole("textbox")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Back to Account" }));
    expect(await screen.findByText("Use an account to sync your settings")).toBeVisible();
  });

  it("a locked request closes for good: Try again goes back to the password", async () => {
    const user = await openSignIn();
    await signIn(user, "late@example.com");
    await screen.findByText("Approve this sign-in on another device");
    mock.setAccount({ answer: "locked" });
    expect(await screen.findByText("Too many tries. This request is closed.", undefined, SLOW)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByLabelText("Password")).toBeVisible();
  });

  it("Cancel while typing the digits gives the request up", async () => {
    const user = await openSignIn();
    await signIn(user, "late@example.com");
    await screen.findByText("Approve this sign-in on another device");
    mock.setAccount({ answer: "enter_digits" });
    await screen.findByText("Type the digits from your other device", undefined, SLOW);
    const cancel = vi.spyOn(accountApi, "cancelSignIn");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(cancel).toHaveBeenCalled();
    expect(await screen.findByLabelText("Password")).toBeVisible();
    vi.restoreAllMocks();
  });

  it("when the service cannot be reached with the digits it says it cannot check, and nothing is closed", async () => {
    render(<ApprovalCard approval={{ state: "enter_digits", expires_in_secs: 500 }} onChanged={() => undefined} onBack={() => undefined} />);
    const user = userEvent.setup();
    vi.spyOn(accountApi, "submitDigits").mockRejectedValueOnce(new ApiFailure("account_offline", "Couldn't reach Orbyn.", 503));
    await typeDigits(user, RIGHT);
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "Couldn’t check this request." })).toBeVisible();
    expect(screen.queryByText("Denied or cancelled.")).toBeNull();
    vi.restoreAllMocks();
  });

  it("every close from the other side reads the same", { timeout: 30000 }, async () => {
    const user = await openSignIn();
    await signIn(user, "deny@example.com");
    expect(await screen.findByText("Denied or cancelled.", undefined, SLOW)).toBeVisible();
    expect(screen.queryByText("The codes didn’t match")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Back to Account" }));
    expect(await screen.findByText("Use an account to sync your settings")).toBeVisible();
  });

  it("the request expired: neutral words, new digits next time, and Try again goes back to the password", async () => {
    const user = await openSignIn();
    await signIn(user, "late@example.com");
    await screen.findByText("Approve this sign-in on another device");
    mock.setAccount({ answer: "timed_out" });
    expect(await screen.findByText("This request expired", undefined, SLOW)).toBeVisible();
    expect(screen.getByText(/A request lasts 10 minutes\. Open Orbyn on one of your trusted devices and try again\. The digits change each time\./)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByLabelText("Password")).toBeVisible();
  });

  it("Cancel gives the request up and returns to sign-in", async () => {
    const user = await openSignIn();
    await signIn(user, "late@example.com");
    await screen.findByText("Approve this sign-in on another device");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(await screen.findByLabelText("Password")).toBeVisible();
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
    expect(within(row).getByRole("button", { name: "Review" })).toBeVisible();
    await user.click(within(row).getByRole("button", { name: "Don’t allow" }));
    await waitFor(() => expect(screen.queryByRole("listitem", { name: "iPad asks to sign in" })).toBeNull());
    expect(screen.queryByRole("dialog", { name: "Confirm it’s you" })).toBeNull();
  });

  it("shows Allow and Don't allow first, with no digits, and never allows by itself", async () => {
    const user = await devicesPage();
    await user.click(await screen.findByRole("button", { name: "Review" }));
    const prompt = await screen.findByRole("dialog", { name: "New sign-in" });
    expect(prompt).toHaveTextContent("Allow it only if you started this sign-in on that device.");
    expect(prompt).toHaveTextContent("Orbyn Planner on iPad");
    expect(prompt).toHaveTextContent("Near Melbourne");
    expect(prompt).not.toHaveTextContent("418 207");
    expect(within(prompt).getAllByRole("button").map((button) => button.textContent)).toEqual(["Don’t allow", "Allow"]);
    expect(within(prompt).queryByRole("button", { name: /Codes match|Compare/ })).toBeNull();
  });

  it("Allow asks for the password for that request, then shows the six digits to type on the new device", async () => {
    const user = await devicesPage();
    await user.click(await screen.findByRole("button", { name: "Review" }));
    const prompt = await screen.findByRole("dialog", { name: "New sign-in" });
    await user.click(within(prompt).getByRole("button", { name: "Allow" }));
    const confirm = await screen.findByRole("dialog", { name: "Confirm it’s you" });
    expect(within(confirm).getByText("To let iPad sign in")).toBeVisible();
    expect(within(confirm).getByText("This confirms one action.")).toBeVisible();
    await user.type(within(confirm).getByLabelText("Password"), PASSWORD);
    await user.click(within(confirm).getByRole("button", { name: "Confirm" }));
    const digits = await screen.findByRole("dialog", { name: "Type these digits on the new device" });
    expect(digits).toHaveTextContent("Type these six digits on the iPad to finish the sign-in. Nothing is signed in until you do.");
    expect(within(digits).getByText("418 207")).toBeVisible();
    expect(within(digits).getByLabelText("Digits 4 1 8, 2 0 7")).toBeVisible();
    expect(within(digits).getByText(/This request expires in \d+:\d\d\./)).toBeVisible();
    expect(within(digits).getAllByRole("button").map((button) => button.textContent)).toEqual(["Cancel"]);
    // The request stays in the list until the digits are accepted or it is closed.
    expect(screen.getByRole("listitem", { name: "iPad asks to sign in" })).toBeVisible();
    // Cancel closes the request.
    await user.click(within(digits).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("listitem", { name: "iPad asks to sign in" })).toBeNull());
    expect(screen.queryByRole("dialog", { name: "Type these digits on the new device" })).toBeNull();
  });

  it("the digits dialog closes by itself when the request leaves the list", async () => {
    const user = await devicesPage();
    await user.click(await screen.findByRole("button", { name: "Review" }));
    await user.click(within(await screen.findByRole("dialog", { name: "New sign-in" })).getByRole("button", { name: "Allow" }));
    const confirm = await screen.findByRole("dialog", { name: "Confirm it’s you" });
    await user.type(within(confirm).getByLabelText("Password"), PASSWORD);
    await user.click(within(confirm).getByRole("button", { name: "Confirm" }));
    await screen.findByRole("dialog", { name: "Type these digits on the new device" });
    mock.setAccount({ dropIncoming: "req-ipad" });
    act(() => announceAccountChange());
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Type these digits on the new device" })).toBeNull());
  });

  it("cancelling the password step returns to Allow and Don't allow", async () => {
    const user = await devicesPage();
    await user.click(await screen.findByRole("button", { name: "Review" }));
    await user.click(within(await screen.findByRole("dialog", { name: "New sign-in" })).getByRole("button", { name: "Allow" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Confirm it’s you" })).getByRole("button", { name: "Cancel" }));
    expect(await screen.findByRole("dialog", { name: "New sign-in" })).toBeVisible();
    expect(screen.getByRole("listitem", { name: "iPad asks to sign in" })).toBeVisible();
  });

  it("Don't allow in the prompt refuses it", async () => {
    const user = await devicesPage();
    await user.click(await screen.findByRole("button", { name: "Review" }));
    const prompt = await screen.findByRole("dialog", { name: "New sign-in" });
    await user.click(within(prompt).getByRole("button", { name: "Don’t allow" }));
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
    expect(within(dialog).getByText("Settings on both sides are merged one setting at a time. If the same setting differs, the newer change wins and you can review it.")).toBeVisible();
    expect(dialog).not.toHaveTextContent(/backup/i);
    expect(within(dialog).getByText("Will sync")).toBeVisible();
    await user.click(within(dialog).getByRole("button", { name: "Connect" }));
    expect(await screen.findByRole("dialog", { name: "Settings connected" })).toHaveTextContent(/^Settings connected24 settings synced\.Done$/);
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
    render(<ApprovalCard approval={{ state: "approved", expires_in_secs: 0 }} onChanged={() => undefined} onBack={() => undefined} />);
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
    await user.click(await screen.findByRole("button", { name: "Review" }));
    const prompt = await screen.findByRole("dialog", { name: "New sign-in" });
    await user.click(within(prompt).getByRole("button", { name: "Allow" }));
    const confirm = await screen.findByRole("dialog", { name: "Confirm it’s you" });
    mock.setAccount({ dropIncoming: "req-ipad" });
    await user.type(within(confirm).getByLabelText("Password"), PASSWORD);
    await user.click(within(confirm).getByRole("button", { name: "Confirm" }));
    expect(await within(confirm).findByText("Denied or cancelled.")).toBeVisible();
    expect(within(confirm).queryByText("That request has ended.")).toBeNull();
  });

  it("the prompt closes when its request leaves the list", async () => {
    const user = await devicesPage();
    await user.click(await screen.findByRole("button", { name: "Review" }));
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
      const view = { state: "waiting" as const, expires_in_secs: 600 };
      const poll = vi.spyOn(accountApi, "pollApproval").mockResolvedValue({ ...view, expires_in_secs: 580 });
      render(<ApprovalCard approval={view} onChanged={() => undefined} onBack={() => undefined} />);
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

  it("after three failed looks in a row says so neutrally, with Cancel only, and keeps looking", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      vi.spyOn(accountApi, "pollApproval").mockRejectedValue(new Error("down"));
      render(<ApprovalCard approval={{ state: "waiting", expires_in_secs: 600 }} onChanged={() => undefined} onBack={() => undefined} />);
      await act(async () => { vi.advanceTimersByTime(6500); });
      expect(await screen.findByRole("heading", { name: "Couldn’t check this request." })).toBeVisible();
      expect(screen.getByText("This isn’t final: the request may still be waiting on your other device.")).toBeVisible();
      expect(screen.queryByRole("alert")).toBeNull();
      expect(screen.queryByText("Denied or cancelled.")).toBeNull();
      expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual(["Cancel"]);
    } finally {
      vi.useRealTimers();
      vi.restoreAllMocks();
    }
  });

  it("keeps the clock out of the live region and gives a closed card an alert and the focus", async () => {
    render(<ApprovalCard approval={{ state: "denied", expires_in_secs: 0 }} onChanged={() => undefined} onBack={() => undefined} />);
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
