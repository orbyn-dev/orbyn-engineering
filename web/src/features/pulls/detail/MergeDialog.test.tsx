import { act, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../api/client";
import { pullsApi } from "../../../api/pulls";
import type { PullRequestDetail } from "../../../api/types";
import { deleteBranchChoice, rememberDeleteBranch, resetMergeOptions, useMergeOptions } from "../model/mergeOptions";
import { MergeDialog, RowMergeDialog } from "./MergeDialog";

const detail = { pull: { number: 7, base: "main", title: "T" }, body: "Message", head_sha: "abc" } as unknown as PullRequestDetail;
const flush = () => act(async () => void (await Promise.resolve()));

beforeEach(() => {
  localStorage.clear();
  resetMergeOptions();
  vi.spyOn(pullsApi, "creditsSetting").mockResolvedValue(false);
  vi.spyOn(pullsApi, "mergeMethodSetting").mockResolvedValue(null);
  vi.spyOn(pullsApi, "detail").mockResolvedValue(detail);
  vi.spyOn(pullsApi, "merge").mockResolvedValue(undefined);
});
afterEach(() => vi.restoreAllMocks());

const open = async (props: Partial<Parameters<typeof RowMergeDialog>[0]> = {}) => {
  const onMerged = vi.fn();
  const onClose = vi.fn();
  const onStale = vi.fn();
  render(<RowMergeDialog projectId="p" number={7} onClose={onClose} onMerged={onMerged} onStale={onStale} {...props} />);
  await flush();
  return { onMerged, onClose, onStale };
};

describe("the merge dialog from a row", () => {
  it("shows its controls, starts with the delete box off, and does not merge until Merge is pressed", async () => {
    await open();
    expect(screen.getByRole("combobox", { name: "Merge method" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Merge when checks pass" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Delete the branch" })).not.toBeChecked();
    expect(screen.getByRole("switch", { name: "Remove agent credits" })).toHaveAccessibleDescription("Follows your account setting. Credit lines stay in this merge.");
    expect(pullsApi.merge).not.toHaveBeenCalled();
  });

  it("sends what the dialog shows, and remembers the delete choice", async () => {
    const { onMerged } = await open();
    fireEvent.change(screen.getByRole("combobox", { name: "Merge method" }), { target: { value: "rebase" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Delete the branch" }));
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await flush();
    expect(pullsApi.merge).toHaveBeenCalledWith("p", 7, { method: "rebase", auto: false, delete_branch: true, strip_credits: false, remove_agent_credits: false, expected_head: "abc" });
    expect(deleteBranchChoice("p")).toBe(true);
    expect(onMerged).toHaveBeenCalled();
  });

  it("a remembered delete shows checked, so it is never silent", async () => {
    rememberDeleteBranch("p", true);
    await open();
    expect(screen.getByRole("checkbox", { name: "Delete the branch" })).toBeChecked();
  });

  it("says it is loading, then offers Try again when the pull request cannot be read", async () => {
    vi.mocked(pullsApi.detail).mockRejectedValueOnce(new ApiFailure("bad_gateway", "no", 502));
    await open();
    expect(screen.getByText("Couldn’t load details.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Merge" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await flush();
    expect(screen.getByRole("button", { name: "Merge" })).toBeInTheDocument();
  });

  it("sends the head it showed, and never merges without it when it is known", async () => {
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await flush();
    expect(vi.mocked(pullsApi.merge).mock.calls[0]?.[2]).toMatchObject({ expected_head: "abc" });
  });

  it("when the head moved, shows the notice in place of the form, with Reload and no Merge", async () => {
    vi.mocked(pullsApi.merge).mockRejectedValueOnce(new ApiFailure("pull_head_changed", "This pull request changed.", 409));
    const { onStale, onClose } = await open();
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await flush();
    expect(screen.getByText("This pull request changed")).toBeInTheDocument();
    expect(screen.getByText("A new commit was added after you opened this page. Review it again before merging.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Merge" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Reload" }));
    expect(onStale).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("Cancel on the stale notice only closes it", async () => {
    vi.mocked(pullsApi.merge).mockRejectedValueOnce(new ApiFailure("pull_head_changed", "x", 409));
    const { onStale, onClose } = await open();
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
    expect(onStale).not.toHaveBeenCalled();
  });

  it("sends the head it was opened with, not one the page read since", async () => {
    const props = { projectId: "p", initialMethod: "squash", onClose: vi.fn(), onMerged: vi.fn(), onStale: vi.fn() };
    const { rerender } = render(<MergeDialog {...props} detail={detail} />);
    rerender(<MergeDialog {...props} detail={{ ...detail, head_sha: "newer" } as PullRequestDetail} />);
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await flush();
    expect(vi.mocked(pullsApi.merge).mock.calls[0]?.[2]).toMatchObject({ expected_head: "abc" });
  });

  it("puts focus on Reload when the notice replaces the form", async () => {
    vi.mocked(pullsApi.merge).mockRejectedValueOnce(new ApiFailure("pull_head_changed", "x", 409));
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await flush();
    expect(screen.getByRole("button", { name: "Reload" })).toHaveFocus();
  });
});

describe("the method the dialog starts on", () => {
  const select = () => screen.getByRole("combobox", { name: "Merge method" }) as HTMLSelectElement;

  it("keeps Squash and merge, with every method offered, when the project has no setting", async () => {
    await open();
    expect(select().value).toBe("squash");
    expect([...select().options].map((option) => [option.value, option.textContent])).toEqual([
      ["squash", "Squash and merge"],
      ["merge", "Create a merge commit"],
      ["rebase", "Rebase and merge"],
    ]);
    expect(screen.getByText("Squash and merge #7 into main.")).toBeInTheDocument();
    expect(pullsApi.mergeMethodSetting).toHaveBeenCalledWith("p", expect.anything());
  });

  it("starts on the global merge method from Settings", async () => {
    vi.mocked(pullsApi.mergeMethodSetting).mockResolvedValue("merge");
    await open();
    expect(select().value).toBe("merge");
    expect(screen.getByText("Create a merge commit #7 into main.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await flush();
    expect(vi.mocked(pullsApi.merge).mock.calls[0]?.[2]).toMatchObject({ method: "merge" });
  });

  it("starts on the repository's own override, which the project-scoped read resolves over the global", async () => {
    vi.mocked(pullsApi.mergeMethodSetting).mockImplementation((projectId) => Promise.resolve(projectId === "repo-b" ? "rebase" : "merge"));
    await open({ projectId: "repo-b" });
    expect(pullsApi.mergeMethodSetting).toHaveBeenCalledWith("repo-b", expect.anything());
    expect(select().value).toBe("rebase");
    expect(screen.getByText("Rebase and merge #7 into main.")).toBeInTheDocument();
  });
});

describe("Remove agent credits", () => {
  const credits = () => screen.getByRole("switch", { name: "Remove agent credits" });

  it("starts from the account setting: off keeps the credit lines", async () => {
    await open();
    expect(credits()).not.toBeChecked();
    expect(credits()).not.toHaveAttribute("aria-readonly");
    expect(credits()).not.toHaveAttribute("aria-disabled");
    expect(pullsApi.creditsSetting).toHaveBeenCalledWith("p", expect.anything());
  });

  it("shows the setting on, and says the credit lines are stripped, while untouched", async () => {
    vi.mocked(pullsApi.creditsSetting).mockResolvedValue(true);
    await open();
    expect(credits()).toBeChecked();
    expect(credits()).toHaveAccessibleDescription("Follows your account setting. Credit lines are stripped from this merge.");
  });

  it("with no setting to read, shows it off and says only that it follows the account", async () => {
    vi.mocked(pullsApi.creditsSetting).mockResolvedValue(null);
    await open();
    expect(credits()).not.toBeChecked();
    expect(credits()).toHaveAccessibleDescription("Follows your account setting.");
  });

  it("toggles, and the hint names the chosen value instead of the account setting", async () => {
    await open();
    fireEvent.click(credits());
    expect(credits()).toBeChecked();
    expect(credits()).toHaveAccessibleDescription("Credit lines are stripped from this merge.");
    fireEvent.click(credits());
    expect(credits()).not.toBeChecked();
    expect(credits()).toHaveAccessibleDescription("Credit lines stay in this merge.");
  });

  it("sends the resolved value: the account setting untouched, and the person's choice over it", async () => {
    vi.mocked(pullsApi.creditsSetting).mockResolvedValue(true);
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await flush();
    expect(vi.mocked(pullsApi.merge).mock.calls[0]?.[2]).toMatchObject({ strip_credits: false, remove_agent_credits: true });
  });

  it("sends false when the person turns the account's on setting off", async () => {
    vi.mocked(pullsApi.creditsSetting).mockResolvedValue(true);
    await open();
    fireEvent.click(credits());
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await flush();
    expect(vi.mocked(pullsApi.merge).mock.calls[0]?.[2]).toMatchObject({ remove_agent_credits: false });
  });

  it("sends nothing when the setting is unknown and the switch was never touched", async () => {
    vi.mocked(pullsApi.creditsSetting).mockResolvedValue(null);
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await flush();
    expect(vi.mocked(pullsApi.merge).mock.calls[0]?.[2]).not.toHaveProperty("remove_agent_credits");
  });
});

describe("the options shared with the merge menu", () => {
  it("starts from the menu's choices and the commit message is the description in four rows", async () => {
    const { result } = renderHook(() => useMergeOptions("p", 7));
    act(() => {
      result.current.setAuto(true);
      result.current.setDeleteBranch(true);
    });
    await open();
    expect(screen.getByRole("checkbox", { name: "Merge when checks pass" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Delete the branch" })).toBeChecked();
    expect(screen.getByRole("button", { name: "Enable auto-merge" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Commit message" })).toHaveValue("Message");
    expect(screen.getByRole("textbox", { name: "Commit message" })).toHaveAttribute("rows", "4");
  });

  it("ticking one in the dialog shows it ticked for the menu, and a merge sends it", async () => {
    const menu = renderHook(() => useMergeOptions("p", 7));
    await open();
    fireEvent.click(screen.getByRole("checkbox", { name: "Merge when checks pass" }));
    expect(menu.result.current.auto).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Enable auto-merge" }));
    await flush();
    expect(vi.mocked(pullsApi.merge).mock.calls[0]?.[2]).toMatchObject({ auto: true });
  });

  it("another pull request starts afresh", async () => {
    const { result } = renderHook(() => useMergeOptions("p", 8));
    act(() => result.current.setAuto(true));
    await open();
    expect(screen.getByRole("checkbox", { name: "Merge when checks pass" })).not.toBeChecked();
  });

  it("hides the message for a rebase, and the method select only where no menu chose the method", async () => {
    render(<MergeDialog projectId="p" detail={detail} initialMethod="rebase" fixedMethod onClose={vi.fn()} onMerged={vi.fn()} onStale={vi.fn()} />);
    expect(screen.queryByRole("textbox", { name: "Commit message" })).toBeNull();
    expect(screen.queryByRole("combobox", { name: "Merge method" })).toBeNull();
    expect(screen.getByText("Rebase and merge #7 into main.")).toBeInTheDocument();
  });
});
