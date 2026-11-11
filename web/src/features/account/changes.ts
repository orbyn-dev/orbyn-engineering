/**
 * Every window that shows the account reads it again when the service says it changed. The service says so
 * on its event stream; this also tells the readers on this page at once when this page's own request changed
 * it, so the sidebar and the title bar follow the Account page without waiting for the stream.
 */
const bus = new EventTarget();

export function announceAccountChange(): void {
  bus.dispatchEvent(new Event("changed"));
}

export function onAccountChange(listener: () => void): () => void {
  bus.addEventListener("changed", listener);
  return () => bus.removeEventListener("changed", listener);
}

/**
 * "Connect settings" in the sync popover opens the same dialog as the Account page's. The popover sits in the sidebar,
 * so the Account page may not be open yet: the ask is kept until a page that can show the dialog takes it.
 */
let connectAsked = false;
const connectBus = new EventTarget();

export function requestConnect(): void {
  connectAsked = true;
  connectBus.dispatchEvent(new Event("ask"));
}

/** True once for each ask, to whichever page takes it first. */
export function takeConnectRequest(): boolean {
  const asked = connectAsked;
  connectAsked = false;
  return asked;
}

export function onConnectRequest(listener: () => void): () => void {
  connectBus.addEventListener("ask", listener);
  return () => connectBus.removeEventListener("ask", listener);
}
