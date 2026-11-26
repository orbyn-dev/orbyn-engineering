/**
 * One person as the issues surfaces draw them: the login's first two letters in a circle, then the login. The login is the
 * tracker's text and is drawn as text only.
 */
export function Person({ login }: { login: string }) {
  return <span className="iss-person"><span className="iss-avatar" aria-hidden>{login.slice(0, 2).toUpperCase()}</span>{login}</span>;
}
