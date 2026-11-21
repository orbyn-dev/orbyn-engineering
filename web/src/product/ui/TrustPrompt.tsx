/** The words of the trust prompt, one copy for the first-run page and for a repository of a project. */
export const TRUST_TITLE = (name: string) => `Trust ${name}?`;
export const TRUST_CONFIRM = "Trust folder";
export const TRUST_DECLINE = "Don’t trust";
export const TRUST_WARNING = "Agents can read files, run commands and edit code in this folder. Trust folders you know.";

/** The folder as the person writes it: the home directory shortened to ~. */
export function homeRelative(path: string): string {
  return path.replace(/^(?:\/Users|\/home)\/[^/]+(?=\/|$)/, "~").replace(/^[A-Za-z]:\\Users\\[^\\]+/, "~");
}

/** What the prompt says under its title: the warning, and the path as text. */
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
