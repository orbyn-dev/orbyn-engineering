// The one door this feature uses to reach the service. The password goes to the service once,
// on loopback, and nowhere else; it is never kept here.
import { request } from "../../api/client";
import type { AccountSignInResult } from "../../generated/AccountSignInResult";
import type { AccountView } from "../../generated/AccountView";
import { announceAccountChange } from "./changes";

const changed = <T>(call: Promise<T>) => call.finally(announceAccountChange);

const post = (path: string, body?: unknown) => changed(request<void>("POST", `/api/account${path}`, body ?? {}));

export const accountApi = {
  view: (signal?: AbortSignal) => request<AccountView>("GET", "/api/account", undefined, signal),
  checkInvite: (invite: string) => post("/sign-up/invite", { invite }),
  startSignUp: (invite: string, email: string) => post("/sign-up/start", { invite, email }),
  resendCode: () => post("/sign-up/resend"),
  verifyCode: (code: string) => post("/sign-up/code", { code }),
  register: (password: string) => post("/sign-up/register", { password }),
  signIn: (email: string, password: string) =>
    changed(request<AccountSignInResult>("POST", "/api/account/sign-in", { email, password })),
  cancelSignIn: () => post("/sign-in/cancel"),
  secondFactor: (code: string) => post("/sign-in/second-factor", { code }),
  confirm: (password: string) => post("/confirm", { password }),
  syncNow: () => post("/sync/now"),
  pauseSync: () => post("/sync/pause"),
  resumeSync: () => post("/sync/resume"),
};
