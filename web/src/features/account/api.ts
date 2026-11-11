// The one door this feature uses to reach the service. The password goes to the service once,
// on loopback, and nowhere else; it is never kept here.
import { request } from "../../api/client";
import type { AccountConfirmAction } from "../../generated/AccountConfirmAction";
import type { AccountDevice } from "../../generated/AccountDevice";
import type { AccountSignInResult } from "../../generated/AccountSignInResult";
import type { AccountView } from "../../generated/AccountView";
import { announceAccountChange } from "./changes";

const changed = <T>(call: Promise<T>) => call.finally(announceAccountChange);

const post = (path: string, body?: unknown, signal?: AbortSignal) =>
  changed(request<void>("POST", `/api/account${path}`, body ?? {}, signal));

export const accountApi = {
  view: (signal?: AbortSignal) => request<AccountView>("GET", "/api/account", undefined, signal),
  checkInvite: (invite: string) => post("/sign-up/invite", { invite }),
  /** Can take a few seconds when Orbyn asks this device for a short check first; `signal` cancels it. */
  startSignUp: (invite: string, email: string, signal?: AbortSignal) => post("/sign-up/start", { invite, email }, signal),
  resendCode: () => post("/sign-up/resend"),
  verifyCode: (code: string) => post("/sign-up/code", { code }),
  register: (password: string) => post("/sign-up/register", { password }),
  signIn: (email: string, password: string) =>
    changed(request<AccountSignInResult>("POST", "/api/account/sign-in", { email, password })),
  cancelSignIn: () => post("/sign-in/cancel"),
  secondFactor: (code: string) => post("/sign-in/second-factor", { code }),
  /** One confirmation is for one action, and one device when it removes a device. */
  confirm: (password: string, action: AccountConfirmAction, deviceId?: string) =>
    post("/confirm", { password, action, device_id: deviceId ?? null }),
  devices: (signal?: AbortSignal) => request<AccountDevice[]>("GET", "/api/account/devices", undefined, signal),
  /** Needs a confirmation for exactly this device; the service refuses without one. */
  removeDevice: (id: string) => changed(request<void>("DELETE", `/api/account/devices/${encodeURIComponent(id)}`)),
  signOut: (discardUnsynced: boolean) => post("/sign-out", { discard_unsynced: discardUnsynced }),
  /** Needs a confirmation for this action; the service refuses without one. */
  signOutEverywhere: () => post("/sign-out-everywhere"),
  syncNow: () => post("/sync/now"),
  pauseSync: () => post("/sync/pause"),
  resumeSync: () => post("/sync/resume"),
};
