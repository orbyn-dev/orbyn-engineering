import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import { installMockFetch } from "../../test-support";
import { wire } from "../../wire.test-support";
import { holdEach, type HeldRequest } from "../../wire.held.test-support";
import { FolderBrowser } from "./FolderBrowser";

const HOME = "C:\\Users\\me";
let mock: MockApi;
/**
 * The service is made slow: it answers each folder the dialog asks for only when the test lets it, so a test can look at what
 * the dialog shows while a folder is still on its way. Each answer is the folder as `mock.folders` holds it when it was asked.
 */
let held: HeldRequest[];
const asked = () => wire().filter((call) => call.path.startsWith("/api/fs/browse")).map((call) => decodeURIComponent(call.path.replace(/^\/api\/fs\/browse\??(path=)?/, "")) || null);

beforeEach(() => {
  mock = installMockFetch();
  held = holdEach("GET", /^\/api\/fs\/browse/);
});
afterEach(() => vi.unstubAllGlobals());

/** Lets the folder asked for at this place in the order answer: the dialog only ever has one read in flight. */
const answer = async (at: number) => {
  await waitFor(() => expect(held.length).toBeGreaterThan(at));
  await act(async () => held[at]!.release());
};

const open = async () => {
  const view = render(<FolderBrowser actionLabel="Add this folder" onChoose={() => {}} />);
  return { ...view, box: view.container.querySelector(".browser-body") as HTMLElement | null };
};

it("keeps one folder box while a folder loads, when it is empty and when it has rows", async () => {
  mock.folders.set(HOME, { [HOME]: ["Documents"], [`${HOME}\\Documents`]: [] });
  const { container, box } = await open();
  expect(box).not.toBeNull();

  await answer(0);
  const rows = await screen.findByRole("button", { name: /Documents/ });
  // The same box, still one node: its size cannot change with the state, so the centred dialog cannot move.
  expect(container.querySelector(".browser-body")).toBe(box);
  expect(within(box as HTMLElement).getByRole("button", { name: /Documents/ })).toBe(rows);

  const user = userEvent.setup();
  await user.click(rows);
  await answer(1);
  expect(await within(box as HTMLElement).findByText("No folders here")).toBeVisible();
  expect(container.querySelector(".browser-body")).toBe(box);
});

it("says No folders here for a Windows folder with no subfolders", async () => {
  mock.folders.set(HOME, { [HOME]: ["All Users"], [`${HOME}\\All Users`]: [] });
  const { box } = await open();
  await answer(0);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: /All Users/ }));
  await answer(1);
  expect(await within(box as HTMLElement).findByText("No folders here")).toBeVisible();
});

it("shows the folder the service named, with the service's separators", async () => {
  // A bare ask is answered with the folder the service starts in.
  mock.folders.set(`${HOME}\\Documents`, { [`${HOME}\\Documents`]: ["notes"] });
  await open();
  await answer(0);
  await waitFor(() => expect(document.querySelector(".browser-path")).toHaveTextContent("C:\\Users\\me\\Documents"));
});

it("goes up two levels when Up is clicked twice before the first folder answers", async () => {
  mock.folders.set(HOME, { [HOME]: ["Documents"] });
  await open();
  await answer(0);
  const user = userEvent.setup();
  const up = screen.getByRole("button", { name: "Up" });
  await user.click(up);
  // The next folder is still on its way: Up still names the folder above the one shown.
  expect(up).toBeEnabled();
  await user.click(up);
  expect(asked().slice(1)).toEqual(["C:\\Users", "C:\\"]);
});
