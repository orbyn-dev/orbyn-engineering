import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../api/client";
import { EarlierMessages } from "./EarlierMessages";
import { earlierApi, setEarlierApi, type EarlierApi, type EarlierPage } from "./earlierContract";

const page = (over: Partial<EarlierPage> = {}): EarlierPage => ({
  source: "Claude Code", total: 312, imported_at: "2026-11-21T10:00:00Z", next_before: "c1",
  messages: [{ role: "user", text: "The logout page loops." }, { role: "assistant", text: "Clear the cookie first." }], ...over,
});
const install = (read: EarlierApi["read"]) => setEarlierApi({ read });

afterEach(() => setEarlierApi(null));

it("sends nothing and draws nothing while no client is installed", () => {
  expect(earlierApi()).toBeNull();
  const { container } = render(<EarlierMessages threadId="t" />);
  expect(container).toBeEmptyDOMElement();
});

it("draws the earlier messages above a line saying where they came from, with no buttons on them", async () => {
  install(vi.fn().mockResolvedValue(page()));
  render(<EarlierMessages threadId="t" />);
  expect(await screen.findByText("The logout page loops.")).toBeInTheDocument();
  expect(screen.getByText("Clear the cookie first.")).toBeInTheDocument();
  expect(screen.getByText(/Claude Code · 312 earlier messages/)).toBeInTheDocument();
  expect(screen.getByText(/Imported 21 Nov · continues in Orbyn/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Show earlier" })).toBeInTheDocument();
});

it("shows file content as text, never as markup", async () => {
  install(vi.fn().mockResolvedValue(page({ messages: [{ role: "assistant", text: "<img src=x onerror=alert(1)><b>bold</b>" }] })));
  const { container } = render(<EarlierMessages threadId="t" />);
  await screen.findByText(/onerror/);
  expect(container.querySelector("img, b")).toBeNull();
});

it("loads more from the cursor and hides the button once the history ends", async () => {
  const read = vi.fn()
    .mockResolvedValueOnce(page())
    .mockResolvedValueOnce(page({ next_before: null, messages: [{ role: "user", text: "Older question." }] }));
  install(read);
  render(<EarlierMessages threadId="t" />);
  await userEvent.click(await screen.findByRole("button", { name: "Show earlier" }));
  expect(await screen.findByText("Older question.")).toBeInTheDocument();
  expect(read).toHaveBeenLastCalledWith("t", { before: "c1" }, expect.anything());
  expect(screen.queryByRole("button", { name: "Show earlier" })).toBeNull();
  // Older messages come before the ones already shown.
  const text = screen.getAllByTestId("earlier-message").map((node) => node.textContent);
  expect(text[0]).toBe("Older question.");
});

it("says once that the history can't be read and lets the person try again", async () => {
  const read = vi.fn()
    .mockRejectedValueOnce(new ApiFailure("history_unreadable", "gone", 422))
    .mockResolvedValueOnce(page());
  install(read);
  render(<EarlierMessages threadId="t" />);
  expect(await screen.findByText(/The earlier messages couldn’t be read/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Try again" }));
  expect(await screen.findByText("The logout page loops.")).toBeInTheDocument();
  expect(screen.queryByText(/couldn’t be read/)).toBeNull();
});

it("draws nothing for a thread with no earlier history, and for a failure that is not about the history", async () => {
  const read = vi.fn().mockResolvedValue(null);
  install(read);
  const { container, rerender } = render(<EarlierMessages threadId="t" />);
  await waitFor(() => expect(read).toHaveBeenCalled());
  expect(container).toBeEmptyDOMElement();
  install(vi.fn().mockRejectedValue(new ApiFailure("network_error", "x", 0)));
  rerender(<EarlierMessages threadId="u" />);
  await waitFor(() => expect(container).toBeEmptyDOMElement());
});
