import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import { ApiFailure } from "../../api/client";
import { installMockFetch } from "../../test-support";
import { act } from "@testing-library/react";
import { accountApi } from "./api";
import { announceAccountChange } from "./changes";
import { AccountSection } from "./AccountSection";
import { ConfirmItsYou } from "./ConfirmItsYou";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

const PASSWORD = "correct horse battery";

async function signedOut() {
  mock.reset("account");
  const user = userEvent.setup();
  render(<AccountSection />);
  await screen.findByText("Use an account to sync your settings");
  return user;
}

async function openSignIn() {
  const user = await signedOut();
  await user.click(screen.getByRole("button", { name: "Sign in" }));
  return user;
}

async function signIn(user: ReturnType<typeof userEvent.setup>, email: string, password = PASSWORD) {
  await user.type(screen.getByLabelText("Email"), email);
  await user.type(screen.getByLabelText("Password"), password);
  await user.click(screen.getByRole("button", { name: "Sign in" }));
}

describe("the gate", () => {
  it("shows nothing at all while the gate is off", async () => {
    const { container } = render(<AccountSection />);
    await waitFor(() => expect(mock.handle("GET", "/api/account").body).toMatchObject({ available: false }));
    expect(container.textContent).toBe("");
  });
});

describe("signed out", () => {
  it("says what an account adds and never moves, without Execution profiles", async () => {
    await signedOut();
    expect(screen.getByText("Engineering works without an account.")).toBeVisible();
    const syncs = screen.getByText("What syncs").parentElement as HTMLElement;
    expect(within(syncs).getAllByRole("listitem").map((item) => item.textContent)).toEqual([
      "Appearance and theme",
      "Keybindings",
      "Chat and diff preferences",
    ]);
    expect(screen.queryByText(/Execution profiles/)).toBeNull();
    expect(screen.getByText("What never syncs")).toBeVisible();
    expect(screen.getByRole("button", { name: "Create account" })).toBeVisible();
    expect(screen.queryByText(/passkey/i)).toBeNull();
    expect(screen.queryByText(/Forgot your password/)).toBeNull();
  });
});

describe("signed out, with sign-up open to everyone", () => {
  it("does not say an invite is needed when it is not", async () => {
    mock.reset("account");
    mock.setAccount({ inviteRequired: false });
    render(<AccountSection />);
    await screen.findByText("Use an account to sync your settings");
    expect(screen.queryByText(/invite/i)).toBeNull();
  });

  it("says so while sign-up is invite-only", async () => {
    await signedOut();
    expect(screen.getByText("Creating an account needs an invite code while sign-up is invite-only.")).toBeVisible();
  });
});

describe("sign in", () => {
  it("signs in and shows the signed-in page with the device name", async () => {
    const user = await openSignIn();
    expect(screen.getByText("Orbyn is invite-only for now.")).toBeVisible();
    await signIn(user, "dev@example.com");
    expect(await screen.findByText("Signed in on this device · This computer")).toBeVisible();
    expect(screen.getByText("dev@example.com")).toBeVisible();
    expect(screen.getByText("Up to date", { exact: false })).toBeVisible();
    // Phase 1 syncs no execution profiles.
    expect(screen.getByText("This device only")).toBeVisible();
    expect(screen.queryByText(/recovery code/i)).toBeNull();
  });

  it("says one thing for a wrong password, keeps the email and clears the password", async () => {
    const user = await openSignIn();
    await signIn(user, "dev@example.com", "nope");
    expect(await screen.findByText("That email and password don’t match. Check them and try again.")).toBeVisible();
    expect(screen.getByLabelText("Email")).toHaveValue("dev@example.com");
    expect(screen.getByLabelText("Password")).toHaveValue("");
  });

  it("keeps what was typed and offers Try again when Orbyn cannot be reached", async () => {
    const user = await openSignIn();
    await signIn(user, "down@example.com");
    expect(await screen.findByText(/Couldn’t reach Orbyn\. Check your connection and try again\. Nothing was changed\./)).toBeVisible();
    expect(screen.getByLabelText("Password")).toHaveValue(PASSWORD);
    expect(screen.getByRole("button", { name: "Try again" })).toBeEnabled();
  });

  it("turns Sign in off while a retry time runs, clears the password and keeps the email", async () => {
    const user = await openSignIn();
    await signIn(user, "slow@example.com");
    expect(await screen.findByText("Too many attempts. Try again in 2 minutes.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Sign in" })).toBeDisabled();
    expect(screen.getByLabelText("Email")).toHaveValue("slow@example.com");
    expect(screen.getByLabelText("Password")).toHaveValue("");
  });

  it("asks for the two-factor code, counts wrong ones and closes after five", async () => {
    const user = await openSignIn();
    await signIn(user, "two@example.com");
    expect(await screen.findByText("Enter your two-factor code")).toBeVisible();
    const field = screen.getByLabelText("6-digit code");
    expect(screen.getByRole("button", { name: "Verify" })).toBeDisabled();
    await user.type(field, "48a2 91x4");
    expect(field).toHaveValue("482 914");
    await user.click(screen.getByRole("button", { name: "Verify" }));
    expect(await screen.findByText(/Codes change every 30 seconds\. 4 tries left\./)).toBeVisible();
    expect(field).toHaveValue("482 914");
    for (const left of [3, 2, 1]) {
      await user.click(screen.getByRole("button", { name: "Verify" }));
      expect(await screen.findByText(new RegExp(`${left} ${left === 1 ? "try" : "tries"} left\\.`))).toBeVisible();
    }
    await user.click(screen.getByRole("button", { name: "Verify" }));
    expect(await screen.findByText("Sign-in closed")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Start again" }));
    expect(await screen.findByRole("button", { name: "Sign in" })).toBeVisible();
  });

  it("Back to Account on the closed card returns to the Account page, not to the sign-in form", async () => {
    const user = await openSignIn();
    await signIn(user, "two@example.com");
    await screen.findByText("Enter your two-factor code");
    await user.type(screen.getByLabelText("6-digit code"), "482914");
    for (let i = 0; i < 5; i += 1) {
      await user.click(screen.getByRole("button", { name: "Verify" }));
      await screen.findByText(i < 4 ? new RegExp(`${4 - i} ${4 - i === 1 ? "try" : "tries"} left`) : "Sign-in closed");
    }
    await user.click(screen.getByRole("button", { name: "Back to Account" }));
    expect(await screen.findByText("Use an account to sync your settings")).toBeVisible();
    expect(screen.queryByLabelText("Password")).toBeNull();
  });

  it("takes a recovery code in any case, with or without the hyphen", async () => {
    const user = await openSignIn();
    await signIn(user, "two@example.com");
    await user.click(await screen.findByRole("button", { name: "Use a recovery code" }));
    const field = screen.getByLabelText("Recovery code");
    await user.type(field, "abcde fgh23");
    expect(field).toHaveValue("ABCDE-FGH23");
    await user.click(screen.getByRole("button", { name: "Verify" }));
    expect(await screen.findByText("Signed in on this device · This computer")).toBeVisible();
  });

  it("a wrong recovery code does not say whether it was used", async () => {
    const user = await openSignIn();
    await signIn(user, "two@example.com");
    await user.click(await screen.findByRole("button", { name: "Use a recovery code" }));
    await user.type(screen.getByLabelText("Recovery code"), "ZZZZZ-ZZZZZ");
    await user.click(screen.getByRole("button", { name: "Verify" }));
    expect(await screen.findByText("That recovery code didn’t work. Check it, or use a code you haven’t used.")).toBeVisible();
  });
});

const NEW_PASSWORD = "a fresh throwaway phrase";

/** Types into the password step: the password, then the same again to confirm it. */
async function typePasswords(user: ReturnType<typeof userEvent.setup>, password = NEW_PASSWORD, confirm = password) {
  await user.type(screen.getByLabelText("Password"), password);
  await user.type(screen.getByLabelText("Confirm password"), confirm);
}

describe("create an account", () => {
  async function toEmail(user: ReturnType<typeof userEvent.setup>) {
    await user.click(screen.getByRole("button", { name: "Create account" }));
    await user.type(screen.getByLabelText("Invite code"), "k7qm 2xwh-9qxa3c2d");
    expect(screen.getByLabelText("Invite code")).toHaveValue("K7QM-2XWH-9QXA-3C2D");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Invite accepted")).toBeVisible();
    expect(screen.queryByLabelText(/name/i)).toBeNull();
    await user.type(screen.getByLabelText("Email"), "new@example.com");
  }

  async function toPassword(user: ReturnType<typeof userEvent.setup>) {
    await toEmail(user);
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Step 3 of 5 · Password")).toBeVisible();
  }

  async function toEmailCode(user: ReturnType<typeof userEvent.setup>) {
    await toPassword(user);
    await typePasswords(user);
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText(/Enter the 8-character code we emailed to new@example.com/)).toBeVisible();
  }

  it("runs the five steps in order and ends signed in", async () => {
    const user = await signedOut();
    expect(screen.queryByText(/Step/)).toBeNull();
    await user.click(screen.getByRole("button", { name: "Create account" }));
    expect(screen.getByText("Step 1 of 5 · Invite code")).toBeVisible();
    await user.type(screen.getByLabelText("Invite code"), "K7QM2XWH9QXA3C2D");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Step 2 of 5 · Email")).toBeVisible();
    expect(screen.getByText("Next you choose a password. Then we email a code to confirm this address.")).toBeVisible();
    await user.type(screen.getByLabelText("Email"), "new@example.com");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Step 3 of 5 · Password")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Choose a password" })).toBeVisible();
    expect(screen.getByText(/For new@example.com/)).toBeVisible();
    expect(screen.getByText("Use at least 12 characters.")).toBeVisible();
    await typePasswords(user);
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Step 4 of 5 · Check your email")).toBeVisible();
    await user.type(screen.getByLabelText("8-character code"), "abcdefgh");
    expect(screen.getByLabelText("8-character code")).toHaveValue("ABCD-EFGH");
    await user.click(screen.getByRole("button", { name: "Verify" }));
    expect(await screen.findByText("Step 5 of 5 · Done")).toBeVisible();
    expect(screen.getByText("You’re signed in as new@example.com on this computer.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Signed in on this device · This computer")).toBeVisible();
  });

  it("sends nothing from the email step: the code is emailed only after the password is submitted", async () => {
    const user = await signedOut();
    const fetched = vi.mocked(fetch);
    await toEmail(user);
    const startCalls = () => fetched.mock.calls.filter(([url]) => String(url).includes("/sign-up/start"));
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByText("Step 3 of 5 · Password");
    expect(startCalls()).toHaveLength(0);
  });

  it("changes the email from the password step, with the invite still accepted", async () => {
    const user = await signedOut();
    await toPassword(user);
    await user.click(screen.getByRole("button", { name: "Change email" }));
    expect(screen.getByLabelText("Email")).toHaveValue("new@example.com");
    expect(screen.getByText("Invite accepted")).toBeVisible();
    expect(screen.getByText("Step 2 of 5 · Email")).toBeVisible();
  });

  it("refuses a wrong invite without saying anything about an email", async () => {
    const user = await signedOut();
    await user.click(screen.getByRole("button", { name: "Create account" }));
    expect(screen.getByText("Step 1 of 5 · Invite code")).toBeVisible();
    await user.type(screen.getByLabelText("Invite code"), "ABCDEFGHJKMNPQRS");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("That invite code isn’t right. Check it and try again.")).toBeVisible();
    expect(screen.queryByLabelText("Email")).toBeNull();
  });

  it("says too short under the password, keeps Continue off, and checks nothing until it is pressed", async () => {
    const user = await signedOut();
    await toPassword(user);
    const fetched = vi.mocked(fetch);
    await typePasswords(user, "short pw", "short pw");
    // Nothing is checked while typing: Continue is on once both fields are filled.
    expect(screen.queryByText(/Too short/)).toBeNull();
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Too short. Use at least 12 characters.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    expect(screen.queryByText("The two passwords don’t match.")).toBeNull();
    expect(fetched.mock.calls.filter(([url]) => String(url).includes("/sign-up/start"))).toHaveLength(0);
    // Long enough again: the message goes and Continue turns on.
    await user.type(screen.getByLabelText("Password"), " with more words");
    expect(screen.queryByText(/Too short/)).toBeNull();
    expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
  });

  it("says the two passwords don’t match, sends nothing and keeps both fields", async () => {
    const user = await signedOut();
    await toPassword(user);
    const fetched = vi.mocked(fetch);
    await typePasswords(user, NEW_PASSWORD, "another long throwaway");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("The two passwords don’t match.")).toBeVisible();
    expect(screen.getByLabelText("Password")).toHaveValue(NEW_PASSWORD);
    expect(screen.getByLabelText("Confirm password")).toHaveValue("another long throwaway");
    expect(fetched.mock.calls.filter(([url]) => String(url).includes("/sign-up/start"))).toHaveLength(0);
  });

  it("says Orbyn could not be reached when the password is submitted, sent nothing, and Try again repeats it", async () => {
    const user = await signedOut();
    await toPassword(user);
    const fail = vi.spyOn(accountApi, "startSignUp").mockRejectedValueOnce(new ApiFailure("account_offline", "Couldn't reach Orbyn.", 503));
    await typePasswords(user);
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Couldn’t reach Orbyn. Check your connection and try again. Nothing was sent.")).toBeVisible();
    expect(screen.getByLabelText("Password")).toHaveValue(NEW_PASSWORD);
    expect(screen.getByLabelText("Confirm password")).toHaveValue(NEW_PASSWORD);
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Step 4 of 5 · Check your email")).toBeVisible();
    expect(fail).toHaveBeenCalledTimes(2);
    fail.mockRestore();
  });

  it("keeps a wrong emailed code so a typo can be fixed", async () => {
    const user = await signedOut();
    await toEmailCode(user);
    await user.type(screen.getByLabelText("8-character code"), "ZZZZZZZZ");
    await user.click(screen.getByRole("button", { name: "Verify" }));
    expect(await screen.findByText("That code didn’t work. Check it, or ask for a new one.")).toBeVisible();
    expect(screen.getByLabelText("8-character code")).toHaveValue("ZZZZ-ZZZZ");
    await user.click(screen.getByRole("button", { name: "Change email" }));
    expect(screen.getByLabelText("Email")).toHaveValue("new@example.com");
  });

  it("says Orbyn could not be reached for the code, nothing changed, and Try again repeats the request", async () => {
    const user = await signedOut();
    await toEmailCode(user);
    const fail = vi.spyOn(accountApi, "verifyCode").mockRejectedValueOnce(new ApiFailure("account_offline", "Couldn't reach Orbyn.", 503));
    await user.type(screen.getByLabelText("8-character code"), "ABCDEFGH");
    await user.click(screen.getByRole("button", { name: "Verify" }));
    expect(await screen.findByText("Couldn’t reach Orbyn. Check your connection and try again. Nothing was changed.")).toBeVisible();
    expect(screen.getByLabelText("8-character code")).toHaveValue("ABCD-EFGH");
    expect(screen.queryByText(/didn’t work/)).toBeNull();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Your account is ready")).toBeVisible();
    fail.mockRestore();
  });

  it("sends the password once, with the email, at the password step and never again", async () => {
    const user = await signedOut();
    const fetched = vi.mocked(fetch);
    await toEmailCode(user);
    const sent = () => fetched.mock.calls.filter(([, init]) => String(init?.body ?? "").includes("fresh throwaway"));
    expect(sent().map(([url]) => String(url))).toEqual(["/api/account/sign-up/start"]);
    await user.type(screen.getByLabelText("8-character code"), "ABCDEFGH");
    await user.click(screen.getByRole("button", { name: "Verify" }));
    await screen.findByText("Your account is ready");
    expect(sent().map(([url]) => String(url))).toEqual(["/api/account/sign-up/start"]);
  });
});

describe("create an account without an invite step", () => {
  async function toCode(user: ReturnType<typeof userEvent.setup>) {
    mock.reset("account");
    mock.setAccount({ inviteRequired: false });
    render(<AccountSection />);
    await screen.findByText("Use an account to sync your settings");
    await user.click(screen.getByRole("button", { name: "Create account" }));
    await user.type(await screen.findByLabelText("Email"), "new@example.com");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await typePasswords(user);
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByText(/Enter the 8-character code we emailed to new@example.com/);
  }

  it("counts four steps, not five, and never more than the total", async () => {
    const user = userEvent.setup();
    mock.reset("account");
    mock.setAccount({ inviteRequired: false });
    render(<AccountSection />);
    await screen.findByText("Use an account to sync your settings");
    await user.click(screen.getByRole("button", { name: "Create account" }));
    expect(await screen.findByText("Step 1 of 4 · Email")).toBeVisible();
    expect(screen.queryByLabelText("Invite code")).toBeNull();
    await user.type(screen.getByLabelText("Email"), "new@example.com");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Step 2 of 4 · Password")).toBeVisible();
    await typePasswords(user);
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Step 3 of 4 · Check your email")).toBeVisible();
    await user.type(screen.getByLabelText("8-character code"), "ABCDEFGH");
    await user.click(screen.getByRole("button", { name: "Verify" }));
    expect(await screen.findByText("Step 4 of 4 · Done")).toBeVisible();
  });

  it("asks for a new code only once a minute, on the app’s own clock", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      await toCode(user);
      const resend = screen.getByRole("button", { name: "Send a new code in 1:00" });
      expect(resend).toBeDisabled();
      act(() => { vi.advanceTimersByTime(30_000); });
      expect(await screen.findByRole("button", { name: "Send a new code in 0:30" })).toBeDisabled();
      act(() => { vi.advanceTimersByTime(31_000); });
      const ready = await screen.findByRole("button", { name: "Send a new code" });
      expect(ready).toBeEnabled();
      await user.click(ready);
      expect(await screen.findByText("We sent a new code to new@example.com.")).toBeVisible();
      expect(screen.getByLabelText("8-character code")).toHaveValue("");
      expect(screen.getByRole("button", { name: "Send a new code in 1:00" })).toBeDisabled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("after 30 minutes a refused code gets a quiet line and Send a new code becomes the main action", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      await toCode(user);
      await user.type(screen.getByLabelText("8-character code"), "ZZZZZZZZ");
      await user.click(screen.getByRole("button", { name: "Verify" }));
      expect(await screen.findByText("That code didn’t work. Check it, or ask for a new one.")).toBeVisible();
      expect(screen.queryByText("Codes last 30 minutes. Ask for a new one.")).toBeNull();
      expect(screen.getByRole("button", { name: /^Verify$/ })).toHaveClass("btn-primary");
      act(() => { vi.advanceTimersByTime(30 * 60_000 + 1000); });
      expect(await screen.findByText("Codes last 30 minutes. Ask for a new one.")).toBeVisible();
      expect(screen.getByRole("button", { name: "Send a new code" })).toHaveClass("btn-primary");
      expect(screen.getByRole("button", { name: "Verify" })).toHaveClass("btn-secondary");
    } finally {
      vi.useRealTimers();
    }
  });

  it("repeats a new-code request after a network failure, not the code", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      await toCode(user);
      vi.spyOn(accountApi, "resendCode").mockRejectedValueOnce(new ApiFailure("account_offline", "Couldn't reach Orbyn.", 503));
      const verify = vi.spyOn(accountApi, "verifyCode");
      act(() => { vi.advanceTimersByTime(61_000); });
      await user.click(await screen.findByRole("button", { name: "Send a new code" }));
      expect(await screen.findByText(/Nothing was changed\./)).toBeVisible();
      await user.click(screen.getByRole("button", { name: "Try again" }));
      expect(await screen.findByText("We sent a new code to new@example.com.")).toBeVisible();
      expect(verify).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
      vi.restoreAllMocks();
    }
  });
});

describe("the done step", () => {
  it("says the settings question comes next only when this computer has settings", async () => {
    const user = userEvent.setup();
    mock.reset("account");
    mock.setAccount({ inviteRequired: false, connectNeeded: true });
    render(<AccountSection />);
    await screen.findByText("Use an account to sync your settings");
    await user.click(screen.getByRole("button", { name: "Create account" }));
    await user.type(await screen.findByLabelText("Email"), "new@example.com");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await typePasswords(user);
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await user.type(await screen.findByLabelText("8-character code"), "ABCDEFGH");
    await user.click(screen.getByRole("button", { name: "Verify" }));
    expect(await screen.findByText("Next, Orbyn asks how to connect this computer’s settings to your account. You can skip it and do it later in Account.")).toBeVisible();
  });
});

describe("the signed-in page when it cannot be fully read", () => {
  async function signedInPage(knobs: () => void = () => undefined) {
    mock.reset("account");
    await fetch("/api/account/sign-in", { method: "POST", body: JSON.stringify({ email: "dev@example.com", password: PASSWORD }) });
    knobs();
    render(<AccountSection />);
    await screen.findByText("Signed in on this device · This computer");
  }

  it("offline: the sync row says Offline and waiting changes, with the text button and no Pause", async () => {
    await signedInPage(() => {
      mock.setAccountWaiting(3);
      mock.setAccount({ sync: ["offline"] });
    });
    expect(screen.getByText(/^Offline · 3 changes waiting/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Waiting for a connection" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Pause" })).toBeNull();
    expect(screen.getByText(/You’re offline\./)).toBeVisible();
  });

  it("syncing: the row says Syncing… with no count, offers Pause and no Sync now", async () => {
    await signedInPage(() => mock.setAccount({ sync: ["syncing"] }));
    expect(screen.getByText("Syncing…")).toBeVisible();
    expect(screen.getByText("Syncing…").textContent).not.toMatch(/\d/);
    expect(screen.queryByRole("button", { name: "Sync now" })).toBeNull();
    expect(screen.getByRole("button", { name: "Pause" })).toBeVisible();
  });

  it("error: keeps the last page dimmed, says it could not check and offers Try again", async () => {
    await signedInPage();
    mock.setAccount({ down: true });
    act(() => announceAccountChange());
    expect(await screen.findByText("Couldn’t load your account.")).toBeVisible();
    expect(screen.getByText(/^Couldn’t check/)).toBeVisible();
    expect(screen.getByText("dev@example.com")).toBeVisible();
    const last = document.querySelector(".ac-last") as HTMLElement;
    expect(last).toHaveAttribute("data-stale");
    // Dimmed and not clickable, but still readable by assistive technology.
    expect(last).not.toHaveAttribute("inert");
    expect(last).not.toHaveAttribute("aria-hidden");
    expect(screen.getByRole("button", { name: "Sync now" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Pause" })).toBeDisabled();
    mock.setAccount({ down: false });
    await userEvent.setup().click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.queryByText("Couldn’t load your account.")).toBeNull());
    expect(screen.getByText(/^Up to date/)).toBeVisible();
  });

  it("loading: keeps the section label while the values are placeholders", async () => {
    mock.reset("account");
    const real = globalThis.fetch;
    vi.stubGlobal("fetch", vi.fn((input: string, init?: RequestInit) => (input === "/api/account" ? new Promise<Response>(() => undefined) : real(input, init))));
    const { container } = render(<AccountSection />);
    await waitFor(() => expect(container.querySelector(".ac-sk")).not.toBeNull());
    expect(within(container.querySelector(".ac-sk") as HTMLElement).getByText("Sync")).toBeInTheDocument();
    expect(container.querySelectorAll(".ac-skr")).toHaveLength(3);
  });
});

describe("Confirm it’s you", () => {
  async function opened(onConfirmed = vi.fn()) {
    mock.reset("account");
    await fetch("/api/account/sign-in", { method: "POST", body: JSON.stringify({ email: "dev@example.com", password: PASSWORD }) });
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(<ConfirmItsYou action="remove_device" deviceId="dev-mini" onConfirmed={onConfirmed} onClose={onClose} />);
    return { user, onClose, onConfirmed };
  }

  it("asks for the password, runs the action once it is confirmed and closes", async () => {
    const { user, onClose, onConfirmed } = await opened();
    expect(screen.getByRole("dialog", { name: "Confirm it’s you" })).toBeVisible();
    expect(screen.getByText("Enter your password to continue.")).toBeVisible();
    // Signing in is not a confirmation, so there is no promise of not being asked again.
    expect(screen.queryByText(/asked again/)).toBeNull();
    expect(screen.getByRole("button", { name: "Confirm" })).toBeDisabled();
    await user.type(screen.getByLabelText("Password"), PASSWORD);
    await user.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(onConfirmed).toHaveBeenCalledTimes(1));
    expect(onClose).toHaveBeenCalled();
    const line = screen.getByText("This confirms removing this device.");
    expect(line).toBeVisible();
    expect(screen.getByLabelText("Password").compareDocumentPosition(line) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // The confirmation names its action and device, and carries the password nowhere else.
    const sent = vi.mocked(fetch).mock.calls.filter(([url]) => String(url) === "/api/account/confirm");
    expect(sent).toHaveLength(1);
    expect(JSON.parse(String(sent[0]?.[1]?.body))).toEqual({ password: PASSWORD, action: "remove_device", device_id: "dev-mini", policy: null });
  });

  it("a wrong password is said under the field, cleared, and the action is not done", async () => {
    const { user, onClose, onConfirmed } = await opened();
    await user.type(screen.getByLabelText("Password"), "nope");
    await user.click(screen.getByRole("button", { name: "Confirm" }));
    expect(await screen.findByText("That isn’t your password. Check it and try again.")).toBeVisible();
    expect(screen.getByLabelText("Password")).toHaveValue("");
    expect(onConfirmed).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("keeps the password typed and offers Try again when Orbyn cannot be reached", async () => {
    const { user, onConfirmed } = await opened();
    mock.fail("/api/account/confirm", 1, 503, "account_offline");
    await user.type(screen.getByLabelText("Password"), PASSWORD);
    await user.click(screen.getByRole("button", { name: "Confirm" }));
    expect(await screen.findByText("Couldn’t reach Orbyn. Check your connection and try again. Nothing was changed.")).toBeVisible();
    expect(screen.getByLabelText("Password")).toHaveValue(PASSWORD);
    expect(onConfirmed).not.toHaveBeenCalled();
    // A network failure is not a wrong password: nothing is said under the field.
    expect(screen.queryByText("That isn’t your password. Check it and try again.")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(onConfirmed).toHaveBeenCalledTimes(1));
  });

  it("Cancel on the network failure closes the dialog and the action is not done", async () => {
    const { user, onClose, onConfirmed } = await opened();
    mock.fail("/api/account/confirm", 1, 503, "account_offline");
    await user.type(screen.getByLabelText("Password"), PASSWORD);
    await user.click(screen.getByRole("button", { name: "Confirm" }));
    await screen.findByText(/Nothing was changed\./);
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
    expect(onConfirmed).not.toHaveBeenCalled();
  });

  it("turns Confirm off with a retry time after too many wrong passwords", async () => {
    const { user } = await opened();
    for (let tries = 0; tries < 5; tries += 1) {
      await user.type(screen.getByLabelText("Password"), "nope");
      await user.click(screen.getByRole("button", { name: "Confirm" }));
      await screen.findByText("That isn’t your password. Check it and try again.");
    }
    await user.type(screen.getByLabelText("Password"), PASSWORD);
    await user.click(screen.getByRole("button", { name: "Confirm" }));
    expect(await screen.findByText("Too many attempts. Try again in 10 minutes.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Confirm" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
  });
});

describe("the device check before a code is sent", () => {
  async function toPassword(user: ReturnType<typeof userEvent.setup>, email: string) {
    await user.click(screen.getByRole("button", { name: "Create account" }));
    await user.type(screen.getByLabelText("Invite code"), "K7QM2XWH9QXA3C2D");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await user.type(await screen.findByLabelText("Email"), email);
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await typePasswords(user);
  }

  it("sits at step 3: a visible wait with Cancel after 400 ms, and Cancel returns to the form with nothing sent", async () => {
    const user = await signedOut();
    await toPassword(user, "busy@example.com");
    const api = (await import("./api")).accountApi;
    let signal: AbortSignal | undefined;
    const spy = vi.spyOn(api, "startSignUp").mockImplementation((_invite, _email, _password, given) => {
      signal = given;
      return new Promise((_resolve, reject) => given?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError"))));
    });
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Checking this device")).toBeVisible();
    expect(screen.getByText("Step 3 of 5 · Password")).toBeVisible();
    expect(screen.getByText("Orbyn does a short check on this device before it sends your code.")).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent("Checking this device…");
    expect(screen.getByText("This takes a moment.")).toBeVisible();
    expect(screen.queryByRole("progressbar")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(signal?.aborted).toBe(true);
    expect(await screen.findByLabelText("Password")).toHaveValue(NEW_PASSWORD);
    expect(screen.getByLabelText("Confirm password")).toHaveValue(NEW_PASSWORD);
    spy.mockRestore();
  });

  it("says plainly when the check could not finish and keeps the form filled in", async () => {
    const user = await signedOut();
    await toPassword(user, "hard@example.com");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("We couldn’t finish checking this device")).toBeVisible();
    expect(screen.getByText("The check could not complete. Nothing was sent, and what you typed is kept.")).toBeVisible();
    expect(screen.getByLabelText("Password")).toHaveValue(NEW_PASSWORD);
    expect(screen.getByLabelText("Confirm password")).toHaveValue(NEW_PASSWORD);
    expect(screen.getByRole("button", { name: "Try again" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("button", { name: "Continue" })).toBeVisible();
  });

  it("turns Try again off while the retry limit runs and keeps what was typed", async () => {
    const user = await signedOut();
    await toPassword(user, "new@example.com");
    vi.spyOn(accountApi, "startSignUp").mockRejectedValueOnce(new ApiFailure("rate_limited", "Too many attempts.", 429, { retry_after_secs: 42 * 60 }));
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Too many attempts. Try again in 42 minutes.")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Too many attempts" })).toBeVisible();
    expect(screen.getByText("What you typed stays in the fields. The button turns on when the time is up.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Try again" })).toBeDisabled();
    expect(screen.getByLabelText("Password")).toHaveValue(NEW_PASSWORD);
    vi.restoreAllMocks();
  });
});
