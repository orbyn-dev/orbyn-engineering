import { Component, type ReactNode } from "react";
import { ApiFailure } from "../../api/client";
import { ErrorState } from "../../product/ui/states";
import { reloadPage } from "./pageReload";

/** What a failed import is drawn as: not a service refusal, so no request, no status, nothing retried by a read. */
const NOT_LOADED = new ApiFailure("screen_not_loaded", "This screen's code did not arrive. Reload to open it again.", 0);

/**
 * Where a deferred screen's failed import lands.
 *
 * The route screens (`app/shell/routes.tsx`), Settings (`app/shell/Shell.tsx`) and the sign-in screen
 * (`app/App.tsx`) are their own chunks. A chunk that does not arrive (a redeploy while the window was open, a
 * cache that lost it) rejects its import, and React throws that rejection to the nearest error boundary: with
 * none, the whole app unmounts and the window stays blank. React also keeps the rejected import, so rendering
 * the screen again cannot retry it, and a page reload is the only way back, which is what the fallback offers.
 *
 * A caller that can move to another screen keys this by what identifies that screen, so the next screen is
 * tried instead of keeping this failure.
 */
export class ScreenBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  override state: { failed: boolean } = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  override render(): ReactNode {
    if (!this.state.failed) return this.props.children;
    return (
      <div className="page">
        <ErrorState title="This screen did not open" failure={NOT_LOADED} onRetry={reloadPage} />
      </div>
    );
  }
}
