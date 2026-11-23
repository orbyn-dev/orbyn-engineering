import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { App } from "../../../app/App";
import { useUi } from "../../../product/state/ui";
import { installMockFetch } from "../../../test-support";

let mock: MockApi;

beforeEach(() => {
  mock = installMockFetch();
  useUi.setState({ projectFolders: true });
  useUi.getState().setPrefs({ welcomeDone: true });
});

/** Add a project → Clone a repository, the way the issue's steps do. */
async function clone(url: string) {
  render(<App />);
  const user = userEvent.setup();
  const nav = await screen.findByRole("navigation", { name: "Workspace" });
  await user.click(await within(nav).findByRole("button", { name: "Add project" }));
  const palette = await screen.findByRole("dialog", { name: "Command palette" });
  await user.click(within(palette).getByRole("button", { name: /Clone a repository/ }));
  const dialog = await screen.findByRole("dialog", { name: "Clone a repository" });
  await user.type(within(dialog).getByLabelText("Git URL or owner/name"), `${url}{Enter}`);
  return user;
}

/** The failed clone's toast, once it is on screen. */
const failure = async () => {
  const line = await screen.findByText("Clone failed");
  const toast = line.closest(".toast");
  expect(toast).not.toBeNull();
  return toast as HTMLElement;
};

const dismiss = async (user: Awaited<ReturnType<typeof clone>>, toast: HTMLElement) => {
  await user.click(within(toast).getByRole("button", { name: "Dismiss" }));
  await waitFor(() => expect(screen.queryByText("Clone failed")).toBeNull());
};

describe("a clone that fails", () => {
  it("is announced: the toast renders inside the toast stack's live region", async () => {
    mock.failClone();
    const user = await clone("https://example.com/team/repo.git");
    const toast = await failure();
    expect(toast).toHaveTextContent("fatal: repository 'bad url with spaces' does not exist");
    // The app's one announcement primitive for toasts: the stack is a polite status region, so the
    // failure is read out instead of sitting in a silent group.
    const region = toast.closest('[role="status"]');
    expect(region).not.toBeNull();
    expect(region).toHaveClass("toasts");
    expect(region).toHaveAttribute("aria-live", "polite");
    // The stack exists before the failure, so the inserted toast is what gets announced.
    expect(region).toBe(document.querySelector(".toasts"));
    await dismiss(user, toast);
  });

  it("keeps the toast in the toast layer, out of the sidebar that holds the Engines chip", async () => {
    mock.failClone();
    const user = await clone("https://example.com/team/repo.git");
    const toast = await failure();
    // Structural stand-in for a bounding box: jsdom lays nothing out, so assert the layers differ.
    // The toast is in the fixed stack (anchored beside or away from the sidebar, see
    // styles/toastLayer.test.ts); the Engines chip is the sidebar footer's update pill.
    const chip = await screen.findByRole("button", { name: /Updates available/ });
    expect(chip).toHaveTextContent("Engines 1");
    expect(chip.closest(".sidebar")).not.toBeNull();
    expect(chip.closest(".toasts")).toBeNull();
    expect(toast.closest(".toasts")).not.toBeNull();
    expect(toast.closest(".sidebar")).toBeNull();
    expect(toast.contains(chip) || chip.contains(toast)).toBe(false);
    await dismiss(user, toast);
  });
});

it("keeps a confirmation in the polite stack: no error mark and no alert", async () => {
  render(<App />);
  await screen.findByRole("navigation", { name: "Workspace" });
  act(() => useUi.getState().pushToast("Download started", "done"));
  const confirmation = await screen.findByText("Download started");
  const toast = confirmation.closest(".toast") as HTMLElement;
  const region = toast.closest('[role="status"]');
  expect(region).toHaveClass("toasts");
  // A success is not the error announcement: it carries no failure mark and interrupts nothing.
  expect(toast.querySelector(".toast-failed")).toBeNull();
  expect(toast.closest('[role="alert"]')).toBeNull();
  expect(toast.closest('[aria-live="assertive"]')).toBeNull();
});
