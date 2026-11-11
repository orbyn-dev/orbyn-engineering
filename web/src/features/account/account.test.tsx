import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import { installMockFetch } from "../../test-support";
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
    expect(await screen.findByText(/Codes change every 30 seconds, so try the current one\. 4 tries left\./)).toBeVisible();
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

describe("create an account", () => {
  async function toEmailCode(user: ReturnType<typeof userEvent.setup>) {
    await user.click(screen.getByRole("button", { name: "Create account" }));
    await user.type(screen.getByLabelText("Invite code"), "k7qm 2xwh-9qxa3c2d");
    expect(screen.getByLabelText("Invite code")).toHaveValue("K7QM-2XWH-9QXA-3C2D");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Invite accepted")).toBeVisible();
    expect(screen.queryByLabelText(/name/i)).toBeNull();
    await user.type(screen.getByLabelText("Email"), "new@example.com");
    await user.type(screen.getByLabelText("Password"), "a fresh throwaway phrase");
    await user.click(screen.getByRole("button", { name: "Continue with a password" }));
    expect(await screen.findByText(/Enter the 8-character code we emailed to new@example.com/)).toBeVisible();
  }

  it("runs the four steps and ends signed in", async () => {
    const user = await signedOut();
    expect(screen.queryByText(/Step/)).toBeNull();
    await toEmailCode(user);
    expect(screen.getByText("Step 3 of 4 · Check your email")).toBeVisible();
    await user.type(screen.getByLabelText("8-character code"), "abcdefgh");
    expect(screen.getByLabelText("8-character code")).toHaveValue("ABCD-EFGH");
    await user.click(screen.getByRole("button", { name: "Verify" }));
    expect(await screen.findByText("Your account is ready")).toBeVisible();
    expect(screen.getByText("You’re signed in as new@example.com on this computer.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("Signed in on this device · This computer")).toBeVisible();
  });

  it("refuses a wrong invite without saying anything about an email", async () => {
    const user = await signedOut();
    await user.click(screen.getByRole("button", { name: "Create account" }));
    expect(screen.getByText("Step 1 of 4 · Invite code")).toBeVisible();
    await user.type(screen.getByLabelText("Invite code"), "ABCDEFGHJKMNPQRS");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByText("That invite code isn’t right. Check it and try again.")).toBeVisible();
    expect(screen.queryByLabelText("Email")).toBeNull();
  });

  it("keeps a wrong emailed code so a typo can be fixed", async () => {
    const user = await signedOut();
    await toEmailCode(user);
    await user.type(screen.getByLabelText("8-character code"), "ZZZZZZZZ");
    await user.click(screen.getByRole("button", { name: "Verify" }));
    expect(await screen.findByText("That code isn’t right. Use the code in the newest email.")).toBeVisible();
    expect(screen.getByLabelText("8-character code")).toHaveValue("ZZZZ-ZZZZ");
    await user.click(screen.getByRole("button", { name: "Change email" }));
    expect(screen.getByLabelText("Email")).toHaveValue("new@example.com");
  });

  it("never sends the password until the code is verified, and only to register", async () => {
    const user = await signedOut();
    const fetched = vi.mocked(fetch);
    await toEmailCode(user);
    const sent = () => fetched.mock.calls.filter(([, init]) => String(init?.body ?? "").includes("fresh throwaway"));
    expect(sent()).toHaveLength(0);
    await user.type(screen.getByLabelText("8-character code"), "ABCDEFGH");
    await user.click(screen.getByRole("button", { name: "Verify" }));
    await screen.findByText("Your account is ready");
    const calls = sent().map(([url]) => String(url));
    expect(calls).toEqual(["/api/account/sign-up/register"]);
  });
});

describe("Confirm it’s you", () => {
  async function opened(onConfirmed = vi.fn()) {
    mock.reset("account");
    await fetch("/api/account/sign-in", { method: "POST", body: JSON.stringify({ email: "dev@example.com", password: PASSWORD }) });
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(<ConfirmItsYou onConfirmed={onConfirmed} onClose={onClose} />);
    return { user, onClose, onConfirmed };
  }

  it("asks for the password, runs the action once it is confirmed and closes", async () => {
    const { user, onClose, onConfirmed } = await opened();
    expect(screen.getByRole("dialog", { name: "Confirm it’s you" })).toBeVisible();
    expect(screen.getByText("Enter your password to continue.")).toBeVisible();
    expect(screen.getByText("You won’t be asked again if you signed in within the last 10 minutes.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Confirm" })).toBeDisabled();
    await user.type(screen.getByLabelText("Password"), PASSWORD);
    await user.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(onConfirmed).toHaveBeenCalledTimes(1));
    expect(onClose).toHaveBeenCalled();
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
    expect(await screen.findByText(/Nothing was changed\./)).toBeVisible();
    expect(screen.getByLabelText("Password")).toHaveValue(PASSWORD);
    expect(onConfirmed).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(onConfirmed).toHaveBeenCalledTimes(1));
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
  async function toDetails(user: ReturnType<typeof userEvent.setup>, email: string) {
    await user.click(screen.getByRole("button", { name: "Create account" }));
    await user.type(screen.getByLabelText("Invite code"), "K7QM2XWH9QXA3C2D");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await user.type(await screen.findByLabelText("Email"), email);
    await user.type(screen.getByLabelText("Password"), "a fresh throwaway phrase");
  }

  it("shows a visible wait with Cancel after 400 ms, and Cancel returns to the form with nothing sent", async () => {
    const user = await signedOut();
    await toDetails(user, "busy@example.com");
    const api = (await import("./api")).accountApi;
    let signal: AbortSignal | undefined;
    const spy = vi.spyOn(api, "startSignUp").mockImplementation((_invite, _email, given) => {
      signal = given;
      return new Promise((_resolve, reject) => given?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError"))));
    });
    await user.click(screen.getByRole("button", { name: "Continue with a password" }));
    expect(await screen.findByText("Checking this device")).toBeVisible();
    expect(screen.getByRole("progressbar", { name: "Checking this device" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(signal?.aborted).toBe(true);
    expect(await screen.findByLabelText("Email")).toHaveValue("busy@example.com");
    expect(screen.getByLabelText("Password")).toHaveValue("a fresh throwaway phrase");
    spy.mockRestore();
  });

  it("says plainly when the check could not finish and keeps the form filled in", async () => {
    const user = await signedOut();
    await toDetails(user, "hard@example.com");
    await user.click(screen.getByRole("button", { name: "Continue with a password" }));
    expect(await screen.findByText("We couldn’t finish checking this device")).toBeVisible();
    expect(screen.getByText("The check could not complete. Nothing was sent, and what you typed is kept.")).toBeVisible();
    expect(screen.getByLabelText("Email")).toHaveValue("hard@example.com");
    expect(screen.getByRole("button", { name: "Try again" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("button", { name: "Continue with a password" })).toBeVisible();
  });
});
