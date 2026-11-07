import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure } from "../../api/client";
import { pullsApi } from "../../api/pulls";
import type { PullRequestDetail } from "../../api/types";
import { RowMergeDialog, deleteBranchChoice, rememberDeleteBranch } from "./MergeDialog";

const detail = { pull: { number: 7, base: "main", title: "T" }, body: "Message", head_sha: "abc" } as unknown as PullRequestDetail;
const flush = () => act(async () => void (await Promise.resolve()));

beforeEach(() => {
  localStorage.clear();
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

describe("delete branch after merge", () => {
  it("is off until chosen and then remembered for that project only", () => {
    expect(deleteBranchChoice("p")).toBe(false);
    rememberDeleteBranch("p", true);
    expect(deleteBranchChoice("p")).toBe(true);
    expect(deleteBranchChoice("q")).toBe(false);
  });
});

describe("the merge dialog from a row", () => {
  it("shows its controls, starts with the delete box off, and does not merge until Merge is pressed", async () => {
    await open();
    expect(screen.getByRole("combobox", { name: "Merge method" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Merge when checks pass" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Delete the branch" })).not.toBeChecked();
    expect(screen.getByRole("switch", { name: "Remove agent credits" })).toHaveAccessibleDescription("Strips agent credit lines from the commit message.");
    expect(pullsApi.merge).not.toHaveBeenCalled();
  });

  it("sends what the dialog shows, and remembers the delete choice", async () => {
    const { onMerged } = await open();
    fireEvent.change(screen.getByRole("combobox", { name: "Merge method" }), { target: { value: "rebase" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Delete the branch" }));
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await flush();
    expect(pullsApi.merge).toHaveBeenCalledWith("p", 7, { method: "rebase", auto: false, delete_branch: true, strip_credits: false, expected_head: "abc" });
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
});
