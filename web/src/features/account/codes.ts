/** The invite and the emailed code share one 30-symbol alphabet: no 0, O, 1, I, L or U. */
export const CODE_ALPHABET = "23456789ABCDEFGHJKMNPQRSTVWXYZ";

export const INVITE_LENGTH = 16;
export const EMAIL_CODE_LENGTH = 8;
export const AUTHENTICATOR_LENGTH = 6;
export const RECOVERY_LENGTH = 10;

/** Capitals with no hyphens or spaces, at most `max` characters: what the person typed or pasted. */
export function stripCode(raw: string, max: number): string {
  return raw.replace(/[\s-]/g, "").toUpperCase().slice(0, max);
}

/** The letters and digits in groups of `size`, joined by hyphens, as the code is shown while typing. */
export function groupCode(code: string, size: number): string {
  const groups: string[] = [];
  for (let index = 0; index < code.length; index += size) groups.push(code.slice(index, index + size));
  return groups.join("-");
}

export const inviteInput = (raw: string) => groupCode(stripCode(raw, INVITE_LENGTH), 4);
export const emailCodeInput = (raw: string) => groupCode(stripCode(raw, EMAIL_CODE_LENGTH), 4);
/** `XXXXX-XXXXX`: ten letters and digits, a hyphen after five. */
export const recoveryInput = (raw: string) => groupCode(stripCode(raw, RECOVERY_LENGTH), 5);
/** Six digits, shown as two groups of three. */
export function authenticatorInput(raw: string): string {
  const digits = raw.replace(/\D/g, "").slice(0, AUTHENTICATOR_LENGTH);
  return digits.length > 3 ? `${digits.slice(0, 3)} ${digits.slice(3)}` : digits;
}

export const isInviteComplete = (shown: string) => stripCode(shown, INVITE_LENGTH).length === INVITE_LENGTH;
export const isEmailCodeComplete = (shown: string) => stripCode(shown, EMAIL_CODE_LENGTH).length === EMAIL_CODE_LENGTH;
export const isRecoveryComplete = (shown: string) => stripCode(shown, RECOVERY_LENGTH).length === RECOVERY_LENGTH;
export const isAuthenticatorComplete = (shown: string) => shown.replace(/\D/g, "").length === AUTHENTICATOR_LENGTH;

/** "in 12 minutes", or "in under a minute" when it is close. */
export function retryText(seconds: number): string {
  if (seconds < 60) return "in under a minute";
  const minutes = Math.ceil(seconds / 60);
  return `in ${minutes} ${minutes === 1 ? "minute" : "minutes"}`;
}
