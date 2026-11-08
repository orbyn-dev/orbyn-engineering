// DEVELOPMENT ONLY. What a pull request says it built, read from its description.
//
// The template has one line: "Frames built: pr-list-failed, pr-detail-running @ v56": the ids of the screens this app can draw for a
// pull request (catalog.mjs) and the version of the designs they were built to. "none" (or "n/a") says the pull request draws no frame.
// The ids are only names: nothing here reads the designs.

export const MAX_FRAMES = 12;

/**
 * @param {string} body the pull request description
 * @returns {{ line: "missing" | "empty" | "none" | "ids", ids: string[], version: string | null }}
 */
export function framesBuilt(body) {
  const clean = String(body ?? "").replace(/<!--[\s\S]*?-->/g, "");
  const m = /^[ \t]*(?:[-*][ \t]+)?Frames built:[ \t]*(.*)$/im.exec(clean);
  if (!m) return { line: "missing", ids: [], version: null };
  let text = m[1].trim();
  if (text === "") return { line: "empty", ids: [], version: null };
  let version = null;
  const v = /^(.*?)\s@\s*(v\d+)\s*$/.exec(text);
  if (v) {
    text = v[1].trim();
    version = v[2];
  }
  if (/^(none|n\/a|no frames?|-)\.?$/i.test(text)) return { line: "none", ids: [], version };
  const ids = [...new Set(text.split(/[\s,]+/).map((x) => x.replace(/^`|`$/g, "").trim()).filter(Boolean))];
  return ids.length ? { line: "ids", ids, version } : { line: "empty", ids: [], version };
}

/**
 * Which of the named frames can be drawn: the ones the catalog knows, up to `max`.
 * @param {string[]} ids
 * @param {Record<string, unknown>} known the catalog
 */
export function selectFrames(ids, known, max = MAX_FRAMES) {
  const unknown = ids.filter((id) => !Object.prototype.hasOwnProperty.call(known, id));
  const usable = ids.filter((id) => !unknown.includes(id));
  return { use: usable.slice(0, max), unknown, over: usable.slice(max) };
}
