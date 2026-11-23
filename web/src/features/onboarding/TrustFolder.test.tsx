import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Project } from "../../generated/Project";
import { useWorkspace } from "../../product/state/workspace";
import { TrustFolder } from "./TrustFolder";

// Trusting is a `Person` route, so only a page that holds the person key may ask. The key comes from the desktop shell's
// bridge, or from the dev preview's fixed key (api/personKey.ts); this decides which of those pages the test draws.
let canTrust = true;
vi.mock("../../api/personKey", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../api/personKey")>()),
  canSendPersonKey: () => canTrust,
}));

const folder: Project = {
  id: "p-new",
  name: "vendor-sdk-with-a-rather-long-repository-name-for-layout",
  path: "/Users/dev/code/clients/acme/vendor-sdk-with-a-rather-long-repository-name-for-layout",
  trusted: false,
  is_git: false,
  created_at: "2026-10-31T09:00:00Z",
  managed: false,
  scratch: false,
};

beforeEach(() => {
  canTrust = true;
  useWorkspace.setState({ projects: { data: [folder], error: null, loading: false } });
});
afterEach(() => {
  vi.unstubAllGlobals();
  window.location.hash = "";
});

it("offers Trust folder where the page can send the person key, and trusts the one folder", async () => {
  const sent: string[] = [];
  vi.stubGlobal("fetch", async (url: string, init?: RequestInit) => {
    sent.push(`${init?.method ?? "GET"} ${String(url).replace(/^.*\/api/, "/api")}`);
    return new Response(JSON.stringify({ ...folder, trusted: true }), { status: 200, headers: { "content-type": "application/json" } });
  });
  render(<TrustFolder projectId={folder.id} threadId={null} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Trust folder" }));
  await waitFor(() => expect(sent).toEqual(["POST /api/projects/p-new/trust"]));
});

it("in a browser tab says the desktop app is needed, offers no Trust folder, keeps the way out, and sends nothing", async () => {
  canTrust = false;
  const sent: string[] = [];
  vi.stubGlobal("fetch", async (url: string, init?: RequestInit) => {
    sent.push(`${init?.method ?? "GET"} ${String(url)}`);
    return new Response("{}", { status: 200, headers: { "content-type": "application/json" } });
  });
  render(<TrustFolder projectId={folder.id} threadId={null} />);

  // The path is still the person's to read, and the folder stays untrusted.
  expect(await screen.findByText("Path")).toBeVisible();
  expect(screen.getByText("~/code/clients/acme/vendor-sdk-with-a-rather-long-repository-name-for-layout")).toBeVisible();
  // No control that the service would refuse with `403 desktop_required`.
  expect(screen.queryByRole("button", { name: "Trust folder" })).toBeNull();
  // What it says instead, before the person can click anything.
  expect(screen.getByText("Trusting a folder needs the Orbyn desktop app.")).toBeVisible();
  // The way out works here, so the page is not a dead end.
  expect(screen.getByRole("link", { name: "Don’t trust" })).toBeVisible();
  expect(sent).toEqual([]);
});
