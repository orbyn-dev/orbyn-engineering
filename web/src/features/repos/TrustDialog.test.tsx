import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createReposMock, repoFixtures } from "../../../mock/repos";
import { TRUST_DESKTOP_ONLY } from "../../product/ui/TrustPrompt";
import { setReposApi } from "./api";
import { TrustDialog } from "./Dialogs";

// The shared trust prompt, drawn as a dialog over a project's repositories. Trusting is a `Person` route, so this page's
// ability to send the person key decides whether the dialog offers the button at all.
let canTrust = true;
vi.mock("../../api/personKey", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../api/personKey")>()),
  canSendPersonKey: () => canTrust,
}));

const untrusted = () => repoFixtures().find((row) => row.name === "orbyn-engine")!;
const show = () => render(<TrustDialog projectId="p1" repo={untrusted()} onClose={() => undefined} onDone={() => undefined} />);

beforeEach(() => {
  canTrust = true;
  setReposApi(createReposMock());
});
afterEach(() => setReposApi(null));

it("offers Trust folder where the page can send the person key", () => {
  show();
  expect(screen.getByRole("dialog", { name: "Trust orbyn-engine?" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Trust folder" })).toBeVisible();
  expect(screen.queryByText(TRUST_DESKTOP_ONLY)).toBeNull();
});

it("in a browser tab says the desktop app is needed and offers no button the service would refuse", () => {
  canTrust = false;
  show();
  expect(screen.getByRole("dialog", { name: "Trust orbyn-engine?" })).toBeVisible();
  expect(screen.getByText(TRUST_DESKTOP_ONLY)).toBeVisible();
  expect(screen.queryByRole("button", { name: "Trust folder" })).toBeNull();
  // The path stays readable, and the way out stays: the dialog is not a dead end.
  expect(screen.getByText("~/Documents/Github/orbyn-engine")).toBeVisible();
  expect(screen.getByRole("button", { name: "Don’t trust" })).toBeVisible();
});
