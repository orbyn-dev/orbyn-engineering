import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import { ApiFailure } from "../../api/client";
import { FolderBrowser, parentOfPath } from "./FolderBrowser";

afterEach(() => vi.restoreAllMocks());

it("finds the folder above a path from its text", () => {
  expect(parentOfPath("/Users/me/code")).toBe("/Users/me");
  expect(parentOfPath("/Users/me/code/")).toBe("/Users/me");
  expect(parentOfPath("/Users")).toBe("/");
  expect(parentOfPath("/")).toBeNull();
  expect(parentOfPath("C:\\Users\\me")).toBe("C:\\Users");
  expect(parentOfPath("C:\\Users")).toBe("C:\\");
  expect(parentOfPath("C:\\")).toBeNull();
});

it("goes Up from the error a failed read leaves", async () => {
  const browse = vi.spyOn(api, "browse").mockImplementation(async (path) => {
    if (path === null) return { path: "/home", parent: "/", entries: [{ name: "locked", path: "/home/locked", is_git: false }] } as never;
    if (path === "/home/locked") throw new ApiFailure("forbidden", "No access.", 403);
    return { path, parent: "/", entries: [] } as never;
  });
  render(<FolderBrowser actionLabel="Open" onChoose={() => {}} />);
  await userEvent.click(await screen.findByRole("button", { name: /locked/ }));
  expect(await screen.findByText("Couldn't read this folder")).toBeTruthy();
  const up = screen.getByRole("button", { name: "Up" });
  expect((up as HTMLButtonElement).disabled).toBe(false);
  await userEvent.click(up);
  await waitFor(() => expect(browse).toHaveBeenLastCalledWith("/home", expect.anything()));
  expect(await screen.findByText("No folders here")).toBeTruthy();
});
