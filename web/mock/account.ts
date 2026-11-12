/**
 * The mock's account: the same routes, answers and throwaway accounts as the service's development
 * stand-in (`orbyn_core::account::dev`), so the screens and their end-to-end specs run without a service.
 * The default scenario has the gate off (`available: false`); the `account` scenario turns it on.
 *
 * Known values: invite K7QM-2XWH-9QXA-3C2D, emailed code ABCD-EFGH, password "correct horse battery"
 * for dev@, two@ (asks for 123456 or the recovery code ABCDE-FGH23), slow@ (rate limited), down@ (offline).
 * Sign-up goes invite, email, password (which creates the account and emails the code), then the code, which signs in.
 * The password step with hard@ fails its device check; busy@ takes a moment in the service (set `/__mock/delay/<ms>` to see it here).
 * A new device asks a trusted one, which then shows six digits (418 207 here) that the new device types: three tries.
 */
import type { AccountDevice } from "../src/generated/AccountDevice.ts";
import type { AccountApprovalRequest } from "../src/generated/AccountApprovalRequest.ts";
import type { AccountApprovalView } from "../src/generated/AccountApprovalView.ts";
import type { AccountView } from "../src/generated/AccountView.ts";
import type { MockResponse } from "./api.ts";

const INVITE = "K7QM2XWH9QXA3C2D";
const EMAIL_CODE = "ABCDEFGH";
const PASSWORD = "correct horse battery";
const SECOND_FACTOR = ["123456", "ABCDEFGH23"];
const ACCOUNTS = ["dev@example.com", "two@example.com", "slow@example.com", "down@example.com", "approve@example.com", "deny@example.com", "late@example.com", "expired@example.com", "connect@example.com", "removed@example.com"];
/** The six digits a trusted device shows once it allowed a request, and the tries the new device has for them. */
const DIGITS = "418207";
const DIGIT_TRIES = 3;
/** What a trusted device answers a new device, by the account that signs in; `late@` is never answered. */
const CANNED: Record<string, AccountApprovalView["state"] | null> = { "approve@example.com": "enter_digits", "deny@example.com": "denied", "late@example.com": null, "expired@example.com": "timed_out" };

const fail = (status: number, code: string, message: string, detail?: unknown): MockResponse => ({
  status,
  body: { code, message, ...(detail ? { detail } : {}) },
});
const clean = (text: unknown) => String(text ?? "").replace(/[\s-]/g, "").toUpperCase();

export function createAccountMock() {
  let on = false;
  let passwords = new Map<string, string>();
  let signup: { email: string; password: string; wrong: number } | null = null;
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
  let deletionScheduled = false;
  let syncStuck = false;
  let devicesDown = false;
  // The throwaway account's devices, the same three the service stand-in lists.
  const freshDevices = (): AccountDevice[] => [
    { id: "dev-this", name: "This computer", platform: "macOS 15", app: "engineering", trusted_since: "2026-07-12T09:00:00Z", current: true },
    { id: "dev-phone", name: "iPhone", platform: "iOS 18", app: "planner", trusted_since: "2026-08-08T09:00:00Z", current: false },
    { id: "dev-mini", name: "Mac mini", platform: "macOS 15", app: "engineering", trusted_since: "2026-09-26T09:00:00Z", current: false },
  ];
  let deviceList = freshDevices();
  /** One confirmation for one action (and device), used up by the action. */
  let grant: { action: string; device: string | null; nonce: string } | null = null;
  let nonces = 0;
  // This computer's own request to sign in, and what a trusted device answered.
  let approval: { answer: AccountApprovalView["state"]; askedAt: number; autoAt: number | null; autoAnswer: AccountApprovalView["state"] | null; email: string; tries: number } | null = null;
  const APPROVAL_LIFE_SECS = 600;
  // Requests from other devices, waiting for this one to answer.
  const freshIncoming = (): AccountApprovalRequest[] => [
    { id: "req-ipad", device_name: "iPad", app: "planner", near: "Near Melbourne", asked_at: new Date().toISOString(), allowed: false, expires_in_secs: 540 },
  ];
  let incoming = freshIncoming();
  // Joining this computer's settings: asked once after a sign-in on a computer that has settings.
  let connectNeeded = false;
  let connect: AccountView["connect"] = "connected";
  let summaryPending = false;
  /** The first sync of `connect@` takes a few seconds, so its progress dialog can be seen. */
  let summaryAt = 0;
  let endedSeen = false;

  const view = () => ({
    available: on,
    invite_required: inviteRequired,
    platform: "macos",
    ...(session ? { session: { email: session, device_name: "This computer" } } : {}),
    needs_second_factor: pending !== null,
    ...(approval && !session ? { approval: approvalView() } : {}),
    ended_notice: syncKind === "signed_out" && syncReason === "session_ended" && !endedSeen,
    connect,
    ...(summaryPending && connect === "connected" && !syncKind && Date.now() >= summaryAt ? { connect_summary: { synced: 24 } } : {}),
    deletion_scheduled: deletionScheduled,
    sync: {
      kind: syncKind ?? (session ? (connect !== "connected" ? "not_connected" : paused ? "paused" : "online") : "signed_out"),
      ...(syncReason ? { reason: syncReason } : paused ? { reason: "by_person" } : {}),
      waiting,
      not_synced: 0,
      conflicts: 0,
      ...(lastSynced ? { last_synced: lastSynced } : {}),
    },
  }) satisfies AccountView;

  function approvalView(): AccountApprovalView {
    if (!approval) return { state: "waiting", expires_in_secs: 0 };
    const left = Math.max(0, APPROVAL_LIFE_SECS - Math.floor((Date.now() - approval.askedAt) / 1000));
    const open = approval.answer === "waiting" || approval.answer === "enter_digits";
    if (open && left === 0) approval.answer = "timed_out";
    const wrong = approval.answer === "enter_digits" && approval.tries < DIGIT_TRIES;
    return {
      state: approval.answer,
      expires_in_secs: approval.answer === "waiting" || approval.answer === "enter_digits" ? left : 0,
      ...(wrong ? { tries_left: approval.tries } : {}),
    };
  }

  function signedOut() {
    session = null;
    pending = null;
    paused = false;
    grant = null;
    approval = null;
    connect = "connected";
    summaryPending = false;
  }

  function signedIn(email: string) {
    session = email;
    pending = null;
    approval = null;
    paused = false;
    // A computer with settings asks once how to join them, when the spec says it has some.
    connect = connectNeeded || email === "connect@example.com" ? "needed" : "connected";
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
      deletionScheduled = false;
      syncStuck = false;
      devicesDown = false;
      deviceList = freshDevices();
      grant = null;
      approval = null;
      incoming = freshIncoming();
      connectNeeded = false;
      endedSeen = false;
      connect = "connected";
      summaryPending = false;
    },
    /** Changes waiting to upload, for a spec that wants the offline or paused wording. */
    setWaiting(count: number) {
      waiting = count;
    },
    /** One door for every state a spec cannot reach by clicking. */
    setKnobs(knobs: {
      inviteRequired?: boolean;
      sync?: [AccountView["sync"]["kind"] | null, AccountView["sync"]["reason"]?];
      down?: boolean;
      devices?: { deletionScheduled?: boolean; down?: boolean; onlyThis?: boolean; syncStuck?: boolean; five?: boolean };
      /** What a trusted device answered this computer's request. */
      answer?: AccountApprovalView["state"];
      /** The next sign-in is on a computer that has settings to join. */
      connectNeeded?: boolean;
      /** A request from another device that has ended since it was listed. */
      dropIncoming?: string;
    }) {
      if (knobs.inviteRequired !== undefined) inviteRequired = knobs.inviteRequired;
      if (knobs.sync) {
        syncKind = knobs.sync[0];
        syncReason = knobs.sync[1] ?? null;
      }
      if (knobs.down !== undefined) viewDown = knobs.down;
      if (knobs.devices) {
        if (knobs.devices.deletionScheduled !== undefined) deletionScheduled = knobs.devices.deletionScheduled;
        if (knobs.devices.down !== undefined) devicesDown = knobs.devices.down;
        if (knobs.devices.syncStuck !== undefined) syncStuck = knobs.devices.syncStuck;
        if (knobs.devices.onlyThis) deviceList = deviceList.filter((device) => device.current);
        // Five devices: the Account page lists three of them, Manage devices all five.
        if (knobs.devices.five) deviceList = [...freshDevices(),
          { id: "dev-laptop", name: "Work laptop", platform: "Windows 11", app: "engineering", trusted_since: "2026-10-30T09:00:00Z", current: false },
          { id: "dev-firefox", name: "Firefox on the work PC", platform: "Web", app: "planner", trusted_since: "2026-11-06T09:00:00Z", current: false }];
      }
      if (knobs.answer && approval) approval.answer = knobs.answer;
      if (knobs.connectNeeded !== undefined) connectNeeded = knobs.connectNeeded;
      if (knobs.dropIncoming) incoming = incoming.filter((request) => request.id !== knobs.dropIncoming);
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
          if (String(data.password ?? "").length < 12) return fail(422, "weak_password", "Use at least 12 characters for the password.");
          signup = { email: String(data.email ?? ""), password: String(data.password ?? ""), wrong: 0 };
          return { status: 204 };
        case "POST /api/account/sign-up/resend":
          if (!signup) return fail(422, "invalid_code", "That code isn't right.");
          signup.wrong = 0;
          return { status: 204 };
        case "POST /api/account/sign-up/code":
          // Every failure of the code answers the same way, and the fifth wrong one refuses all until a new code is sent.
          if (signup && signup.wrong < 5 && clean(data.code) === EMAIL_CODE) {
            passwords.set(signup.email, signup.password);
            signedIn(signup.email);
            signup = null;
            return { status: 204 };
          }
          if (signup) signup.wrong += 1;
          return fail(422, "invalid_code", "That code isn't right.");
        case "POST /api/account/sign-in": {
          const email = String(data.email ?? "");
          if (email === "slow@example.com") return fail(429, "rate_limited", "Too many attempts.", { retry_after_secs: 90 });
          if (email === "down@example.com") return fail(503, "account_offline", "Couldn't reach Orbyn.");
          if (passwords.get(email) !== data.password) return fail(403, "wrong_password", "Wrong email or password.");
          if (email === "two@example.com") {
            pending = { email, wrong: 0 };
            return { status: 200, body: { outcome: "second_factor" } };
          }
          if (email === "removed@example.com") {
            // The service ended this computer's session: signed out, with the reason said.
            syncKind = "signed_out";
            syncReason = "session_ended";
            return { status: 200, body: { outcome: "signed" } };
          }
          if (email in CANNED) {
            const canned = CANNED[email] ?? null;
            approval = { answer: canned === "timed_out" ? "timed_out" : "waiting", askedAt: Date.now(), autoAt: canned && canned !== "timed_out" ? Date.now() + 1500 : null, autoAnswer: canned, email, tries: DIGIT_TRIES };
            return { status: 200, body: { outcome: "awaiting_approval" } };
          }
          signedIn(email);
          return { status: 200, body: { outcome: "signed" } };
        }
        case "POST /api/account/sign-in/approval": {
          if (!approval) return { status: 200, body: null };
          if (approval.answer === "waiting" && approval.autoAt !== null && Date.now() >= approval.autoAt) approval.answer = approval.autoAnswer ?? "waiting";
          return { status: 200, body: approvalView() };
        }
        case "POST /api/account/sign-in/digits": {
          if (!approval) return { status: 200, body: null };
          approvalView();
          const typed = clean(data.digits);
          if (!/^\d{6}$/.test(typed)) return fail(422, "invalid_digits", "Type the six digits.");
          if (approval.answer !== "enter_digits") return approval.answer === "waiting" ? fail(409, "request_not_allowed", "A trusted device has not allowed this yet.") : { status: 200, body: approvalView() };
          if (typed === DIGITS) {
            const email = approval.email;
            const shown: AccountApprovalView = { state: "approved", expires_in_secs: 0 };
            signedIn(email);
            return { status: 200, body: shown };
          }
          approval.tries -= 1;
          if (approval.tries <= 0) approval.answer = "locked";
          return { status: 200, body: approvalView() };
        }
        case "GET /api/account/approvals":
          if (!session) return fail(403, "reauth_required", "Sign in first.");
          return { status: 200, body: incoming };
        case "POST /api/account/connect":
          if (session && connect !== "connected") {
            connect = "connected";
            summaryPending = true;
            summaryAt = session === "connect@example.com" ? Date.now() + 3000 : 0;
          }
          return { status: 204 };
        case "POST /api/account/connect/later":
          if (session && connect === "needed") connect = "declined";
          return { status: 204 };
        case "POST /api/account/ended/seen":
          endedSeen = true;
          return { status: 204 };
        case "POST /api/account/connect/seen":
          summaryPending = false;
          return { status: 204 };
        case "POST /api/account/connect/use-device":
        case "POST /api/account/connect/use-account": {
          const policy = route.endsWith("use-device") ? "use_device" : "use_account";
          const allowed = grant?.action === "connect_settings" && grant.device === policy;
          grant = null;
          if (!session || !allowed) return fail(403, "reauth_required", "Confirm it's you to do this.");
          if (connect !== "connected") {
            connect = "connected";
            summaryPending = true;
            summaryAt = session === "connect@example.com" ? Date.now() + 3000 : 0;
          }
          return { status: 204 };
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
          approval = null;
          return { status: 204 };
        case "POST /api/account/confirm":
          if (!session) return fail(403, "reauth_required", "Confirm it's you to do this.");
          if (wrongConfirms >= 5) return fail(429, "rate_limited", "Too many attempts.", { retry_after_secs: 600 });
          if (passwords.get(session) !== data.password) {
            wrongConfirms += 1;
            return fail(403, "wrong_password", "Wrong email or password.");
          }
          wrongConfirms = 0;
          if (!["remove_device", "sign_out_everywhere", "approve_device", "connect_settings"].includes(String(data.action))) return fail(400, "invalid", "Say what to confirm.");
          if ((data.action === "remove_device" || data.action === "approve_device") && !data.device_id) return fail(400, "invalid", "Say which one.");
          if (data.action === "connect_settings" && data.policy !== "use_device" && data.policy !== "use_account") return fail(400, "invalid", "Say which choice.");
          nonces += 1;
          grant = { action: String(data.action), device: data.action === "sign_out_everywhere" ? null : String(data.action === "connect_settings" ? data.policy : data.device_id), nonce: `nonce-${nonces}` };
          return { status: 200, body: { nonce: grant.nonce } };
        case "POST /api/account/confirm/cancel":
          if (grant?.nonce === data.nonce) grant = null;
          return { status: 204 };
        case "GET /api/account/devices":
          if (!session) return fail(403, "reauth_required", "Sign in first.");
          return devicesDown ? fail(503, "account_offline", "Couldn't reach Orbyn.") : { status: 200, body: deviceList };
        case "POST /api/account/sign-out":
          if (session) signedOut();
          return { status: 204 };
        case "POST /api/account/sign-out-everywhere": {
          const allowed = grant?.action === "sign_out_everywhere";
          grant = null;
          if (!session || !allowed) return fail(403, "reauth_required", "Confirm it's you to do this.");
          if (deletionScheduled) return fail(409, "deletion_scheduled", "Deletion is scheduled.");
          signedOut();
          return { status: 204 };
        }
        case "POST /api/account/sync/now":
          // The worker uploads what waits, unless a spec says it cannot.
          if (!syncStuck) {
            lastSynced = Math.floor(Date.now() / 1000);
            waiting = 0;
          }
          return { status: 202 };
        case "POST /api/account/sync/pause":
          paused = true;
          return { status: 204 };
        case "POST /api/account/sync/resume":
          paused = false;
          return { status: 204 };
        default: {
          const answer = /^POST \/api\/account\/approvals\/([^/]+)\/(approve|deny)$/.exec(route);
          if (answer) {
            const id = decodeURIComponent(answer[1] ?? "");
            if (answer[2] === "approve") {
              const allowed = grant?.action === "approve_device" && grant.device === id;
              grant = null;
              if (!session || !allowed) return fail(403, "reauth_required", "Confirm it's you to do this.");
            }
            const found = incoming.find((request) => request.id === id);
            if (!found) return fail(404, "request_gone", "That request has ended.");
            if (answer[2] === "approve") {
              // Allowing shows the digits; the request stays listed, as allowed, until it is closed.
              if (found.allowed) return fail(404, "request_gone", "That request has ended.");
              incoming = incoming.map((request) => (request.id === id ? { ...request, allowed: true } : request));
              return { status: 200, body: { digits: "418 207" } };
            }
            incoming = incoming.filter((request) => request.id !== id);
            return { status: 204 };
          }
          const removal = /^DELETE \/api\/account\/devices\/([^/]+)$/.exec(route);
          if (removal) {
            const id = decodeURIComponent(removal[1] ?? "");
            const allowed = grant?.action === "remove_device" && grant.device === id;
            grant = null;
            if (!session || !allowed) return fail(403, "reauth_required", "Confirm it's you to do this.");
            deviceList = deviceList.filter((device) => device.id !== id || device.current);
            return { status: 204 };
          }
          return fail(404, "not_found", "No such account route.");
        }
      }
    },
  };
}
