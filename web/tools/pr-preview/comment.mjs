// DEVELOPMENT ONLY. The comment the UI preview posts on a pull request. Ours only: the design frames are private and never shown here.

export const MARKER = "<!-- ui-preview -->";
const LABEL = { light: "Light", dark: "Dark", dim: "Dim" };

const img = (url, alt) => `<img src="${url}" alt="${alt}" width="700">`;

/** Asks for the line, and says what it is for. Never blocks anything. */
export function askForFrames(reason) {
  return [
    MARKER,
    "## UI preview",
    "",
    reason === "empty"
      ? "The **Frames built** line is empty."
      : "This pull request changes the web app but has no **Frames built** line.",
    "",
    "Add one to the description so the screens can be shown here: the ids of the screens you built and the design version, for example `Frames built: pr-list-failed, pr-detail-running @ v56`. A pull request that draws nothing says `Frames built: none`. This is a request, not a check: nothing is blocked.",
    "",
  ].join("\n");
}

/**
 * @param {object} p
 * @param {{ id: string, images: Record<string, string> }[]} p.units frames drawn; images keyed "light", "dark", "dim"
 * @param {string[]} p.failed frames that could not be drawn
 * @param {string[]} p.unknown ids the catalog does not have
 * @param {string[]} p.over ids beyond the cap
 * @param {string | null} p.version design version from the line
 * @param {(rel: string) => string} p.url
 * @param {string} p.runUrl
 * @param {string} p.sha
 * @param {string | null} p.error why nothing could be drawn at all
 */
export function buildComment(p) {
  const lines = [MARKER, "## UI preview", ""];
  if (p.error) {
    lines.push(`The preview could not be built for \`${p.sha.slice(0, 7)}\`: ${p.error}. See the [workflow run](${p.runUrl}). Nothing is blocked.`, "");
    return lines.join("\n");
  }
  lines.push(
    `Frames built${p.version ? ` @ ${p.version}` : ""}: ${p.units.length} drawn from the dev-only mock app (fake data, no account), commit \`${p.sha.slice(0, 7)}\`. Light first; Dark and Dim are folded. This shows our screens only, not the designs.`,
    "",
  );
  for (const u of p.units) {
    lines.push(`### \`${u.id}\``, "");
    if (u.images.light) lines.push(img(p.url(u.images.light), `${u.id}, Light`), "");
    const more = ["dark", "dim"].filter((t) => u.images[t]);
    if (more.length) {
      lines.push("<details><summary>Dark and Dim</summary>", "");
      for (const t of more) lines.push(`**${LABEL[t]}**`, "", img(p.url(u.images[t]), `${u.id}, ${LABEL[t]}`), "");
      lines.push("</details>", "");
    }
  }
  if (p.failed.length) lines.push(`Could not draw: ${p.failed.map((f) => `\`${f}\``).join(", ")}. See the [workflow run](${p.runUrl}).`, "");
  if (p.unknown.length) lines.push(`Not screens the preview knows: ${p.unknown.map((f) => `\`${f}\``).join(", ")} (see \`web/tools/pr-preview/catalog.mjs\`).`, "");
  if (p.over.length) lines.push(`Over the limit of 12, not drawn: ${p.over.map((f) => `\`${f}\``).join(", ")}.`, "");
  lines.push("---", `Every image, as one download: [this workflow run](${p.runUrl}).`, "");
  return lines.join("\n");
}
