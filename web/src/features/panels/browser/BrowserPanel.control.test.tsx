import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PreviewTab } from "../../../api/types";
import { BrowserPanel } from "./BrowserPanel";
import { useBrowserPanels } from "./browserPanels";

class FakeSocket {
  onmessage: ((event: { data: string }) => void) | null = null;
  close() {}
}

const tab = (control: PreviewTab["control"]): PreviewTab => ({
  id: "first", thread_id: "t", url: "https://example.com/", title: "first", control, recording: false, viewers: 0, width: 1280, height: 800,
  can_go_back: false, can_go_forward: false, zoom: 1, appearance: "system", mobile: false, private: false, profile: null, opened_by: "person", crashed: false,
});

beforeEach(() => {
  useBrowserPanels.setState({ moved: {} });
  vi.stubGlobal("WebSocket", FakeSocket);
});
afterEach(() => vi.unstubAllGlobals());

function serve(control: PreviewTab["control"], answer: (resolve: (response: Response) => void) => void) {
  const posts: unknown[] = [];
  vi.stubGlobal("fetch", (path: string, init?: RequestInit) => {
    if (path.endsWith("/control")) {
      posts.push(JSON.parse(String(init?.body)));
      return new Promise<Response>(answer);
    }
    const body = path === "/api/shell" ? { attached: false, windows: 1, facts: {} } : path.endsWith("/previews") ? [tab(control)] : {};
    return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
  });
  return posts;
}

it("shows a red line when taking control is refused, leaves the button unpressed and sends one request for a double press", async () => {
  const user = userEvent.setup();
  let refuse: () => void = () => undefined;
  const posts = serve("none", (resolve) => { refuse = () => resolve(new Response(JSON.stringify({ code: "unavailable", message: "The service did not answer." }), { status: 503 })); });
  render(<BrowserPanel threadId="t" />);
  const hand = await screen.findByRole("button", { name: "Take control" });
  await user.dblClick(hand);
  refuse();
  expect(await screen.findByRole("alert")).toHaveTextContent("Couldn’t take control. The service did not answer.");
  expect(posts).toEqual([{ human: true }]);
  expect(screen.getByRole("button", { name: "Take control" })).toHaveAttribute("aria-pressed", "false");
});

it("says Give control back, pressed, while the person holds the page, and sends the hand-back once", async () => {
  const user = userEvent.setup();
  let release: () => void = () => undefined;
  const posts = serve("human", (resolve) => { release = () => resolve(new Response("{}", { status: 200 })); });
  render(<BrowserPanel threadId="t" />);
  const hand = await screen.findByRole("button", { name: "Give control back" });
  expect(hand).toHaveAttribute("aria-pressed", "true");
  await user.dblClick(hand);
  release();
  await waitFor(() => expect(posts.length).toBeGreaterThan(0));
  expect(posts).toEqual([{ human: false }]);
});
