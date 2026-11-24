// The custom search address (parity H2): a template with %s where the encoded search words go. It is data from a person's
// settings: it is checked, never evaluated, and only an http(s) address is used.

export type SearchAddressCheck = { ok: true } | { ok: false; reason: "empty" | "scheme" | "token"; message: string | null };

/** The words the settings Preview row shows. */
export const SEARCH_PREVIEW_WORDS = "orbyn designs";

const TOKEN = "%s";

/** Whether a custom address can be used, and the sentence for the field when it cannot. An empty field is not an error. */
export function checkSearchAddress(text: string): SearchAddressCheck {
  const address = text.trim();
  if (address === "") return { ok: false, reason: "empty", message: null };
  if (!/^https?:\/\/[^\s]/i.test(address)) return { ok: false, reason: "scheme", message: "Start the address with https:// or http://." };
  if (/\s/.test(address)) return { ok: false, reason: "scheme", message: "Start the address with https:// or http://." };
  const count = address.split(TOKEN).length - 1;
  if (count === 0) return { ok: false, reason: "token", message: "Add %s where the search words go." };
  if (count > 1) return { ok: false, reason: "token", message: "Use %s once, where the search words go." };
  return { ok: true };
}

/** The page the words open, or null when the address is not usable. The words are encoded as the built-in engines encode them. */
export function customSearchUrl(address: string, words: string): string | null {
  if (!checkSearchAddress(address).ok) return null;
  const trimmed = address.trim();
  const at = trimmed.indexOf(TOKEN);
  return trimmed.slice(0, at) + encodeURIComponent(words) + trimmed.slice(at + TOKEN.length);
}
