import { expect, it, vi } from "vitest";
import { deferNavigation, registerNavigationGuard, type NavigationResume } from "./navigationGuard";

it("is a no-op without an active editor and fences cancelled/unmounted stale intents", () => {
  const navigate = vi.fn(); expect(deferNavigation(navigate)).toBe(false); expect(navigate).not.toHaveBeenCalled();
  let pending: NavigationResume = () => undefined;
  const first = registerNavigationGuard((resume) => { pending = resume; });
  expect(deferNavigation(navigate)).toBe(true); const cancelled = pending;
  first.cancelAttempt(); cancelled(); expect(navigate).not.toHaveBeenCalled();
  deferNavigation(navigate); const unmounted = pending;
  first.dispose(); const second = registerNavigationGuard((resume) => { pending = resume; });
  first.dispose(); unmounted(); expect(navigate).not.toHaveBeenCalled();
  expect(deferNavigation(navigate)).toBe(true);
  const discard = vi.fn(); pending(discard); expect(discard).toHaveBeenCalledTimes(1); expect(navigate).toHaveBeenCalledTimes(1);
  pending(); expect(navigate).toHaveBeenCalledTimes(1); second.dispose();
  expect(deferNavigation(navigate)).toBe(false);
});
