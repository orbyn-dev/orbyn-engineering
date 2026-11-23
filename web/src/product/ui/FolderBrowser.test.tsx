import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import type { FsListing } from "../../api/types";

/**
 * The browse asks the service one folder at a time and is answered by hand here, so a
 * test can look at what the dialog shows while a folder is still on its way.
 */
const asks = vi.hoisted(() => ({ pending: [] as { path: string | null; answer(listing: FsListing): void }[] }));

vi.mock("../../api/client", async (original) => {
  const actual = await original<typeof import("../../api/client")>();
  return {
    ...actual,
    api: {
      ...actual.api,
      browse: (path: string | null) =>
        new Promise<FsListing>((resolve) => asks.pending.push({ path, answer: resolve })),
    },
  };
});

import { FolderBrowser } from "./FolderBrowser";

afterEach(() => {
  asks.pending.length = 0;
});

const HOME = "C:\\Users\\me";
const listing = (path: string, parent: string | null, names: string[] = []): FsListing => ({
  path,
  parent,
  entries: names.map((name) => ({ name, path: `${path}\\${name}`.replace(/\\\\/g, "\\"), is_git: false })),
});

/** Answers the open ask for a folder: the dialog only ever has one read in flight. */
const answer = async (path: string | null, value: FsListing) => {
  const ask = asks.pending.find((open) => open.path === path);
  if (!ask) throw new Error(`no open ask for ${String(path)}`);
  asks.pending.splice(asks.pending.indexOf(ask), 1);
  await act(async () => ask.answer(value));
};

const open = async () => {
  const view = render(<FolderBrowser actionLabel="Add this folder" onChoose={() => {}} />);
  return { ...view, box: view.container.querySelector(".browser-body") as HTMLElement | null };
};

it("keeps one folder box while a folder loads, when it is empty and when it has rows", async () => {
  const { container, box } = await open();
  expect(box).not.toBeNull();

  await answer(null, listing(HOME, "C:\\Users", ["Documents"]));
  const rows = await screen.findByRole("button", { name: /Documents/ });
  // The same box, still one node: its size cannot change with the state, so the centred dialog cannot move.
  expect(container.querySelector(".browser-body")).toBe(box);
  expect(within(box as HTMLElement).getByRole("button", { name: /Documents/ })).toBe(rows);

  const user = userEvent.setup();
  await user.click(rows);
  await answer(`${HOME}\\Documents`, listing(`${HOME}\\Documents`, HOME, []));
  expect(await within(box as HTMLElement).findByText("No folders here")).toBeVisible();
  expect(container.querySelector(".browser-body")).toBe(box);
});

it("says No folders here for a Windows folder with no subfolders", async () => {
  const { box } = await open();
  await answer(null, listing(HOME, null, ["All Users"]));
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: /All Users/ }));
  await answer(`${HOME}\\All Users`, listing(`${HOME}\\All Users`, HOME, []));
  expect(await within(box as HTMLElement).findByText("No folders here")).toBeVisible();
});

it("shows the folder the service named, with the service's separators", async () => {
  await open();
  await answer(null, listing(`${HOME}\\Documents`, HOME, ["notes"]));
  await waitFor(() => expect(document.querySelector(".browser-path")).toHaveTextContent("C:\\Users\\me\\Documents"));
});

it("goes up two levels when Up is clicked twice before the first folder answers", async () => {
  await open();
  await answer(null, listing(HOME, "C:\\Users", ["Documents"]));
  const user = userEvent.setup();
  const up = screen.getByRole("button", { name: "Up" });
  await user.click(up);
  // The next folder is still on its way: Up still names the folder above the one shown.
  expect(up).toBeEnabled();
  await user.click(up);
  expect(asks.pending.map((ask) => ask.path)).toEqual(["C:\\Users", "C:\\"]);
});
