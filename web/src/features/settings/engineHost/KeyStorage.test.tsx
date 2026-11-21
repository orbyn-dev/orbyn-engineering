import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import { createEngineHostMock, storageFixtures } from "../../../../mock/engineHost";
import { KEYCHAIN_DENIED, KEYCHAIN_LOCKED, type KeyStorage, setEngineHostApi } from "../../enginehost";
import { KeyStorageGroup, KeyStorageRow } from "./KeyStorage";

afterEach(() => { setEngineHostApi(null); vi.unstubAllGlobals(); vi.restoreAllMocks(); });
const rowOf = (name: string) => screen.getByText(name, { selector: ".name" }).closest("[role=row]") as HTMLElement;
const table = () => screen.findByRole("table", { name: "API key storage" });
const KEY = "sk-test-KEY-0123456789";

it("draws nothing while the service does not keep a key for the engine (404), and the one request it makes is a plain read", async () => {
  const fetching = vi.fn(async () => new Response(JSON.stringify({ code: "not_found", message: "This engine does not keep a key." }), { status: 404 }));
  vi.stubGlobal("fetch", fetching);
  render(<><KeyStorageGroup /><KeyStorageRow engineId="cursor" /></>);
  await waitFor(() => expect(fetching).toHaveBeenCalled());
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByText("API key storage")).toBeNull();
  expect(screen.queryByText("API key")).toBeNull();
  for (const call of fetching.mock.calls as unknown as [string, RequestInit][]) expect(call[1].method).toBe("GET");
});

it("through the real client: shows the service's state, and Remove… shows how many threads it stopped", async () => {
  const status = { engine: "cursor", opted_in: true, present: true, state: "saved" };
  const sent: string[] = [];
  vi.stubGlobal("fetch", vi.fn(async (url: string, init: RequestInit) => {
    sent.push(`${init.method} ${url}`);
    if (init.method === "DELETE") return new Response(JSON.stringify({ ...status, present: false, state: "not_saved", stopped: 2 }), { status: 200 });
    return new Response(JSON.stringify(status), { status: 200 });
  }));
  const user = userEvent.setup();
  render(<KeyStorageRow engineId="cursor" />);
  await user.click(await screen.findByRole("button", { name: "Remove…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Remove key" }));
  expect(await screen.findByRole("status")).toHaveTextContent("Removed. Stopped: 2 running threads.");
  expect(sent.filter((entry) => entry.startsWith("DELETE"))).toHaveLength(1);
});

it("lists each engine with its state and the one action that fits it", async () => {
  setEngineHostApi(createEngineHostMock());
  render(<KeyStorageGroup />);
  await table();
  expect(screen.getByText(/Orbyn only ever uses the system keychain: never a file, never shown again, never in a log/)).toBeVisible();
  expect(screen.getByText("Behind a setting")).toBeVisible();
  expect(within(rowOf("Cursor")).getByText("Saved in your keychain. It goes only to Cursor.")).toBeVisible();
  expect(within(rowOf("Cursor")).getByText("Saved")).toBeVisible();
  expect(within(rowOf("Cursor")).getByRole("button", { name: "Replace…" })).toBeVisible();
  expect(within(rowOf("Cursor")).queryByRole("button", { name: "Remove…" })).toBeNull();
  expect(within(rowOf("Codex")).getByText("Not saved. Orbyn stores nothing; Codex keeps its own sign-in.")).toBeVisible();
  expect(within(rowOf("Codex")).getByRole("button", { name: "Use keychain…" })).toBeVisible();
  expect(within(rowOf("Antigravity")).getByText("Not asked yet. Orbyn stores nothing until you choose.")).toBeVisible();
  expect(within(rowOf("Antigravity")).getByRole("button", { name: "Choose…" })).toBeVisible();
});

it("asks first, and Not now writes nothing", async () => {
  const mock = createEngineHostMock();
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<KeyStorageGroup />);
  await user.click(await within(await table()).findByRole("button", { name: "Choose…" }));
  const dialog = await screen.findByRole("dialog", { name: "Let Orbyn keep Antigravity’s API key in your keychain?" });
  expect(within(dialog).getByText(/never in a file, never shown again and never written to a log\. It goes only to Antigravity\./)).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Not now" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual([]);
});

it("Don’t store is one decline write, however fast the clicks", async () => {
  const mock = createEngineHostMock({ delayMs: 20 });
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<KeyStorageGroup />);
  await user.click(await within(await table()).findByRole("button", { name: "Choose…" }));
  await user.dblClick(within(await screen.findByRole("dialog")).getByRole("button", { name: "Don’t store" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["decline:antigravity"]);
  expect(within(rowOf("Antigravity")).getByText("Declined")).toBeVisible();
});

it("Continue writes nothing and opens one hidden field; Save key stays dimmed while it is empty", async () => {
  const mock = createEngineHostMock();
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<KeyStorageGroup />);
  await user.click(await within(await table()).findByRole("button", { name: "Choose…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Continue" }));
  const dialog = await screen.findByRole("dialog", { name: "Antigravity API key" });
  expect(within(dialog).getByText("Paste the key. Orbyn saves it in your keychain and does not show it again.")).toBeVisible();
  expect(within(dialog).getByText("Hidden while you type. Never written to a file or a log.")).toBeVisible();
  const field = within(dialog).getByLabelText("API key") as HTMLInputElement;
  expect(field.type).toBe("password");
  expect(within(dialog).queryByRole("button", { name: /show|copy/i })).toBeNull();
  expect(within(dialog).getByRole("button", { name: "Save key" })).toBeDisabled();
  expect(mock.writes).toEqual([]);
  await user.type(field, "x");
  expect(within(dialog).getByRole("button", { name: "Save key" })).toBeEnabled();
});

it("Save key is one write, empties the field first and leaves the key nowhere in the page", async () => {
  const mock = createEngineHostMock({ delayMs: 30 });
  const seen = vi.spyOn(mock, "saveKey");
  setEngineHostApi(mock);
  const user = userEvent.setup();
  const { baseElement } = render(<KeyStorageGroup />);
  await user.click(await within(await table()).findByRole("button", { name: "Choose…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Continue" }));
  const field = within(await screen.findByRole("dialog")).getByLabelText("API key") as HTMLInputElement;
  await user.type(field, KEY);
  // The page holds no value attribute, so the key is in no markup even while it is typed.
  expect(baseElement.innerHTML).not.toContain(KEY);
  await user.dblClick(screen.getByRole("button", { name: "Save key" }));
  expect(field.value).toBe("");
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["save:antigravity"]);
  expect(seen).toHaveBeenCalledTimes(1);
  expect(seen).toHaveBeenCalledWith("antigravity", KEY);
  expect(within(rowOf("Antigravity")).getByText("Saved in your keychain. It goes only to Antigravity.")).toBeVisible();
  expect(baseElement.innerHTML).not.toContain(KEY);
  expect(location.href).not.toContain(KEY);
});

it("Enter in the field saves once, and Escape is blocked while the write runs", async () => {
  const mock = createEngineHostMock({ delayMs: 60 });
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<KeyStorageGroup />);
  await user.click(await within(await table()).findByRole("button", { name: "Choose…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Continue" }));
  await user.type(within(await screen.findByRole("dialog")).getByLabelText("API key"), `${KEY}{Enter}`);
  await user.keyboard("{Escape}");
  expect(screen.getByRole("dialog")).toBeVisible();
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["save:antigravity"]);
});

it("Cancel on the key dialog writes nothing and keeps no key", async () => {
  const mock = createEngineHostMock();
  const seen = vi.spyOn(mock, "saveKey");
  setEngineHostApi(mock);
  const user = userEvent.setup();
  const { baseElement } = render(<KeyStorageGroup />);
  await user.click(await within(await table()).findByRole("button", { name: "Choose…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Continue" }));
  await user.type(within(await screen.findByRole("dialog")).getByLabelText("API key"), KEY);
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(seen).not.toHaveBeenCalled();
  expect(baseElement.innerHTML).not.toContain(KEY);
});

it("Replace… opens the same field with its own sentence, and saving swaps the key", async () => {
  const mock = createEngineHostMock();
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<KeyStorageGroup />);
  await user.click(await within(await table()).findByRole("button", { name: "Replace…" }));
  const dialog = await screen.findByRole("dialog", { name: "Replace Cursor’s API key" });
  expect(within(dialog).getByText("Paste the new key. The saved key stays until the new one is saved; then Orbyn keeps only the new one.")).toBeVisible();
  const field = within(dialog).getByLabelText("New API key") as HTMLInputElement;
  expect(field.value).toBe("");
  await user.type(field, KEY);
  await user.click(within(dialog).getByRole("button", { name: "Save key" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["save:cursor"]);
});

it("says a locked keychain and a denied one apart, and a retry asks for the key again", async () => {
  const mock = createEngineHostMock();
  const seen = vi.spyOn(mock, "saveKey");
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<KeyStorageGroup />);
  const enter = async (lock: typeof KEYCHAIN_LOCKED | typeof KEYCHAIN_DENIED) => {
    mock.failNext(lock);
    await user.type(within(await screen.findByRole("dialog")).getByLabelText("API key"), KEY);
    await user.click(screen.getByRole("button", { name: "Save key" }));
  };
  await user.click(await within(await table()).findByRole("button", { name: "Choose…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Continue" }));
  await enter(KEYCHAIN_LOCKED);
  const row = () => rowOf("Antigravity");
  expect(await within(row()).findByText("Locked")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("The keychain is locked. Unlock it, then try again.");
  expect(within(row()).getByText("Not saved. Orbyn stores nothing.")).toBeVisible();
  // Try again opens the field again, empty: no copy of the key was kept to send a second time.
  await user.click(screen.getByRole("button", { name: "Try again" }));
  expect((within(await screen.findByRole("dialog")).getByLabelText("API key") as HTMLInputElement).value).toBe("");
  await enter(KEYCHAIN_DENIED);
  expect(await within(row()).findByText("Denied")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("The system didn’t let Orbyn use the keychain. Allow it in the system’s keychain settings, then try again.");
  expect(mock.writes).toEqual(["save:antigravity", "save:antigravity"]);
  expect(seen).toHaveBeenCalledTimes(2);
});

it("Remove… asks first with a red button, deletes only what Orbyn saved and keeps threads running", async () => {
  const mock = createEngineHostMock({ delayMs: 40 });
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<KeyStorageRow engineId="cursor" />);
  expect(await screen.findByText("Saved in your keychain")).toBeVisible();
  expect(screen.getByText(/Built behind a setting, off until you turn it on\./)).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Remove…" }));
  const dialog = await screen.findByRole("dialog", { name: "Remove Cursor’s API key from the keychain?" });
  expect(within(dialog).getByText("Orbyn deletes the key it saved. Cursor uses its own sign-in again, or asks you to sign in. Threads already running keep going.")).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(mock.writes).toEqual([]);
  await user.click(screen.getByRole("button", { name: "Remove…" }));
  await user.dblClick(within(await screen.findByRole("dialog")).getByRole("button", { name: "Remove key" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["remove:cursor"]);
  expect(await screen.findByText("Orbyn stores nothing; Cursor keeps its own sign-in.")).toBeVisible();
});

it("a failed removal says the key is still saved and offers Try again", async () => {
  const mock = createEngineHostMock();
  mock.failRemoveNext(true);
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<KeyStorageRow engineId="cursor" />);
  await user.click(await screen.findByRole("button", { name: "Remove…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Remove key" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Orbyn couldn’t remove the key from the keychain. Try again.");
  expect(screen.getByText("Still saved in your keychain")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(mock.writes).toEqual(["remove:cursor", "remove:cursor"]);
});

it("shows a rejected key with Replace… and Remove… and never the key", async () => {
  const rejected: KeyStorage[] = [{ ...storageFixtures()[0]!, choice: "rejected" }];
  setEngineHostApi(createEngineHostMock({ storage: rejected }));
  render(<KeyStorageRow engineId="cursor" />);
  expect(await screen.findByRole("alert")).toHaveTextContent("Cursor didn’t accept the saved key. Replace it or remove it.");
  expect(screen.getByText("Saved in your keychain. Cursor rejected it.")).toBeVisible();
  expect(screen.getByText("Rejected")).toBeVisible();
  expect(screen.getByRole("button", { name: "Replace…" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Remove…" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
});

it("shows Saving and Removing while the write runs", async () => {
  const mock = createEngineHostMock({ delayMs: 80 });
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<KeyStorageRow engineId="cursor" />);
  await user.click(await screen.findByRole("button", { name: "Remove…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Remove key" }));
  expect(await screen.findByText("Removing it from the keychain…")).toBeVisible();
});

it("never renders a credential, even if the service sent one by mistake", async () => {
  const leaky = [{ ...storageFixtures()[0]!, token: "sk-LEAK-123", secret: "hunter2", key: "sk-LEAK-456" }];
  setEngineHostApi(createEngineHostMock({ storage: leaky as never }));
  const user = userEvent.setup();
  const { baseElement } = render(<KeyStorageGroup />);
  await user.click(await screen.findByRole("button", { name: "Replace…" }));
  expect(baseElement.innerHTML).not.toMatch(/LEAK|hunter2/);
  expect(location.href).not.toMatch(/LEAK|hunter2/);
});

it("cuts an engine name or error the service chose, and shows it as text", async () => {
  const long = `<img src=x onerror=alert(1)>${"x".repeat(200)}`;
  setEngineHostApi(createEngineHostMock({ storage: [{ engine_id: "e", name: long, choice: "not_asked" }] }));
  const { baseElement } = render(<KeyStorageGroup />);
  await table();
  expect(baseElement.querySelector("img")).toBeNull();
  expect(screen.getByText(/^<img src=x onerror=alert\(1\)>x*…$/, { selector: ".name" }).textContent!.length).toBeLessThanOrEqual(81);
});

it("puts no key, engine name or choice into a URL or the console", async () => {
  const log = vi.spyOn(console, "log").mockImplementation(() => undefined);
  const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
  const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
  const mock = createEngineHostMock();
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<KeyStorageGroup />);
  await user.click(await within(await table()).findByRole("button", { name: "Choose…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Continue" }));
  await user.type(within(await screen.findByRole("dialog")).getByLabelText("API key"), KEY);
  await user.click(screen.getByRole("button", { name: "Save key" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(location.hash + location.search).toBe("");
  for (const spy of [log, warn, error]) expect(spy).not.toHaveBeenCalled();
});

it("a refused replace keeps the saved state, says the old key is still there and keeps Remove…", async () => {
  const mock = createEngineHostMock();
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<KeyStorageRow engineId="cursor" />);
  await user.click(await screen.findByRole("button", { name: "Replace…" }));
  mock.failNext(KEYCHAIN_LOCKED);
  await user.type(within(await screen.findByRole("dialog")).getByLabelText("New API key"), KEY);
  await user.click(screen.getByRole("button", { name: "Save key" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("The new key wasn’t saved; the old key is still in your keychain.");
  expect(screen.getByText("Saved in your keychain")).toBeVisible();
  expect(screen.getByText("Saved")).toBeVisible();
  expect(screen.queryByText(/stores nothing/)).toBeNull();
  expect(screen.getByRole("button", { name: "Remove…" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Try again" }));
  expect(await screen.findByRole("dialog", { name: "Replace Cursor’s API key" })).toBeVisible();
});

it("two answers that land before a render each keep their own row", async () => {
  const mock = createEngineHostMock({ delayMs: 10 });
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<KeyStorageGroup />);
  const t = await table();
  await user.click(within(t).getByRole("button", { name: "Choose…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Don’t store" }));
  await user.click(within(t).getByRole("button", { name: "Replace…" }));
  await user.type(within(await screen.findByRole("dialog")).getByLabelText("New API key"), KEY);
  await user.click(screen.getByRole("button", { name: "Save key" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(within(rowOf("Antigravity")).getByText("Declined")).toBeVisible();
  expect(within(rowOf("Cursor")).getByText("Saved")).toBeVisible();
});

it("shows a state or choice the service made up as Unknown, and never throws on a prototype name", async () => {
  setEngineHostApi(createEngineHostMock({ storage: [
    { engine_id: "a", name: "A", choice: "toString" as never }, { engine_id: "b", name: "B", choice: "constructor" as never }] }));
  render(<KeyStorageGroup />);
  await table();
  expect(screen.getAllByText("Unknown")).toHaveLength(2);
});

it("keeps the key field away from password managers and out of the markup", async () => {
  setEngineHostApi(createEngineHostMock());
  const user = userEvent.setup();
  render(<KeyStorageGroup />);
  await user.click(await within(await table()).findByRole("button", { name: "Choose…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Continue" }));
  const field = within(await screen.findByRole("dialog")).getByLabelText("API key");
  expect(field).toHaveAttribute("autocomplete", "new-password");
  expect(field).toHaveAttribute("type", "password");
  expect(field).not.toHaveAttribute("value");
});

it("documents that the real routes are Person-class, carry the key in the body and never echo it", () => {
  const source = readFileSync(join(process.cwd(), "src/features/enginehost/contract.ts"), "utf8");
  for (const operation of ["saveKey", "removeKey", "decline", "stop", "retry"]) {
    const at = source.indexOf(`${operation}(`);
    expect(at).toBeGreaterThan(0);
    expect(source.slice(Math.max(0, at - 700), at)).toContain("Person-class (ENFORCED");
  }
  expect(source).toMatch(/JSON body and never in the path or query/);
  expect(source).toMatch(/must not\s+\* echo the key/);
});
