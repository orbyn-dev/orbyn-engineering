import { settingsLabel } from "../settingsSearch";
import { useUi } from "../../../state/ui";
import { SettingGroup } from "../../../ui/SettingRow";
import { heading, Toggle } from "./parts";

export function LabsSection() {
  const multiline = useUi((state) => state.multilineRows);
  const plan = useUi((state) => state.planControls);
  const meter = useUi((state) => state.contextMeter);
  const setPrefs = useUi((state) => state.setPrefs);
  return (
    <>
      {heading("Labs")}
      <SettingGroup title="Sidebar">
        <Toggle
          title={settingsLabel("labs-multiline-rows")}
          description="Let thread titles in folders wrap."
          on={multiline}
          set={(on) => setPrefs({ multilineRows: on })}
        />
      </SettingGroup>
      <SettingGroup title="Legacy">
        <Toggle
          title={settingsLabel("labs-plan-mode")}
          description="Keeps the Plan button, /plan, /default and Shift+Tab. Off uses Build."
          on={plan}
          set={(on) => setPrefs({ planControls: on })}
        />
        <Toggle
          title={settingsLabel("labs-context-window-indicator")}
          description="Shows how full the context is, as a circle in the composer."
          on={meter}
          set={(on) => setPrefs({ contextMeter: on })}
        />
      </SettingGroup>
    </>
  );
}
