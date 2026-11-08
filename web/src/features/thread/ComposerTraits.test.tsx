import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import type { EngineStatus, ModelInfo, PermissionMode } from "../../api/types";
import { EffortTrigger, offeredModes, shownAccess, useTraitsMenu } from "./ComposerTraits";

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
