import { afterEach, expect, it, vi } from "vitest";

const shell = vi.hoisted(() => ({ attached: true }));
vi.mock("../../api/shell", () => ({ inDesktopApp: () => shell.attached }));

import { canPickFolder, FolderPickFailure, pickFolder } from "./folder";

const bridgeWith = (invoke?: (command: string) => unknown) => { (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = invoke ? { invoke } : undefined; };
afterEach(() => { bridgeWith(); shell.attached = true; });

it("cannot pick in a plain browser tab, or in a window with no bridge", () => {
  shell.attached = false;
  bridgeWith(() => "/x");
  expect(canPickFolder()).toBe(false);
  shell.attached = true;
  bridgeWith();
  expect(canPickFolder()).toBe(false);
  bridgeWith(() => "/x");
  expect(canPickFolder()).toBe(true);
});

it("asks the shell's one command and returns the path as text, or null when cancelled", async () => {
  const invoke = vi.fn(() => "/Users/me/orbyn-docs");
  bridgeWith(invoke);
  expect(await pickFolder()).toBe("/Users/me/orbyn-docs");
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(invoke).toHaveBeenCalledWith("pick_folder");
  bridgeWith(() => null);
  expect(await pickFolder()).toBeNull();
});

it("refuses an answer that is not a plain path, and a shell without the command", async () => {
  for (const bad of [42, "", "a\nb", "x".repeat(5000), { path: "/x" }]) {
    bridgeWith(() => bad);
    await expect(pickFolder()).rejects.toBeInstanceOf(FolderPickFailure);
  }
  bridgeWith(() => { throw "unknown command"; });
  await expect(pickFolder()).rejects.toThrow("Type the path instead");
  shell.attached = false;
  await expect(pickFolder()).rejects.toBeInstanceOf(FolderPickFailure);
});
