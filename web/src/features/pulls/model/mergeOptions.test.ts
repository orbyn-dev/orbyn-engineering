import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { deleteBranchChoice, rememberDeleteBranch, resetMergeOptions, useMergeOptions } from "./mergeOptions";

beforeEach(() => {
  localStorage.clear();
  resetMergeOptions();
});

describe("the merge options the menu and the dialog share", () => {
  it("starts with checks-pass off and the project's remembered delete choice", () => {
    rememberDeleteBranch("p", true);
    const { result } = renderHook(() => useMergeOptions("p", 1));
    expect(result.current.auto).toBe(false);
    expect(result.current.deleteBranch).toBe(true);
  });

  it("one reader sees what the other sets", () => {
    const menu = renderHook(() => useMergeOptions("p", 1));
    const dialog = renderHook(() => useMergeOptions("p", 1));
    act(() => menu.result.current.setAuto(true));
    expect(dialog.result.current.auto).toBe(true);
    act(() => dialog.result.current.setDeleteBranch(true));
    expect(menu.result.current.deleteBranch).toBe(true);
  });

  it("is scoped to the pull request: another one starts afresh, and so does coming back", () => {
    const first = renderHook(() => useMergeOptions("p", 1));
    act(() => first.result.current.setAuto(true));
    const other = renderHook(() => useMergeOptions("p", 2));
    expect(other.result.current.auto).toBe(false);
    act(() => other.result.current.setAuto(true));
    expect(renderHook(() => useMergeOptions("p", 1)).result.current.auto).toBe(false);
  });

  it("setting a choice does not write the remembered delete choice by itself", () => {
    const { result } = renderHook(() => useMergeOptions("p", 1));
    act(() => result.current.setDeleteBranch(true));
    expect(deleteBranchChoice("p")).toBe(false);
  });
});

describe("delete branch after merge", () => {
  it("is off until chosen and then remembered for that project only", () => {
    expect(deleteBranchChoice("p")).toBe(false);
    rememberDeleteBranch("p", true);
    expect(deleteBranchChoice("p")).toBe(true);
    expect(deleteBranchChoice("q")).toBe(false);
  });
});
