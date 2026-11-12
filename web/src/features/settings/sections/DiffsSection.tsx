import { settingsLabel } from "../settingsSearch";
import { useUi } from "../../../product/state/ui";
import { Segmented } from "../../../ui/Segmented";
import { SettingGroup, SettingRow } from "../rows/SettingRow";
import { PreferenceGroups } from "../rows/PreferenceRows";
import { heading, Toggle } from "./parts";

/** The same stored layout the diff panel's own switch uses. */
function DiffViewRow() {
  const layout = useUi((state) => state.diffLayout);
  const setPrefs = useUi((state) => state.setPrefs);
  return (
    <SettingRow title={settingsLabel("diffs-diff-view")}>
      <Segmented
        label="Diff view"
        value={layout}
        options={[
          { value: "stacked", label: "Unified" },
          { value: "split", label: "Split" },
        ]}
        onChange={(next) => setPrefs({ diffLayout: next })}
      />
    </SettingRow>
  );
}

/** The same stored switch the Changes panel's menu has: whitespace-only changes leave the diff. */
function IgnoreWhitespaceRow() {
  const on = useUi((state) => state.diffIgnoreWhitespace);
  const setPrefs = useUi((state) => state.setPrefs);
  return <Toggle title={settingsLabel("diffs-ignore-whitespace-changes")} on={on} set={(next) => setPrefs({ diffIgnoreWhitespace: next })} />;
}

export function DiffsSection() {
  const inApp = useUi((state) => state.pullLinksInApp);
  const setPrefs = useUi((state) => state.setPrefs);
  return (
    <>
      {heading("Diffs & pull requests")}
      <SettingGroup>
        <DiffViewRow />
        <IgnoreWhitespaceRow />
      </SettingGroup>
      <PreferenceGroups groups={[{ title: "", rows: [{
        key: "pull_request_merge_method",
        title: settingsLabel("diffs-merge-method"),
        options: [
          { value: "", label: "Ask" },
          { value: "merge", label: "Merge" },
          { value: "squash", label: "Squash" },
          { value: "rebase", label: "Rebase" },
        ],
      }] }]} />
      <SettingGroup>
        <SettingRow title={settingsLabel("diffs-open-pull-request-links-here")} description="In the Pull requests page, or on the host">
          <Segmented
            label="Open pull request links here"
            value={inApp ? "app" : "host"}
            options={[
              { value: "app", label: "In the app" },
              { value: "host", label: "On the host" },
            ]}
            onChange={(next) => setPrefs({ pullLinksInApp: next === "app" })}
          />
        </SettingRow>
      </SettingGroup>
      <PreferenceGroups
        groups={[
          {
            title: "",
            rows: [
              { key: "remove_agent_credits_on_merge", title: settingsLabel("diffs-remove-agent-credits-on-merge"), description: "Strips agent credit lines from the merged commit message" },
              {
                key: "review_guides_enabled",
                title: settingsLabel("diffs-review-guides"),
                description: "An agent writes a walkthrough for each pull request",
              },
              {
                key: "review_guide_instructions",
                title: settingsLabel("diffs-guide-instructions"),
                description: "What the guide should cover",
                multiline: true,
              },
              { key: "review_guide_model", title: settingsLabel("diffs-guide-model") },
            ],
          },
        ]}
      />
    </>
  );
}
