import { settingsLabel } from "../settingsSearch";
import { useUi } from "../../../product/state/ui";
import { SettingGroup } from "../rows/SettingRow";
import { heading, Toggle } from "./parts";

export function LabsSection() {
  const multiline = useUi((state) => state.multilineRows);
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
          title={settingsLabel("labs-context-window-indicator")}
          description="Shows how full the context is, as a circle in the composer."
          on={meter}
          set={(on) => setPrefs({ contextMeter: on })}
        />
      </SettingGroup>
    </>
  );
}
