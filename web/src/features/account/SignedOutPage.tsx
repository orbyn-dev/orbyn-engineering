import { Info } from "lucide-react";
import { Button } from "../../ui/controls";

/** What an account adds, and what it never moves. The page a signed-out person sees first. */
export function SignedOutHome({ inviteRequired, onSignIn, onCreate }: { inviteRequired: boolean; onSignIn(): void; onCreate(): void }) {
  return (
    <>
      <div className="ac-note">
        <Info size={14} aria-hidden />
        <div>
          <b>Engineering works without an account.</b> Your threads, code and projects stay on this computer, signed in or not.
        </div>
      </div>
      <section className="ac-card" data-wide data-full aria-label="Use an account to sync your settings">
        <h2>Use an account to sync your settings</h2>
        <p>Sign in to keep your personal settings the same on every computer where you use Orbyn. Everything else stays where it is.</p>
        <div className="ac-acts">
          <Button tone="primary" onClick={onSignIn}>Sign in</Button>
          <Button onClick={onCreate}>Create account</Button>
        </div>
        {inviteRequired ? <p className="ac-sub">Creating an account needs an invite code while sign-up is invite-only.</p> : null}
      </section>
      <div className="ac-two">
        <div>
          <b>What syncs</b>
          <ul>
            <li>Appearance and theme</li>
            <li>Keybindings</li>
            <li>Chat and diff preferences</li>
          </ul>
        </div>
        <div>
          <b>What never syncs</b>
          <ul>
            <li>Threads and messages</li>
            <li>Your code and files</li>
            <li>Secrets and provider sign-ins</li>
            <li>Terminal history</li>
          </ul>
        </div>
      </div>
    </>
  );
}
