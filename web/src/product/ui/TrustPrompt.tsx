/** The words of the trust prompt, one copy for the first-run page and for a repository of a project. */
export const TRUST_TITLE = (name: string) => `Trust ${name}?`;
export const TRUST_CONFIRM = "Trust folder";
export const TRUST_DECLINE = "Don’t trust";
export const TRUST_WARNING = "Agents can read files, run commands and edit code in this folder. Trust folders you know.";

/**
 * Says why the prompt offers no Trust button. Trusting is a `Person` route, which needs the person key only the desktop
 * shell holds (crates/orbynd/src/routes/classes.rs), so a browser tab cannot do it: the prompt says so and keeps the
 * way out, rather than offering a button whose only answer is `403 desktop_required`. The same rule and the same
 * wording pattern as the Crew designs (v34) use for "Approving once needs the Orbyn desktop app."
 */
export const TRUST_DESKTOP_ONLY = "Trusting a folder needs the Orbyn desktop app.";

/** The folder as the person writes it: the home directory shortened to ~. */
export function homeRelative(path: string): string {
  return path.replace(/^(?:\/Users|\/home)\/[^/]+(?=\/|$)/, "~").replace(/^[A-Za-z]:\\Users\\[^\\]+/, "~");
}

/**
 * What the prompt says under its title: the warning, and the path as text.
 *
 * The facts row is the design's `.card` (padding 12px 14px, radius 12px, gap 4px as the frame overrides it) and `.kv`
 * (label at the start, value at the end of the row), and the value is `.kv b`, which carries no `text-align`: a path that
 * wraps keeps one left edge instead of ragging against the right one (engineer-designs frame `trust`). The value wraps
 * inside its half of the row (`overflow-wrap: anywhere`), because a folder's own name can be longer than the row.
 */
export function TrustBody({ path }: { path: string }) {
  return (
    <>
      <p>{TRUST_WARNING}</p>
      <dl className="trust-facts">
        <div>
          <dt>Path</dt>
          <dd><code className="trust-path">{homeRelative(path)}</code></dd>
        </div>
      </dl>
    </>
  );
}
