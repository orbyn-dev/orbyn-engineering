/**
 * The mock's account: the same routes, answers and throwaway accounts as the service's development
 * stand-in (`orbyn_core::account::dev`), so the screens and their end-to-end specs run without a service.
 * The default scenario has the gate off (`available: false`); the `account` scenario turns it on.
 *
 * Known values: invite K7QM-2XWH-9QXA-3C2D, emailed code ABCD-EFGH, password "correct horse battery"
 * for dev@, two@ (asks for 123456 or the recovery code ABCDE-FGH23), slow@ (rate limited), down@ (offline).
 * Sign-up with hard@ fails its device check; busy@ takes a moment in the service (set `/__mock/delay/<ms>` to see it here).
 */
import type { AccountView } from "../src/generated/AccountView.ts";
import type { MockResponse } from "./api.ts";

const INVITE = "K7QM2XWH9QXA3C2D";
const EMAIL_CODE = "ABCDEFGH";
const PASSWORD = "correct horse battery";
const SECOND_FACTOR = ["123456", "ABCDEFGH23"];
const ACCOUNTS = ["dev@example.com", "two@example.com", "slow@example.com", "down@example.com"];

const fail = (status: number, code: string, message: string, detail?: unknown): MockResponse => ({
  status,
  body: { code, message, ...(detail ? { detail } : {}) },
});
const clean = (text: unknown) => String(text ?? "").replace(/[\s-]/g, "").toUpperCase();

export function createAccountMock() {
  let on = false;
  let passwords = new Map<string, string>();
  let signup: { email: string; verified: boolean } | null = null;
  let pending: { email: string; wrong: number } | null = null;
  let session: string | null = null;
  let paused = false;
  let waiting = 0;
  let lastSynced: number | null = null;
  let wrongConfirms = 0;
  let inviteRequired = true;
  let syncKind: AccountView["sync"]["kind"] | null = null;
  let syncReason: AccountView["sync"]["reason"] | null = null;
  let viewDown = false;

  const view = () => ({
    available: on,
    invite_required: inviteRequired,
    platform: "macos",
    ...(session ? { session: { email: session, device_name: "This computer" } } : {}),
    needs_second_factor: pending !== null,
    deletion_scheduled: false,
    sync: {
      kind: syncKind ?? (session ? (paused ? "paused" : "online") : "signed_out"),
      ...(syncReason ? { reason: syncReason } : paused ? { reason: "by_person" } : {}),
      waiting,
      not_synced: 0,
      conflicts: 0,
      ...(lastSynced ? { last_synced: lastSynced } : {}),
    },
  }) satisfies AccountView;

  function signedIn(email: string) {
    session = email;
    pending = null;
    paused = false;
    lastSynced = Math.floor(Date.now() / 1000);
  }

  return {
    reset(gateOn: boolean) {
      on = gateOn;
      passwords = new Map(ACCOUNTS.map((email) => [email, PASSWORD]));
      signup = null;
      pending = null;
      session = null;
      paused = false;
      waiting = 0;
      lastSynced = null;
      wrongConfirms = 0;
      inviteRequired = true;
      syncKind = null;
      syncReason = null;
      viewDown = false;
    },
    /** Changes waiting to upload, for a spec that wants the offline or paused wording. */
    setWaiting(count: number) {
      waiting = count;
    },
    /** Sign-up without an invite step. */
    setInviteRequired(required: boolean) {
      inviteRequired = required;
    },
    /** Forces the sync state the status shows, for the offline and refused wording. */
    setSync(kind: AccountView["sync"]["kind"] | null, reason: AccountView["sync"]["reason"] | null = null) {
      syncKind = kind;
      syncReason = reason;
    },
    /** Makes reading the account fail, for the "couldn't load" page. */
    setViewDown(down: boolean) {
      viewDown = down;
    },
    handle(method: string, path: string, body: unknown): MockResponse {
      const data = (body ?? {}) as Record<string, unknown>;
      const route = `${method} ${path}`;
      if (route === "GET /api/account") return viewDown ? fail(503, "account_offline", "Couldn't reach Orbyn.") : { status: 200, body: view() };
      if (!on) return fail(403, "account_disabled", "Accounts are not available yet.");
      switch (route) {
        case "POST /api/account/sign-up/invite":
          return clean(data.invite) === INVITE ? { status: 204 } : fail(403, "invite_required", "An invite is needed.");
        case "POST /api/account/sign-up/start":
          if (inviteRequired && clean(data.invite) !== INVITE) return fail(403, "invite_required", "An invite is needed.");
          if (data.email === "hard@example.com") return fail(502, "check_failed", "Couldn't finish checking this device.");
          signup = { email: String(data.email ?? ""), verified: false };
          return { status: 204 };
        case "POST /api/account/sign-up/resend":
          return signup ? { status: 204 } : fail(422, "invalid_code", "That code isn't right.");
        case "POST /api/account/sign-up/code":
          if (signup && clean(data.code) === EMAIL_CODE) {
            signup.verified = true;
            return { status: 204 };
          }
          return fail(422, "invalid_code", "That code isn't right.");
        case "POST /api/account/sign-up/register":
          if (!signup?.verified) return fail(422, "invalid_code", "That code isn't right.");
          passwords.set(signup.email, String(data.password ?? ""));
          signedIn(signup.email);
          signup = null;
          return { status: 204 };
        case "POST /api/account/sign-in": {
          const email = String(data.email ?? "");
          if (email === "slow@example.com") return fail(429, "rate_limited", "Too many attempts.", { retry_after_secs: 90 });
          if (email === "down@example.com") return fail(503, "account_offline", "Couldn't reach Orbyn.");
          if (passwords.get(email) !== data.password) return fail(403, "wrong_password", "Wrong email or password.");
          if (email === "two@example.com") {
            pending = { email, wrong: 0 };
            return { status: 200, body: { outcome: "second_factor" } };
          }
          signedIn(email);
          return { status: 200, body: { outcome: "signed" } };
        }
        case "POST /api/account/sign-in/second-factor": {
          if (!pending) return fail(422, "sign_in_closed", "Start signing in again.");
          if (SECOND_FACTOR.includes(clean(data.code))) {
            signedIn(pending.email);
            return { status: 204 };
          }
          pending.wrong += 1;
          if (pending.wrong >= 5) {
            pending = null;
            return fail(422, "sign_in_closed", "Too many wrong codes.");
          }
          return fail(422, "invalid_code", "That code isn't right.", { tries_left: 5 - pending.wrong });
        }
        case "POST /api/account/sign-in/cancel":
          pending = null;
          return { status: 204 };
        case "POST /api/account/confirm":
          if (!session) return fail(403, "reauth_required", "Confirm it's you to do this.");
          if (wrongConfirms >= 5) return fail(429, "rate_limited", "Too many attempts.", { retry_after_secs: 600 });
          if (passwords.get(session) !== data.password) {
            wrongConfirms += 1;
            return fail(403, "wrong_password", "Wrong email or password.");
          }
          wrongConfirms = 0;
          return { status: 204 };
        case "POST /api/account/sync/now":
          lastSynced = Math.floor(Date.now() / 1000);
          return { status: 202 };
        case "POST /api/account/sync/pause":
          paused = true;
          return { status: 204 };
        case "POST /api/account/sync/resume":
          paused = false;
          return { status: 204 };
        default:
          return fail(404, "not_found", "No such account route.");
      }
    },
  };
}
