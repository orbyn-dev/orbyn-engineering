import { useEffect, useRef } from "react";
import type { EngineStatus, PermissionMode } from "../../../api/types";
import { resolveModel, useEngineModels } from "../../../product/state/modelStore";
import { EffortTrigger, offeredModes, useTraitsMenu } from "../../thread";
import { ModelPickerTrigger } from "../../thread";
import type { ModelChoice } from "./draftHelpers";

export function DraftModelControls({
  engines,
  engine,
  model,
  effort,
  extras,
  access,
  register,
  onModel,
  onEffort,
  onOption,
  onAccess,
}: {
  engines: EngineStatus[];
  engine: string;
  model: string | null;
  effort: string | null;
  extras: Record<string, string>;
  access: PermissionMode | null;
  register(open: () => (element: HTMLElement) => void): void;
  onModel(next: ModelChoice): void;
  onEffort(value: string): void;
  onOption(option: string, value: string): void;
  onAccess(mode: PermissionMode): void;
}) {
  const models = useEngineModels(engine);
  const resolved = resolveModel(models, model);
  const modes = offeredModes(engines.find((entry) => entry.id === engine) ?? null);
  const traits = useTraitsMenu({ model: resolved, effort, extras, access, modes, onEffort, onOption, onAccess });
  const openAt = useRef(traits.openAt);
  openAt.current = traits.openAt;
  useEffect(() => {
    register(() => (element: HTMLElement) => openAt.current(element, true));
  }, [register]);
  return (
    <>
      <ModelPickerTrigger
        engines={engines}
        selected={{ engine, model }}
        onPick={onModel}
      />
      <EffortTrigger
        model={resolved}
        effort={effort}
        access={access}
        onOpen={(element) => traits.openAt(element, false)}
      />
      {traits.menu}
    </>
  );
}
