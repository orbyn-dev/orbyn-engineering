// The address, zoom, device and recent-page rules of the Browser panel.

/** A page on this machine: it loads with no network, and needs nothing from the Orbyn service. */
export function isLocalUrl(url: string): boolean {
  try {
    const host = new URL(url).hostname.toLowerCase();
    return host === "localhost" || host.endsWith(".localhost") || host === "127.0.0.1" || host === "[::1]";
  } catch {
    return false;
  }
}

export const q = encodeURIComponent;
export const VIEWPORT = { width: 1280, height: 800 };

const ENGINES: Record<string, string> = {
  duckduckgo: "https://duckduckgo.com/?q=",
  google: "https://www.google.com/search?q=",
  bing: "https://www.bing.com/search?q=",
  brave: "https://search.brave.com/search?q=",
  kagi: "https://kagi.com/search?q=",
  startpage: "https://www.startpage.com/sp/search?query=",
};

/** A URL, a local address, or words for the chosen search engine. */
export function resolveAddress(input: string, engine: string): string {
  const text = input.trim();
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(text)) return text;
  if (/^(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$)/i.test(text)) return `http://${text}`;
  if (/^[^\s/]+\.[a-z]{2,}(\/|$)/i.test(text)) return `https://${text}`;
  return `${ENGINES[engine] ?? ENGINES.duckduckgo}${q(text)}`;
}

export const RESIZE_DELAY_MS = 300;
/** Pixels of change, both ways together, worth asking the browser to lay out again. */
export const RESIZE_SLACK = 8;
export const TAB_POLL_MS = 1000;

const RECENT_LIMIT = 50;
const ZOOM_STEPS = [0.5, 0.75, 1, 1.25, 1.5, 2];

/** Sizes a page can take. `null` follows the panel. */
export const DEVICES: { id: string; label: string; size: { width: number; height: number } | null; phone: boolean }[] = [
  { id: "fit", label: "Fit panel", size: null, phone: false },
  { id: "phone", label: "Phone, 393 by 852", size: { width: 393, height: 852 }, phone: true },
  { id: "large-phone", label: "Large phone, 430 by 932", size: { width: 430, height: 932 }, phone: true },
  { id: "tablet", label: "Tablet, 820 by 1180", size: { width: 820, height: 1180 }, phone: true },
  { id: "laptop", label: "Laptop, 1280 by 800", size: { width: 1280, height: 800 }, phone: false },
  { id: "desktop", label: "Desktop, 1920 by 1080", size: { width: 1920, height: 1080 }, phone: false },
];

/** The next stop up or down from a zoom, which may sit between stops. */
export function stepZoom(zoom: number, up: boolean): number {
  const next = up ? ZOOM_STEPS.find((stop) => stop > zoom) : [...ZOOM_STEPS].reverse().find((stop) => stop < zoom);
  return next ?? zoom;
}

/** Newest first, each once. */
export function addRecent(recent: string[], url: string): string[] {
  return [url, ...recent.filter((entry) => entry !== url)].slice(0, RECENT_LIMIT);
}
