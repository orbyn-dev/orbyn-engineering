import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { CrewSettingsSection } from "../autopilot/approvals/CrewSettings";
import { LessonsEntry } from "./LessonsEntry";
import { list, project, stubFetch, type Answer, type Call } from "./support";

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

describe("the lessons row of Settings › Crew", () => {
  it("draws nothing and reads no lessons route while Crew Preview is off", async () => {
    answers.preview = { status: 404, body: { code: "not_found", message: "Not found" } };
    const { container } = render(<LessonsEntry />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/preview"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(calls.some(({ url }) => url.includes("/api/crew/lessons"))).toBe(false);
  });

  it("draws no row when the one probe read answers 404 not_found", async () => {
    answers.list = { status: 404, body: { code: "not_found", message: "Not found" } };
    const { container } = render(<LessonsEntry />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/lessons?"))).toBe(true));
    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByRole("button", { name: "Open Lessons" })).toBeNull();
  });

  it("draws no row while the probe read runs", async () => {
    answers.list = "never";
    const { container } = render(<LessonsEntry />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/lessons?"))).toBe(true));
    expect(container).toBeEmptyDOMElement();
  });

  it("draws no row when the probe read fails", async () => {
    answers.list = { status: 500, body: { code: "unreadable", message: "The lessons on this device can’t be read." } };
    const { container } = render(<LessonsEntry />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/lessons?"))).toBe(true));
    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByRole("button", { name: "Open Lessons" })).toBeNull();
  });

  it("is drawn after one answered probe, with its words, and opening it only navigates", async () => {
    const user = userEvent.setup();
    render(<LessonsEntry />);
    const open = await screen.findByRole("button", { name: "Open Lessons" });
    expect(screen.getByText("Lessons")).toBeInTheDocument();
    expect(
      screen.getByText(
        /What Crew has learned on this device from a failure that repeated, an escape or a revert you confirmed, and your corrections\./,
      ),
    ).toBeInTheDocument();
    expect(calls.filter(({ url }) => url.includes("/api/crew/lessons?"))).toHaveLength(1);
    await user.click(open);
    expect(window.location.hash).toBe("#/settings/crew/lessons");
  });
});

describe("Settings › Crew", () => {
  it("puts the Lessons row after Escapes", async () => {
    const mock: MockApi = installMockFetch();
    mock.reset("crew-lessons");
    render(<CrewSettingsSection />);
    const escapes = await screen.findByRole("button", { name: "Open Escapes" });
    const lessons = await screen.findByRole("button", { name: "Open Lessons" });
    expect(escapes.compareDocumentPosition(lessons) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
