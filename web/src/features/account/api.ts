// The one door this feature uses to reach the service. The password goes to the service once,
// on loopback, and nowhere else; it is never kept here.
import { request } from "../../api/client";
import type { AccountConfirmAction } from "../../generated/AccountConfirmAction";
import type { AccountApproved } from "../../generated/AccountApproved";
import type { AccountApprovalRequest } from "../../generated/AccountApprovalRequest";
import type { AccountApprovalView } from "../../generated/AccountApprovalView";
import type { AccountConfirmed } from "../../generated/AccountConfirmed";
import type { AccountConnectPolicy } from "../../generated/AccountConnectPolicy";
import type { AccountDevice } from "../../generated/AccountDevice";
import type { AccountSignInResult } from "../../generated/AccountSignInResult";
import type { AccountView } from "../../generated/AccountView";
import { announceAccountChange } from "./changes";

const changed = <T>(call: Promise<T>) => call.finally(announceAccountChange);

const post = (path: string, body?: unknown, signal?: AbortSignal) =>
  changed(request<void>("POST", `/api/account${path}`, body ?? {}, signal));

/** The header a confirmed action sends back: what "Confirm it's you" answered with. */
const confirmed = (nonce: string): { headers: Record<string, string> } => ({ headers: { "x-orbyn-confirm": nonce } });

export const accountApi = {
  view: (signal?: AbortSignal) => request<AccountView>("GET", "/api/account", undefined, signal),
  checkInvite: (invite: string) => post("/sign-up/invite", { invite }),
  /**
   * The password step: it creates the account and emails the code, and sends the invite and the email with it because
   * the earlier steps send nothing. It can take a few seconds when Orbyn asks this device for a short check first;
   * `signal` cancels it.
   */
  startSignUp: (invite: string, email: string, password: string, signal?: AbortSignal) => post("/sign-up/start", { invite, email, password }, signal),
  resendCode: () => post("/sign-up/resend"),
  /** The emailed code is the last step: it signs the new account in. */
  verifyCode: (code: string) => post("/sign-up/code", { code }),
  signIn: (email: string, password: string) =>
    changed(request<AccountSignInResult>("POST", "/api/account/sign-in", { email, password })),
  cancelSignIn: () => post("/sign-in/cancel"),
  secondFactor: (code: string) => post("/sign-in/second-factor", { code }),
  /** One confirmation is for one action, and one device when it removes a device. */
  confirm: (password: string, action: AccountConfirmAction, deviceId?: string, policy?: AccountConnectPolicy) =>
    request<AccountConfirmed>("POST", "/api/account/confirm", { password, action, device_id: deviceId ?? null, policy: policy ?? null }),
  /** The dialog was cancelled: the confirmation it made does not stay live. Only its own nonce can end it. */
  cancelConfirmation: (nonce: string) => request<void>("POST", "/api/account/confirm/cancel", { nonce }),
  /** Asks how this computer's own request stands; an allowed one signs in. Null when there is none. */
  pollApproval: (signal?: AbortSignal) => request<AccountApprovalView | null>("POST", "/api/account/sign-in/approval", {}, signal),
  approvals: (signal?: AbortSignal) => request<AccountApprovalRequest[]>("GET", "/api/account/approvals", undefined, signal),
  /** The six digits typed on this computer for its allowed request; only the right ones sign in. Never kept here. */
  submitDigits: (digits: string) => changed(request<AccountApprovalView | null>("POST", "/api/account/sign-in/digits", { digits })),
  /** Needs a confirmation for exactly this request. Answers with the six digits to show for the new device. */
  approve: (id: string, nonce: string) => changed(request<AccountApproved>("POST", `/api/account/approvals/${encodeURIComponent(id)}/approve`, {}, undefined, confirmed(nonce))),
  /** Refusing, or cancelling an allowed request, only closes it, so it needs no password. */
  deny: (id: string) => changed(request<void>("POST", `/api/account/approvals/${encodeURIComponent(id)}/deny`, {})),
  /** Combining needs no password; the two choices that overwrite do (`connectUseDevice`, `connectUseAccount`). */
  connect: () => post("/connect"),
  connectUseDevice: (nonce: string) => changed(request<void>("POST", "/api/account/connect/use-device", {}, undefined, confirmed(nonce))),
  connectUseAccount: (nonce: string) => changed(request<void>("POST", "/api/account/connect/use-account", {}, undefined, confirmed(nonce))),
  connectLater: () => post("/connect/later"),
  connectSeen: () => post("/connect/seen"),
  /** The person was told their session ended, or dismissed it. */
  endedSeen: () => post("/ended/seen"),
  devices: (signal?: AbortSignal) => request<AccountDevice[]>("GET", "/api/account/devices", undefined, signal),
  /** Needs a confirmation for exactly this device; the service refuses without one. */
  removeDevice: (id: string, nonce: string) => changed(request<void>("DELETE", `/api/account/devices/${encodeURIComponent(id)}`, undefined, undefined, confirmed(nonce))),
  signOut: (discardUnsynced: boolean) => post("/sign-out", { discard_unsynced: discardUnsynced }),
  /** Needs a confirmation for this action; the service refuses without one. */
  signOutEverywhere: (nonce: string) => changed(request<void>("POST", "/api/account/sign-out-everywhere", {}, undefined, confirmed(nonce))),
  syncNow: () => post("/sync/now"),
  pauseSync: () => post("/sync/pause"),
  resumeSync: () => post("/sync/resume"),
};
