import { describe, expect, it } from "vitest";
import {
  CODE_ALPHABET,
  authenticatorInput,
  emailCodeInput,
  inviteInput,
  isAuthenticatorComplete,
  isInviteComplete,
  recoveryInput,
  retryText,
  stripCode,
} from "./codes";

describe("typed codes", () => {
  it("the alphabet has 30 symbols and never I, L, O, U, 0 or 1", () => {
    expect(CODE_ALPHABET).toHaveLength(30);
    for (const bad of "ILOU01") expect(CODE_ALPHABET).not.toContain(bad);
  });

  it("an invite ignores case, hyphens and spaces and shows XXXX-XXXX-XXXX-XXXX", () => {
    const want = "K7QM-2XWH-9QXA-3C2D";
    for (const typed of ["k7qm-2xwh-9qxa-3c2d", "K7QM 2XWH 9QXA 3C2D", " k7qm2xwh9qxa3c2d ", "K7QM2XWH-9QXA 3c2d"]) {
      expect(inviteInput(typed)).toBe(want);
    }
    expect(isInviteComplete(want)).toBe(true);
    expect(isInviteComplete("K7QM-2XWH")).toBe(false);
  });

  it("an invite stops at 16 characters", () => {
    expect(stripCode("K7QM2XWH9QXA3C2DEXTRA", 16)).toBe("K7QM2XWH9QXA3C2D");
  });

  it("the emailed code gets its hyphen after four characters", () => {
    expect(emailCodeInput("abcd")).toBe("ABCD");
    expect(emailCodeInput("abcde")).toBe("ABCD-E");
    expect(emailCodeInput("abcd efgh")).toBe("ABCD-EFGH");
  });

  it("a recovery code is XXXXX-XXXXX and ignores case, the hyphen and spaces", () => {
    expect(recoveryInput("kjqmr 4wxtp")).toBe("KJQMR-4WXTP");
    expect(recoveryInput("kjqmr-4wxtp")).toBe("KJQMR-4WXTP");
  });

  it("an authenticator code takes digits only, in two groups of three", () => {
    expect(authenticatorInput("48a2 91-3")).toBe("482 913");
    expect(isAuthenticatorComplete("482 913")).toBe(true);
    expect(isAuthenticatorComplete("482 91")).toBe(false);
  });

  it("a retry time counts in minutes, then says under a minute", () => {
    expect(retryText(12 * 60)).toBe("in 12 minutes");
    expect(retryText(61)).toBe("in 2 minutes");
    expect(retryText(60)).toBe("in 1 minute");
    expect(retryText(59)).toBe("in under a minute");
  });
});
