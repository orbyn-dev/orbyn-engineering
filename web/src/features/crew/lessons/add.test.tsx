import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspace } from "../../../product/state/workspace";
import { LessonAddPage } from "./LessonAddPage";
import { lesson, project, stubFetch, writes, type Answer, type Call, type Write } from "./support";

let calls: Call[] = [];
let answers: { preview?: Answer; projects?: Answer; write?: (one: Write) => Answer };

const notFound = { status: 404, body: { code: "not_found", message: "No such route." } } as const;

beforeEach(() => {
  calls = [];
  answers = {
    preview: { status: 200, body: { enabled: true } },
    projects: { status: 200, body: [project] },
    write: () => ({ status: 201, body: lesson({ id: "les-new", source: "person", text: "A new sentence.", rev: 1 }) }),
  };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
});

const open = () => render(<LessonAddPage repo="p-app" />);
const field = () => screen.findByRole("textbox", { name: "Lesson text" });

describe("adding a lesson by hand", () => {
  it("draws nothing and reads no lessons route while Crew Preview is off", async () => {
    answers.preview = { status: 404, body: notFound.body };
    const { container } = open();
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/preview"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(calls.some(({ url }) => url.includes("/api/crew/lessons"))).toBe(false);
  });

  it("draws the form, its scope choice and its 300-character count, and sends nothing yet", async () => {
    open();
    expect(await screen.findByRole("heading", { name: /^Crew › Add a lesson/ })).toBeInTheDocument();
    expect(
      screen.getByText(/Write a lesson in your own words, up to 300 characters\./),
    ).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "This repository" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "All repositories" })).not.toBeChecked();
    expect(
      screen.getByText(
        "This repository is orbyn-engineering, and its lesson may name a path. All repositories names path classes only, never a path.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("0 of 300 characters")).toBeInTheDocument();
    expect(screen.getByText("Nothing is saved until you press Save.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    expect(writes(calls, "POST")).toHaveLength(0);
  });

  it("adds a lesson for this repository and opens its card", async () => {
    const user = userEvent.setup();
    open();
    await field();
    await user.type(await field(), "A new sentence.");
    expect(screen.getByText("15 of 300 characters")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(window.location.hash).toBe("#/settings/crew/lessons/lesson?repo=p-app&id=les-new"));
    expect(writes(calls, "POST")).toHaveLength(1);
    expect(writes(calls, "POST")[0]?.url).toBe("/api/crew/lessons");
    expect(JSON.parse(String(writes(calls, "POST")[0]?.init?.body))).toEqual({
      scope: "repo",
      repo: "p-app",
      text: "A new sentence.",
    });
  });

  it("adds one for all repositories without naming a repository", async () => {
    const user = userEvent.setup();
    open();
    await user.type(await field(), "A new sentence.");
    await user.click(screen.getByRole("radio", { name: "All repositories" }));
    expect(screen.getByRole("radio", { name: "All repositories" })).toBeChecked();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(JSON.parse(String(writes(calls, "POST")[0]?.init?.body))).toEqual({ scope: "global", text: "A new sentence." }));
  });

  it("keeps the text and says the scan refused it, staying on the form", async () => {
    answers.write = () => ({
      status: 400,
      body: { code: "secret_refused", message: "Not saved: this looks like it holds a secret. Edit it and save again." },
    });
    const user = userEvent.setup();
    open();
    await user.type(await field(), "The token is sk-live-xxxx.");
    await user.click(screen.getByRole("button", { name: "Save" }));
    const refusal = await screen.findByRole("alert");
    expect(refusal).toHaveTextContent("Not saved: this looks like it holds a secret. Edit it and save again.");
    expect(screen.getByRole("textbox", { name: "Lesson text" })).toHaveValue("The token is sk-live-xxxx.");
    expect(window.location.hash).toBe("");
  });

  it("keeps the text when the service refused the write, and Try again sends it again", async () => {
    answers.write = () => ({ status: 403, body: { code: "desktop_required", message: "Only a person can add a lesson." } });
    const user = userEvent.setup();
    open();
    await user.type(await field(), "A new sentence.");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Can’t save the lesson")).toBeInTheDocument();
    expect(screen.getByText("Only a person can add a lesson.")).toBeInTheDocument();
    answers.write = () => ({ status: 201, body: lesson({ id: "les-new" }) });
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(writes(calls, "POST")).toHaveLength(2));
  });

  it("draws nothing for a repository the route names but the project list does not show", async () => {
    const { container } = render(<LessonAddPage repo="p-nowhere" />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/preview"))).toBe(true));
    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByRole("textbox", { name: "Lesson text" })).toBeNull();
  });

  it("uses the first project when the route names no repository", async () => {
    render(<LessonAddPage />);
    expect(
      await screen.findByText(/This repository is orbyn-engineering, and its lesson may name a path\./),
    ).toBeInTheDocument();
  });

  it("goes back to the list without sending anything", async () => {
    const user = userEvent.setup();
    open();
    await field();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(window.location.hash).toBe("#/settings/crew/lessons?repo=p-app");
    expect(writes(calls, "POST")).toHaveLength(0);
  });
});
