import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { EngineStatus, ModelInfo, PermissionMode } from "../../../api/types";
import { EffortTrigger, offeredModes, oneModeReason, refusalSentence, refusedModes, shownAccess, useTraitsMenu } from "./ComposerTraits";

const withModes = (permission_modes?: PermissionMode[]) =>
  ({ capabilities: { permission_modes } }) as unknown as EngineStatus;

it("offers only the engine's modes and shows a missing saved mode as Supervised", () => {
  const pi = withModes(["full_access", "supervised", "auto_edits"]);
  expect(offeredModes(pi)).toEqual(["supervised", "auto_edits", "full_access"]);
  expect(shownAccess(pi, "auto")).toBe("supervised");
  expect(shownAccess(pi, "full_access")).toBe("full_access");
  expect(shownAccess(withModes(["supervised", "full_access"]), "auto_edits")).toBe("supervised");
  // An older service that does not list modes offers them all.
  expect(offeredModes(withModes(undefined))).toHaveLength(4);
  expect(offeredModes(null)).toHaveLength(4);
});

function Menu({ modes }: { modes: PermissionMode[] }) {
  const traits = useTraitsMenu({
    model: null,
    effort: null,
    extras: {},
    access: "supervised",
    modes,
    onEffort: () => {},
    onOption: () => {},
    onAccess: () => {},
  });
  return (
    <>
      <button type="button" onClick={(event) => traits.openAt(event.currentTarget, true)}>
        open
      </button>
      {traits.menu}
    </>
  );
}

it("lists only the offered modes in the permissions menu", () => {
  render(<Menu modes={["supervised", "auto"]} />);
  fireEvent.click(screen.getByRole("button", { name: "open" }));
  const group = screen.getByRole("group", { name: "Permissions" });
  const labels = [...group.querySelectorAll("button")].map((button) => button.getAttribute("aria-label"));
  expect(labels).toEqual(["Supervised", "Auto"]);
});

const withEffort = (values: { id: string; label: string; default?: boolean }[]) =>
  ({ options: [{ id: "effort", label: "Effort", values }] }) as unknown as ModelInfo;

it("names the effort level in its pill, says Default when none is chosen, and draws nothing without an effort setting", () => {
  const levels = [{ id: "low", label: "Low" }, { id: "medium", label: "Medium", default: true }];
  const shown = render(<EffortTrigger model={withEffort(levels)} effort={null} access={null} onOpen={() => {}} />);
  expect(screen.getByRole("button", { name: /Effort: Medium/ })).toHaveTextContent(/Medium|^$/);
  shown.unmount();
  // An effort option with no level chosen or defaulted still shows a pill, so a level can be picked.
  const unset = render(<EffortTrigger model={withEffort([{ id: "low", label: "Low" }])} effort={null} access={null} onOpen={() => {}} />);
  expect(screen.getByRole("button", { name: "Effort: Default" })).toHaveTextContent("Default");
  unset.unmount();
  const absent = render(<EffortTrigger model={{ options: [] } as unknown as ModelInfo} effort={null} access={null} onOpen={() => {}} />);
  expect(absent.container).toBeEmptyDOMElement();
});

describe("an engine that never asks before acting", () => {
  // The list as the service words it (a guess until slice 3 merges): one mode that is not in the generated type.
  const cursor = { name: "Cursor", capabilities: { permission_modes: ["no_approvals"] } } as unknown as EngineStatus;

  it("refuses Supervised and Auto edits, offers Auto and Full access, and reads no engine name", () => {
    expect(refusedModes(cursor)).toEqual(["supervised", "auto_edits"]);
    expect(offeredModes(cursor)).toEqual(["auto", "full_access"]);
    const renamed = { name: "Other", capabilities: { permission_modes: ["no_approvals"] } } as unknown as EngineStatus;
    expect(refusedModes(renamed)).toEqual(["supervised", "auto_edits"]);
    expect(refusedModes({ name: "Cursor", capabilities: { permission_modes: ["supervised", "auto"] } } as unknown as EngineStatus)).toEqual([]);
    expect(refusedModes(null)).toEqual([]);
  });

  it("shows the saved mode as it is, because the next send is refused rather than run as Supervised", () => {
    expect(shownAccess(cursor, "supervised")).toBe("supervised");
    expect(shownAccess(cursor, "auto")).toBe("auto");
  });

  it("says why in the app's own mode names", () => {
    expect(refusalSentence("cursor", "Cursor", ["supervised", "auto_edits"])).toBe(
      "Cursor does not ask before acting, so Supervised and Auto-accept edits are not available here.",
    );
    expect(refusalSentence("cursor", "Cursor", ["supervised"])).toBe("Cursor does not ask before acting, so Supervised is not available here.");
  });

  it("gives another engine Orbyn's own reason, in both the one mode and the refused rows", () => {
    expect(oneModeReason("kiro", "Kiro")).toBe("Orbyn cannot ask before Kiro acts. This thread has one mode.");
    expect(refusalSentence("kiro", "Kiro", ["supervised", "auto_edits"])).toBe(
      "Orbyn cannot ask before Kiro acts. Supervised and Auto-accept edits are not available here.",
    );
    expect(refusalSentence("kiro", "Kiro", ["supervised"])).toBe("Orbyn cannot ask before Kiro acts. Supervised is not available here.");
  });

  it("draws the refused modes dimmed with the reason, and a press on one changes nothing and says why", () => {
    const onAccess = vi.fn();
    const onRefused = vi.fn();
    function Open() {
      const traits = useTraitsMenu({
        model: null, effort: null, extras: {}, access: "auto", modes: offeredModes(cursor),
        refused: { modes: refusedModes(cursor), sentence: refusalSentence("cursor", "Cursor", refusedModes(cursor)), onPick: onRefused },
        onEffort: () => {}, onOption: () => {}, onAccess,
      });
      return <><button type="button" onClick={(event) => traits.openAt(event.currentTarget, true)}>open</button>{traits.menu}</>;
    }
    render(<Open />);
    fireEvent.click(screen.getByRole("button", { name: "open" }));
    const group = screen.getByRole("group", { name: "Permissions" });
    const labels = [...group.querySelectorAll("button")].map((button) => button.getAttribute("aria-label"));
    expect(labels).toEqual(["Supervised", "Auto-accept edits", "Auto", "Full access"]);
    const supervised = screen.getByRole("menuitemradio", { name: "Supervised" });
    expect(supervised).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("menuitemradio", { name: "Auto" })).not.toHaveAttribute("aria-disabled");
    expect(screen.getByText("Cursor does not ask before acting, so Supervised and Auto-accept edits are not available here.")).toBeVisible();
    fireEvent.click(supervised);
    expect(onAccess).not.toHaveBeenCalled();
    expect(onRefused).toHaveBeenCalledTimes(1);
  });
});

it("reaches the effort pill by Tab and opens its menu with Enter and with Space", async () => {
  const user = userEvent.setup();
  const opened = vi.fn();
  render(<EffortTrigger model={withEffort([{ id: "low", label: "Low" }])} effort={null} access={null} onOpen={opened} />);
  const trigger = screen.getByRole("button", { name: "Effort: Default" });
  expect(trigger).toHaveAttribute("aria-haspopup", "menu");
  await user.tab();
  expect(trigger).toHaveFocus();
  await user.keyboard("{Enter}");
  expect(opened).toHaveBeenCalledTimes(1);
  await user.keyboard(" ");
  expect(opened).toHaveBeenCalledTimes(2);
});
