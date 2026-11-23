import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspace } from "../../../product/state/workspace";
import { LiveLessons } from "./LiveLessons";
import { bodyOf, lesson, list, project, stubFetch, writes, type Answer, type Call } from "./support";

let calls: Call[] = [];
let answers: { preview?: Answer; projects?: Answer; list?: Answer; hidden?: Answer; settings?: Answer };

beforeEach(() => {
  calls = [];
  answers = {
    preview: { status: 200, body: { enabled: true } },
    projects: { status: 200, body: [project] },
    list: { status: 200, body: list() },
  };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
});

const intro =
  "What Crew has learned on this device. A lesson comes from a failure that repeated, an escape or a revert you confirmed, or something you typed as a correction, or you add one by hand. You can read, edit, turn off, promote, move back and delete each one.";

describe("the lessons list on the real route", () => {
  it("draws nothing and reads no lessons route while Crew Preview is off", async () => {
    answers.preview = { status: 404, body: { code: "not_found", message: "Not found" } };
    const { container } = render(<LiveLessons view="list" />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/preview"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(calls.some(({ url }) => url.includes("/api/crew/lessons"))).toBe(false);
  });

  it("draws nothing at all when the lessons routes answer 404 not_found", async () => {
    answers.list = { status: 404, body: { code: "not_found", message: "Not found" } };
    const { container } = render(<LiveLessons view="list" />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/lessons?"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(screen.queryByText("Nothing learned yet")).toBeNull();
    expect(screen.queryByText("Can’t read the lessons")).toBeNull();
  });

  it("groups the list by scope with its counts, and carries its intro and banner words", async () => {
    render(<LiveLessons view="list" initial="p-app" />);
    expect(await screen.findByRole("heading", { name: /^This repository · orbyn-engineering/ })).toBeInTheDocument();
    expect(screen.getByText("All repositories", { selector: ".cx-apchip" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /^All repositories/ })).toBeInTheDocument();
    expect(screen.getByText("1 of 50")).toBeInTheDocument();
    expect(screen.getByText("1 of 20")).toBeInTheDocument();
    expect(screen.getByText(intro)).toBeInTheDocument();
    expect(screen.getByText("Lessons are data, not rules.")).toBeInTheDocument();
    expect(
      screen.getByText(/A worker gets at most 5 of them as text added to its brief after the route is chosen\./),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Crew is built so the router, the score, the verifier, the answer key, a lease, a budget and the checks never read one\./),
    ).toBeInTheDocument();
    // The source word, what each lesson reached, and its scope chip are the read's own facts.
    expect(screen.getByText("Repeated failure")).toBeInTheDocument();
    expect(screen.getByText("Revert")).toBeInTheDocument();
    expect(screen.getByText(/Used in 3 briefs · last /)).toBeInTheDocument();
    expect(screen.getByText("Not used yet")).toBeInTheDocument();
    expect(screen.getByText("This repository", { selector: ".cx-apchip" })).toBeInTheDocument();
    expect(screen.getByText("All repositories", { selector: ".cx-apchip" })).toBeInTheDocument();
  });

  it("keeps the head and the banner while loading, and draws no changing control", async () => {
    answers.list = "never";
    render(<LiveLessons view="list" />);
    expect(await screen.findByText("Lessons are data, not rules.")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Reading lessons");
    expect(screen.queryByRole("switch")).toBeNull();
    expect(screen.queryByRole("button", { name: "Add a lesson" })).toBeNull();
  });

  it("reads under the Hidden filter and says why a lesson is hidden", async () => {
    answers.list = { status: 200, body: list({ counts: { active: 2, disabled: 0, hidden: 1 } }) };
    answers.hidden = {
      status: 200,
      body: list({
        counts: { active: 1, disabled: 0, hidden: 1 },
        lessons: [lesson({ id: "les-hidden", text: "A hidden lesson", hidden_by_cap: true, in_next_brief: false })],
      }),
    };
    const user = userEvent.setup();
    render(<LiveLessons view="list" />);
    await user.click(await screen.findByRole("button", { name: "Hidden (over 50) · 1" }));
    await waitFor(() =>
      expect(calls.some(({ url }) => url.includes("/api/crew/lessons?") && url.includes("filter=hidden"))).toBe(true),
    );
    expect(await screen.findByText("A hidden lesson")).toBeInTheDocument();
    expect(screen.getByText("Hidden: over the limit, never used.")).toBeInTheDocument();
  });

  it("sends the switch's PUT once per own click, with the repository and the chosen value", async () => {
    answers.settings = { status: 200, body: { repo: "p-app", use_lessons: false } };
    const user = userEvent.setup();
    render(<LiveLessons view="list" />);
    const control = await screen.findByRole("switch", { name: "Use lessons in orbyn-engineering" });
    expect(control).toBeChecked();
    await user.click(control);
    await waitFor(() => expect(writes(calls, "PUT")).toHaveLength(1));
    expect(writes(calls, "PUT")[0]?.url).toBe("/api/crew/lessons/settings");
    expect(bodyOf(writes(calls, "PUT")[0]?.init)).toEqual({ repo: "p-app", use_lessons: false });
    await waitFor(() => expect(control).not.toBeChecked());
    await user.click(control);
    await waitFor(() => expect(writes(calls, "PUT")).toHaveLength(2));
    expect(bodyOf(writes(calls, "PUT")[1]?.init)).toEqual({ repo: "p-app", use_lessons: true });
  });

  it("keeps the old value when the switch is refused, and says why", async () => {
    answers.settings = { status: 500, body: { code: "internal", message: "The switch could not be saved." } };
    const user = userEvent.setup();
    render(<LiveLessons view="list" />);
    const control = await screen.findByRole("switch", { name: "Use lessons in orbyn-engineering" });
    await user.click(control);
    expect(await screen.findByText("Can’t save the setting: The switch could not be saved.")).toBeInTheDocument();
    expect(control).toBeChecked();
    expect(writes(calls, "PUT")).toHaveLength(1);
  });

  it("keeps every row readable with the switch off", async () => {
    answers.list = { status: 200, body: list({ use_lessons: false, counts: { active: 2, disabled: 1, hidden: 0 }, lessons: [lesson({ state: "disabled", text: "Off but readable" })] }) };
    render(<LiveLessons view="list" />);
    expect(await screen.findByRole("switch", { name: "Use lessons in orbyn-engineering" })).not.toBeChecked();
    expect(screen.getByText("Off but readable")).toBeInTheDocument();
    expect(screen.getByText("Off", { selector: ".cx-apchip" })).toBeInTheDocument();
  });

  it("says what an empty list means, and offers the Add a lesson button", async () => {
    answers.list = { status: 200, body: list({ counts: { active: 0, disabled: 0, hidden: 0 }, lessons: [] }) };
    render(<LiveLessons view="list" />);
    expect(await screen.findByText("Nothing learned yet")).toBeInTheDocument();
    expect(
      screen.getByText(/Nothing has been learned on this device yet\. A lesson is written by a failure that repeats/),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add a lesson" })).toBeInTheDocument();
  });

  it("says No hidden lessons when the Hidden filter is empty", async () => {
    answers.list = { status: 200, body: list({ counts: { active: 0, disabled: 0, hidden: 0 }, lessons: [] }) };
    const user = userEvent.setup();
    render(<LiveLessons view="list" />);
    await user.click(await screen.findByRole("button", { name: "Hidden (over 50) · 0" }));
    expect(await screen.findByText("No hidden lessons")).toBeInTheDocument();
  });

  it("fails in one red banner that changes nothing, and Try again reads the list again", async () => {
    answers.list = { status: 500, body: { code: "unreadable", message: "The lessons on this device can’t be read." } };
    const user = userEvent.setup();
    render(<LiveLessons view="list" />);
    expect(await screen.findByText("Can’t read the lessons")).toBeInTheDocument();
    expect(screen.getByText(/Nothing was changed\./)).toBeInTheDocument();
    expect(screen.getByText(/unreadable: The lessons on this device can’t be read\./)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy the error" })).toBeInTheDocument();
    answers.list = { status: 200, body: list() };
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("heading", { name: /^This repository · orbyn-engineering/ })).toBeInTheDocument();
    expect(calls.filter(({ url }) => url.includes("/api/crew/lessons?"))).toHaveLength(2);
  });

  it("names a row and opens its card with the id and the repository in the address", async () => {
    const user = userEvent.setup();
    render(<LiveLessons view="list" />);
    await screen.findByRole("heading", { name: /^This repository · orbyn-engineering/ });
    await user.click(screen.getAllByRole("button", { name: "Open" })[0]!);
    expect(window.location.hash).toBe("#/settings/crew/lessons/lesson?repo=p-app&id=les-1");
  });
});
