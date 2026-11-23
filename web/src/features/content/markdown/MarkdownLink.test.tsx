import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { threads } from "../../../../mock/fixtures";
import type { MockApi } from "../../../../mock/api";
import { installMockFetch } from "../../../test-support";
import { wire } from "../../../wire.test-support";
import { useWorkspace } from "../../../product/state/workspace";
import { pullChecks, pullPreviewPlacement, pullReview, pullState } from "./PullLinkPreview";
import { pullNumberOf, pullRepositoryLabel, samePullRequest } from "./pullLink";
import { Markdown } from "./Markdown";
import { previewProjectId } from "./MarkdownLink";

// Pull request 203 of the mock service's "pulls" scenario: opened by riley, checks passing, review required.
const servedHref = "https://github.com/orbyn-dev/orbyn-engineering/pull/203";
const href = "https://github.com/acme/orbyn/pull/42";
let mock: MockApi;
const pullReads = () => wire().filter((call) => /\/pulls\//.test(call.path));

const hover = async (link: HTMLElement, ms = 350) => {
  fireEvent.pointerEnter(link, { pointerType: "mouse" });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
};

beforeEach(() => {
  vi.useFakeTimers();
  mock = installMockFetch();
  mock.reset("pulls");
  useWorkspace.setState({
    threads: Object.fromEntries(threads().map((thread) => [thread.id, thread])),
  });
  window.location.hash = "#/threads/t-approval";
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("pull link addresses", () => {
  it("reads the number, repository and host from a host's address", () => {
    expect(pullNumberOf(href)).toBe(42);
    expect(pullNumberOf("https://github.com/o/r/pull/42/files#x")).toBe(42);
    expect(pullNumberOf("https://example.com/docs")).toBeNull();
    expect(pullRepositoryLabel(href)).toBe("acme/orbyn");
    expect(pullRepositoryLabel("not a url")).toBeNull();
  });

  it("matches only the same pull request in the same repository", () => {
    expect(samePullRequest("https://github.com/Acme/Orbyn/pull/42/files", href)).toBe(true);
    expect(samePullRequest("https://github.com/acme/other/pull/42", href)).toBe(false);
    expect(samePullRequest("https://github.com/acme/orbyn/pull/43", href)).toBe(false);
    expect(samePullRequest("https://example.com/docs", href)).toBe(false);
  });
});

describe("pull link preview", () => {
  it("shows the title, state, author and checks after a short hover", async () => {
    render(<Markdown text={`See [the fix](${servedHref}).`} projectId="p-app" />);
    const link = screen.getByRole("link", { name: "the fix" });
    fireEvent.pointerEnter(link, { pointerType: "mouse" });
    act(() => {
      vi.advanceTimersByTime(349);
    });
    expect(pullReads()).toEqual([]);
    expect(screen.queryByRole("tooltip")).toBeNull();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(pullReads()).toEqual([{ method: "GET", path: "/api/projects/p-app/pulls/203" }]);
    const card = screen.getByRole("tooltip");
    expect(card).toHaveTextContent("Return to the page people came from");
    expect(card).toHaveTextContent("orbyn-dev/orbyn-engineering#203 · Open");
    expect(card).toHaveTextContent("riley");
    expect(card).toHaveTextContent("Checks passing");
    expect(card).toHaveTextContent("Awaiting review");
  });

  it("uses the current thread's project when the prose has no project", async () => {
    render(<Markdown text={`See [the fix](${servedHref}).`} />);
    await hover(screen.getByRole("link", { name: "the fix" }));
    expect(pullReads()).toEqual([{ method: "GET", path: "/api/projects/p-app/pulls/203" }]);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Return to the page people came from");
  });

  it("leaves before the delay without a request or a card", async () => {
    const link = (() => {
      render(<Markdown text={`See [the fix](${servedHref}).`} projectId="p-app" />);
      return screen.getByRole("link", { name: "the fix" });
    })();
    fireEvent.pointerEnter(link, { pointerType: "mouse" });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(100);
    });
    fireEvent.pointerLeave(link);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(500);
    });
    expect(pullReads()).toEqual([]);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("stays quiet for other links, touch pointers and routes without a project", async () => {
    render(<Markdown text={"[docs](https://example.com/docs)"} projectId="p-app" />);
    await hover(screen.getByRole("link", { name: "docs" }));
    expect(pullReads()).toEqual([]);

    const pr = (() => {
      render(<Markdown text={`[the fix](${servedHref})`} projectId="p-app" />);
      return screen.getByRole("link", { name: "the fix" });
    })();
    fireEvent.pointerEnter(pr, { pointerType: "touch" });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(500);
    });
    expect(pullReads()).toEqual([]);

    window.location.hash = "#/pulls";
    render(<Markdown text={`[other](${servedHref})`} />);
    await hover(screen.getByRole("link", { name: "other" }));
    expect(pullReads()).toEqual([]);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("hides the card when the project answers with another pull request", async () => {
    // The service answers pull request 203 of orbyn-dev/orbyn-engineering, not this repository's.
    render(<Markdown text="[the fix](https://github.com/acme/other/pull/203)" projectId="p-app" />);
    await hover(screen.getByRole("link", { name: "the fix" }));
    expect(pullReads()).toHaveLength(1);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("hides the card when the pull request cannot be read", async () => {
    render(<Markdown text="[the fix](https://github.com/orbyn-dev/orbyn-engineering/pull/999)" projectId="p-app" />);
    await hover(screen.getByRole("link", { name: "the fix" }));
    expect(pullReads()).toEqual([{ method: "GET", path: "/api/projects/p-app/pulls/999" }]);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});

describe("pull link presentation", () => {
  it("names draft, merged, closed and open states", () => {
    expect(pullState({ state: "open", draft: true }).label).toBe("Draft");
    expect(pullState({ state: "merged", draft: false }).label).toBe("Merged");
    expect(pullState({ state: "closed", draft: false }).label).toBe("Closed");
    expect(pullState({ state: "open", draft: false }).label).toBe("Open");
  });

  it("names checks and review outcomes, and omits the absent ones", () => {
    expect(pullChecks("pass")?.label).toBe("Checks passing");
    expect(pullChecks("fail")?.label).toBe("Checks failing");
    expect(pullChecks("pending")?.label).toBe("Checks running");
    expect(pullChecks("none")).toBeNull();
    expect(pullReview("approved")?.label).toBe("Approved");
    expect(pullReview("changes_requested")?.label).toBe("Changes requested");
    expect(pullReview("review_required")?.label).toBe("Awaiting review");
    expect(pullReview(null)).toBeNull();
  });

  it("places the card below a link, above one near the bottom, and inside the viewport", () => {
    const viewport = { width: 1024, height: 768 };
    expect(pullPreviewPlacement({ left: 100, top: 100, bottom: 120 }, viewport)).toEqual({
      left: 100,
      top: 126,
      above: false,
    });
    expect(pullPreviewPlacement({ left: 1000, top: 700, bottom: 720 }, viewport)).toEqual({
      left: 696,
      top: 694,
      above: true,
    });
  });

  it("reads the project from a thread route only", () => {
    expect(previewProjectId("#/threads/t-approval")).toBe("p-app");
    expect(previewProjectId("#/pulls")).toBeNull();
    expect(previewProjectId("#/threads/missing")).toBeNull();
  });
});
