import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createLibraryMock } from "../../../mock/library";
import { ValueForm } from "./ValueForm";
import { setLibraryApi } from "./api";

afterEach(() => { setLibraryApi(null); vi.restoreAllMocks(); });
const SET = "01je0000000000000000000001";
const SECRET = "hunter2-Zq9!";

const field = () => screen.getByLabelText(/Value for API_TOKEN/) as HTMLInputElement;
const mount = (done = vi.fn(), cancel = vi.fn(), planned = vi.fn()) => { render(<ValueForm setId={SET} keyName="API_TOKEN" replacing={false} onDone={done} onPlanned={planned} onCancel={cancel} />); return { done, cancel, planned }; };

it("is a password field that opts out of password managers, autofill and spellcheck, and has no name", () => {
  mount();
  const input = field();
  expect(input.type).toBe("password");
  expect(input).toHaveAttribute("autocomplete", "new-password");
  expect(input).toHaveAttribute("data-1p-ignore", "true");
  expect(input).toHaveAttribute("data-lpignore", "true");
  expect(input).toHaveAttribute("data-bwignore", "true");
  expect(input).toHaveAttribute("data-form-type", "other");
  expect(input).toHaveAttribute("spellcheck", "false");
  expect(input.hasAttribute("name")).toBe(false);
  expect(input.maxLength).toBe(8192);
});

it("is uncontrolled: the typed text is in the DOM only, never as a value attribute, and Save needs text", async () => {
  mount();
  const user = userEvent.setup();
  expect(screen.getByRole("button", { name: "Save value" })).toBeDisabled();
  await user.type(field(), SECRET);
  expect(field().value).toBe(SECRET);
  expect(field().getAttribute("value")).toBeNull();
  expect(screen.getByRole("button", { name: "Save value" })).toBeEnabled();
  expect(document.body.innerHTML).not.toContain(SECRET);
});

it("clears the field before the request leaves, sends the value once in the body, and echoes nothing", async () => {
  const mock = createLibraryMock({ delayMs: 30 });
  setLibraryApi(mock);
  const { done } = mount();
  const user = userEvent.setup();
  await user.type(field(), SECRET);
  await user.click(screen.getByRole("button", { name: "Save value" }));
  expect(field().value).toBe("");
  await waitFor(() => expect(done).toHaveBeenCalledTimes(1));
  expect(mock.writes).toEqual([`key:${SET}:API_TOKEN`]);
  expect(mock.vault.get(`${SET}/API_TOKEN`)).toBe(SECRET);
  expect(mock.lastKeyBody()).toBe(JSON.stringify({ value: SECRET }));
  expect(document.body.innerHTML).not.toContain(SECRET);
  expect(mock.writes.join()).not.toContain(SECRET);
});

it("does one write for a double click and a repeated Enter", async () => {
  const mock = createLibraryMock({ delayMs: 40 });
  setLibraryApi(mock);
  mount();
  const user = userEvent.setup();
  await user.type(field(), SECRET);
  const save = screen.getByRole("button", { name: "Save value" });
  fireEvent.click(save);
  fireEvent.click(save);
  fireEvent.submit(field().closest("form")!);
  await waitFor(() => expect(mock.writes).toHaveLength(1));
  await new Promise((resolve) => setTimeout(resolve, 80));
  expect(mock.writes).toHaveLength(1);
});

it("never writes the value to storage, the URL or the console", async () => {
  const mock = createLibraryMock();
  setLibraryApi(mock);
  const logs = [vi.spyOn(console, "log"), vi.spyOn(console, "warn"), vi.spyOn(console, "error"), vi.spyOn(console, "info")];
  const setItem = vi.spyOn(Storage.prototype, "setItem");
  const before = location.href;
  const { done } = mount();
  const user = userEvent.setup();
  await user.type(field(), SECRET);
  await user.click(screen.getByRole("button", { name: "Save value" }));
  await waitFor(() => expect(done).toHaveBeenCalled());
  expect(setItem).not.toHaveBeenCalled();
  expect(location.href).toBe(before);
  for (const log of logs) expect(JSON.stringify(log.mock.calls)).not.toContain(SECRET);
});

it("shows its own words for a known refusal, and clears the field so nothing stays on screen", async () => {
  const mock = createLibraryMock();
  setLibraryApi(mock);
  mock.failNext("keychain_blocked", `The prompt for ${SECRET} was cancelled`);
  const { done } = mount();
  const user = userEvent.setup();
  await user.type(field(), SECRET);
  await user.click(screen.getByRole("button", { name: "Save value" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("The keychain is locked or the prompt was cancelled.");
  expect(document.body.innerHTML).not.toContain(SECRET);
  expect(field().value).toBe("");
  expect(done).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Save value" })).toBeDisabled();
});

it("scrubs the exact and the encoded forms of the value from a service message it does not know", async () => {
  const mock = createLibraryMock();
  setLibraryApi(mock);
  const value = "pa ss/word+1";
  mock.failNext("weird_code", `bad ${value} and ${encodeURIComponent(value)} and ${btoa(value)} and ${JSON.stringify(value).slice(1, -1)}`);
  mount();
  const user = userEvent.setup();
  await user.type(field(), value);
  await user.click(screen.getByRole("button", { name: "Save value" }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("Couldn’t save the value.");
  for (const form of [value, encodeURIComponent(value), btoa(value)]) expect(alert.textContent).not.toContain(form);
  expect(alert.textContent).toContain("[hidden]");
});

it("shows a generic line for a failure that is not from the service", async () => {
  const mock = createLibraryMock();
  mock.setKey = async () => { throw new Error(`boom ${SECRET}`); };
  setLibraryApi(mock);
  mount();
  const user = userEvent.setup();
  await user.type(field(), SECRET);
  await user.click(screen.getByRole("button", { name: "Save value" }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("Something went wrong.");
  expect(alert.textContent).not.toContain(SECRET);
});

it("Cancel closes without a write", async () => {
  const mock = createLibraryMock();
  setLibraryApi(mock);
  const { cancel } = mount();
  await userEvent.setup().click(screen.getByRole("button", { name: "Cancel" }));
  expect(cancel).toHaveBeenCalled();
  expect(mock.writes).toEqual([]);
});

it("answers keychain_unavailable as the Planned state with the service's message, not as an error, and shows no value", async () => {
  const mock = createLibraryMock();
  setLibraryApi(mock);
  mock.failNext("keychain_unavailable", `There is no usable keychain on this computer yet, so values cannot be saved. ${SECRET}`);
  const { done, planned } = mount();
  const user = userEvent.setup();
  await user.type(field(), SECRET);
  await user.click(screen.getByRole("button", { name: "Save value" }));
  await waitFor(() => expect(planned).toHaveBeenCalledTimes(1));
  expect(planned.mock.calls[0]![0]).toContain("There is no usable keychain on this computer yet");
  expect(planned.mock.calls[0]![0]).not.toContain(SECRET);
  expect(done).not.toHaveBeenCalled();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(field().value).toBe("");
});
