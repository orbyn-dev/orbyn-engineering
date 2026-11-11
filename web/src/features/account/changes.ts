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
