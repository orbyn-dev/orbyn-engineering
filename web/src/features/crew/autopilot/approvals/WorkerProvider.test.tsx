// Settings › Crew › Worker provider (Crew Designs v75, frames `settings-crew-worker-provider`, `-any`, `-narrow`).
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { Project } from "../../../../api/types";
import { useWorkspace } from "../../../../product/state/workspace";
import { installMockFetch } from "../../../../test-support";
import { CrewSettingsSection } from "./CrewSettings";
import { WorkerProvider } from "./WorkerProvider";

let mock: MockApi;
const SET = "/api/crew/projects/p-app/worker-provider/set";
const writes = () => vi.mocked(fetch).mock.calls.filter(([url, init]) => String(url).includes("/worker-provider/set") && init?.method === "POST");
const choice = (name: string) => screen.getByRole("radio", { name });

beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
  useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-app" } as Project], error: null, loading: false } });
});

describe("Settings › Crew › Worker provider", () => {
  it("shows Same as lead selected by default, with the design's help text", async () => {
    mock.reset("crew");
    render(<WorkerProvider />);
    const group = await screen.findByRole("radiogroup", { name: "Worker provider" });
    expect(group).toBeVisible();
    expect(choice("Same as lead")).toHaveAttribute("aria-checked", "true");
    expect(choice("Any provider")).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText("Same as lead keeps workers and scouts on the lead’s provider. Any provider lets Crew pick among the engines that can run workers. Review lanes always use another provider; this row does not change that.")).toBeVisible();
    expect(screen.queryByText("Planned")).toBeNull();
  });

  it("shows Any provider selected when that is what the project stored", async () => {
    mock.reset("crew-worker-provider-any");
    render(<WorkerProvider />);
    await screen.findByRole("radiogroup", { name: "Worker provider" });
    expect(choice("Any provider")).toHaveAttribute("aria-checked", "true");
    expect(choice("Same as lead")).toHaveAttribute("aria-checked", "false");
  });

  it("saves the choice with the person key and shows the stored answer", async () => {
    mock.reset("crew");
    render(<WorkerProvider />);
    await screen.findByRole("radiogroup", { name: "Worker provider" });
    await userEvent.click(choice("Any provider"));
    await waitFor(() => expect(choice("Any provider")).toHaveAttribute("aria-checked", "true"));
    expect(writes()).toHaveLength(1);
    const [url, init] = writes()[0]!;
    expect(String(url)).toContain(SET);
    expect(JSON.parse(String(init?.body))).toEqual({ worker_provider: "any" });
    expect(new Headers(init?.headers).get("x-orbyn-person")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("holds the old choice and ignores a second click while the write is out", async () => {
    mock.reset("crew");
    render(<WorkerProvider />);
    await screen.findByRole("radiogroup", { name: "Worker provider" });
    const real = vi.mocked(fetch).getMockImplementation()!;
    let release: () => void = () => undefined;
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (String(input).includes("/worker-provider/set")) await new Promise<void>((resolve) => { release = resolve; });
      return real(input, init);
    });
    await userEvent.click(choice("Any provider"));
    await waitFor(() => expect(screen.getByRole("radiogroup", { name: "Worker provider" }).parentElement).toHaveAttribute("aria-busy", "true"));
    expect(choice("Same as lead")).toHaveAttribute("aria-checked", "true");
    await userEvent.click(choice("Same as lead"));
    await userEvent.click(choice("Any provider"));
    expect(writes()).toHaveLength(1);
    release();
    await waitFor(() => expect(choice("Any provider")).toHaveAttribute("aria-checked", "true"));
    expect(screen.getByRole("radiogroup", { name: "Worker provider" }).parentElement).toHaveAttribute("aria-busy", "false");
  });

  it("says plainly that a write failed and leaves the old choice shown", async () => {
    mock.reset("crew-worker-provider-fail");
    render(<WorkerProvider />);
    await screen.findByRole("radiogroup", { name: "Worker provider" });
    await userEvent.click(choice("Any provider"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not save the worker provider.");
    expect(choice("Same as lead")).toHaveAttribute("aria-checked", "true");
    expect(choice("Any provider")).toHaveAttribute("aria-checked", "false");
    expect(screen.getByRole("radiogroup", { name: "Worker provider" }).parentElement).toHaveAttribute("aria-busy", "false");
  });

  it("draws nothing while there is no project", async () => {
    mock.reset("crew");
    useWorkspace.setState({ projects: { data: [], error: null, loading: false } });
    render(<WorkerProvider />);
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(screen.queryByRole("radiogroup", { name: "Worker provider" })).toBeNull();
    expect(vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes("worker-provider"))).toHaveLength(0);
  });

  it("is a row of Settings › Crew, before Engine tiers", async () => {
    mock.reset("crew");
    render(<CrewSettingsSection />);
    const group = await screen.findByRole("radiogroup", { name: "Worker provider" });
    const tiers = await screen.findByRole("region", { name: "Engine tiers" });
    expect(group.compareDocumentPosition(tiers) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
