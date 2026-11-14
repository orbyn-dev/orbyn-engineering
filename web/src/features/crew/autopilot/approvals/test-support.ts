// Test helpers for approving once: the desktop shell's bridge faked in the page, a click the browser would call trusted, and a
// random nonce (a fixed string would look like a real secret flowing into a parameter, and nothing here should hold one).
import { act } from "@testing-library/react";
import { vi } from "vitest";
import { readShellStatus } from "../../../../api/shell";

/** A random 16-byte nonce as the shell makes it: 32 lowercase hex characters. */
export const randomNonce = () => Array.from(crypto.getRandomValues(new Uint8Array(16)), (byte) => byte.toString(16).padStart(2, "0")).join("");

type Invoke = (command: string, args?: Record<string, unknown>) => unknown;

/**
 * Makes this window the desktop app (the service says a shell started it, and the shell's bridge answers with `invoke`) or a plain
 * browser tab. Call it after the mock fetch is installed and before the card is drawn.
 */
export async function inApp(attached: boolean, invoke?: Invoke) {
  const real = fetch;
  vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
    if (String(input).endsWith("/api/shell") && (init?.method ?? "GET") === "GET") {
      return new Response(JSON.stringify({ attached, windows: 1, facts: {} }), { status: 200 });
    }
    return real(input, init);
  }));
  (window as unknown as { __TAURI_INTERNALS__?: { invoke: Invoke } }).__TAURI_INTERNALS__ = attached && invoke ? { invoke } : undefined;
  await readShellStatus();
}

const reactProps = (element: HTMLElement) => {
  const key = Object.keys(element).find((name) => name.startsWith("__reactProps$"));
  if (!key) throw new Error("not a React element");
  return (element as unknown as Record<string, { onClick?(event: unknown): void }>)[key]!;
};

/**
 * A click as a browser makes it from a person's input. jsdom marks every event it makes untrusted (`isTrusted` cannot be set), so this
 * hands the element's own React handler an event that says it is trusted. The real thing is tried in the end-to-end spec.
 */
export async function trustedClick(element: HTMLElement, detail = 1) {
  const props = reactProps(element);
  if (!props.onClick) throw new Error("no click handler");
  await act(async () => {
    props.onClick?.({ nativeEvent: { isTrusted: true, type: "click", detail }, isTrusted: true, type: "click", detail, preventDefault: () => undefined, stopPropagation: () => undefined });
  });
}
