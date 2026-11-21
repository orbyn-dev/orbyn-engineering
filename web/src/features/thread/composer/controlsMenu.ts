import type { ModelInfo, PermissionMode, Thread } from "../../../api/types";
import type { MenuItem } from "../../../ui/Menu";
import { ACCESS, type AccessView, type useTraitsMenu } from "./ComposerTraits";
import { effortButton } from "./composerConstants";

/** The footer's controls, folded into one menu on a narrow composer. */
export function controlsMenuItems({
  thread,
  model,
  engineName,
  effortShown,
  effortLevel,
  busy,
  unmanaged,
  access,
  accessView,
  traits,
}: {
  thread: Thread;
  model: ModelInfo | null;
  engineName: string;
  effortShown: boolean;
  effortLevel: { label: string } | null;
  busy: boolean;
  unmanaged: boolean;
  access: PermissionMode;
  accessView?: AccessView;
  traits: Pick<ReturnType<typeof useTraitsMenu>, "openAt">;
}): MenuItem[] {
  return [
    {
      label: `${model?.label ?? thread.model ?? engineName}${effortShown ? ` · ${effortLevel?.label ?? "Default"}` : ""}`,
      disabled: busy,
      onSelect: () => document.querySelector<HTMLElement>("form.composer [data-model-trigger]")?.click(),
    },
    ...(effortShown
      ? [{
        label: "Effort",
        disabled: busy,
        onSelect: () => {
          const trigger = effortButton();
          if (trigger) traits.openAt(trigger, false);
        },
      }]
      : []),
    ...(unmanaged
      ? []
      : [
          {
            label: `Permissions: ${accessView?.label ?? ACCESS[access].label}`,
            disabled: busy || !!accessView?.locked,
            onSelect: () => {
              const trigger = effortButton();
              if (trigger) traits.openAt(trigger, true);
            },
          },
        ]),
  ];
}
