import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { dayOf } from "../autopilot/approvals/ledger";
import { LessonAddPage } from "./LessonAddPage";
import { LessonCardPage } from "./LessonCardPage";
import { LiveLessons } from "./LiveLessons";

let mock: MockApi;

beforeEach(() => {
  window.location.hash = "";
  useWorkspace.setState(useWorkspace.getInitialState(), true);
  mock = installMockFetch();
});

const calls = () => vi.mocked(fetch).mock.calls;
const requests = (method: string, path: string) =>
  calls().filter(([url, init]) => (init?.method ?? "GET") === method && String(url) === path);

/** Holds one matching request on a promise this test releases, then hands it to the mock's own answer. */
function hold(method: string, path: string): () => void {
  const real = vi.mocked(fetch).getMockImplementation()!;
  let release: () => void = () => undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  vi.mocked(fetch).mockImplementation(async (input, init) => {
    if ((init?.method ?? "GET") === method && String(input) === path) await gate;
    return real(input, init);
  });
  return release;
}

describe("the repository switch while its write runs", () => {
  it("reads Saving…, then Off, when turning lessons off is answered", async () => {
    mock.reset("crew-lessons");
    const user = userEvent.setup();
    render(<LiveLessons view="list" initial="p-app" />);
    const control = await screen.findByRole("switch", { name: "Use lessons in orbyn-engineering-app" });
    const toggle = control.closest("label")!;
    expect(toggle).toHaveTextContent("On");

    const release = hold("PUT", "/api/crew/lessons/settings");
    await user.click(control);
    await waitFor(() => expect(requests("PUT", "/api/crew/lessons/settings")).toHaveLength(1));
    await waitFor(() => expect(toggle).toHaveTextContent("Saving…"));
    expect(control).toBeDisabled();

    await act(async () => release());
    await waitFor(() => expect(toggle).toHaveTextContent("Off"));
    expect(control).not.toBeChecked();
  });

  it("reads Saving…, then On, when turning lessons on is answered", async () => {
    mock.reset("crew-lessons-off");
    const user = userEvent.setup();
    render(<LiveLessons view="list" initial="p-app" />);
    const control = await screen.findByRole("switch", { name: "Use lessons in orbyn-engineering-app" });
    const toggle = control.closest("label")!;
    expect(toggle).toHaveTextContent("Off");

    const release = hold("PUT", "/api/crew/lessons/settings");
    await user.click(control);
    await waitFor(() => expect(requests("PUT", "/api/crew/lessons/settings")).toHaveLength(1));
    await waitFor(() => expect(toggle).toHaveTextContent("Saving…"));
    expect(control).toBeDisabled();

    await act(async () => release());
    await waitFor(() => expect(toggle).toHaveTextContent("On"));
    expect(control).toBeChecked();
  });
});

describe("the Add page while its save runs", () => {
  it("reads Saving…, says Saving to this device. and reads the field only, after the untouched hint", async () => {
    mock.reset("crew-lessons");
    const user = userEvent.setup();
    render(<LessonAddPage repo="p-app" />);
    const field = await screen.findByRole("textbox", { name: "Lesson text" });
    expect(screen.getByText("Nothing is saved until you press Save.")).toBeInTheDocument();
    expect((field as HTMLTextAreaElement).readOnly).toBe(false);
    expect(screen.getByRole("button", { name: "Save" })).toBeInTheDocument();

    await user.type(field, "A sentence to keep.");
    const release = hold("POST", "/api/crew/lessons");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(requests("POST", "/api/crew/lessons")).toHaveLength(1));

    expect(await screen.findByRole("button", { name: "Saving…" })).toBeDisabled();
    expect(screen.getByText("Saving to this device.")).toBeInTheDocument();
    expect(screen.queryByText("Nothing is saved until you press Save.")).toBeNull();
    expect((field as HTMLTextAreaElement).readOnly).toBe(true);

    await act(async () => release());
    await waitFor(() => expect(window.location.hash).toBe("#/settings/crew/lessons/lesson?repo=p-app&id=les-added-1"));
  });
});

describe("a lesson's edit save while it runs", () => {
  it("reads the field only and sends nothing on a second Save, while the button says Saving…", async () => {
    mock.reset("crew-lessons");
    const user = userEvent.setup();
    render(<LessonCardPage id="les-returnto" repo="p-app" />);
    await user.click(await screen.findByRole("button", { name: "Edit" }));
    const field = await screen.findByRole("textbox", { name: "Lesson text" });
    await user.clear(field);
    await user.type(field, "Check the fixture path before changing the test.");

    const release = hold("PATCH", "/api/crew/lessons/les-returnto");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(requests("PATCH", "/api/crew/lessons/les-returnto")).toHaveLength(1));

    const saving = await screen.findByRole("button", { name: "Saving…" });
    expect((field as HTMLTextAreaElement).readOnly).toBe(true);
    expect(screen.getByText("Saving to this device.")).toBeInTheDocument();

    // The button is off while the save runs, so a second click cannot send (the guard inside send is only a backstop).
    expect(saving).toBeDisabled();
    await user.click(saving);
    expect(requests("PATCH", "/api/crew/lessons/les-returnto")).toHaveLength(1);

    await act(async () => release());
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
    expect(requests("PATCH", "/api/crew/lessons/les-returnto")).toHaveLength(1);
  });
});

describe("the states that need a write to fail", () => {
  it("draws the scan's refusal of an add, in the contract's own sentence, and keeps the text", async () => {
    mock.reset("crew-lessons-add-secret");
    const user = userEvent.setup();
    render(<LessonAddPage repo="p-app" />);
    const field = await screen.findByRole("textbox", { name: "Lesson text" });
    await user.type(field, "The token is sk-live-xxxx.");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Not saved: this looks like it holds a secret. Edit it and save again.",
    );
    expect(field).toHaveValue("The token is sk-live-xxxx.");
    expect(window.location.hash).toBe("");
  });

  it("says the service could not save an add, and offers Try again", async () => {
    mock.reset("crew-lessons-add-fails");
    const user = userEvent.setup();
    render(<LessonAddPage repo="p-app" />);
    const field = await screen.findByRole("textbox", { name: "Lesson text" });
    await user.type(field, "A sentence to keep.");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByText("Can’t save the lesson")).toBeInTheDocument();
    expect(screen.getByText("The service could not save the lesson. Try again.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
    expect(window.location.hash).toBe("");
  });

  it("says the service could not save an edit, and keeps the edit form", async () => {
    mock.reset("crew-lessons-save-fails");
    const user = userEvent.setup();
    render(<LessonCardPage id="les-returnto" repo="p-app" />);
    await user.click(await screen.findByRole("button", { name: "Edit" }));
    const field = await screen.findByRole("textbox", { name: "Lesson text" });
    await user.clear(field);
    await user.type(field, "Check the fixture path before changing the test.");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByText("Can’t save the lesson")).toBeInTheDocument();
    expect(screen.getByText("The service could not save the lesson. Try again.")).toBeInTheDocument();
    expect(field).toHaveValue("Check the fixture path before changing the test.");
  });

  it("says the service could not delete the lesson, and keeps the card", async () => {
    mock.reset("crew-lessons-delete-fails");
    const user = userEvent.setup();
    render(<LessonCardPage id="les-returnto" repo="p-app" />);
    await user.click(await screen.findByRole("button", { name: "Delete" }));

    expect(await screen.findByText("Can’t delete the lesson")).toBeInTheDocument();
    expect(screen.getByText("The service could not delete the lesson. Try again.")).toBeInTheDocument();
    // The delete failed, so the card stays with the lesson and no Undo window was opened.
    expect(screen.getByRole("button", { name: "Delete" })).toBeInTheDocument();
  });
});

describe("the Add page when the projects read fails", () => {
  it("says Can’t read the projects, and its Retry reads the projects again", async () => {
    mock.reset("crew-lessons");
    mock.fail("/api/projects", 1, 503, "unavailable", "The service is unavailable.");
    const user = userEvent.setup();
    render(<LessonAddPage repo="p-app" />);

    expect(await screen.findByText("Can’t read the projects")).toBeInTheDocument();
    expect(screen.getByText("The service is unavailable.")).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Lesson text" })).toBeNull();

    // The shared ErrorState names its action Retry.
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("heading", { name: /^Crew › Add a lesson/ })).toBeInTheDocument();
    expect(requests("GET", "/api/projects")).toHaveLength(2);
  });
});

describe("a lesson row's learned day", () => {
  it("says Learned with the lesson's created_at day", async () => {
    mock.reset("crew-lessons");
    render(<LiveLessons view="list" initial="p-app" />);
    const text = await screen.findByText(
      "A refreshed session must keep a person signed in: check the session route after the refresh.",
    );
    const row = text.closest(".cx-lsrow") as HTMLElement;
    // The row's day comes from created_at, not from the lesson's later updated_at.
    expect(within(row).getByText(`Learned ${dayOf("2026-11-05T09:40:00Z")}`)).toBeInTheDocument();
  });
});
