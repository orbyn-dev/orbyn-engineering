import { ChevronDown } from "lucide-react";
import { settingsLabel } from "../settingsSearch";
import { useState } from "react";
import { api } from "../api";
import { useAsync } from "../../../lib/useAsync";
import type { ProjectGroupingMode } from "../../../generated/ProjectGroupingMode";
import { useUi } from "../../../product/state/ui";
import { Button } from "../../../ui/controls";
import { Segmented } from "../../../ui/Segmented";
import { SettingGroup, SettingRow } from "../rows/SettingRow";
import { BaseDirectoryDialog } from "../projects/BaseDirectoryDialog";
import { Menu } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { DETAIL_ROWS, detailsMenuItems } from "../../../product/ui/details";
import { DesktopApp } from "./DesktopApp";
import { SnapshotSettingsSection } from "../../snapshot";
import { PreferenceGroups } from "../rows/PreferenceRows";
import { Toggle } from "./parts";

function DetailRowsButton() {
  const hidden = useUi((state) => state.detailsHidden);
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null);
  return (
    <>
      <Button
        small
        icon={<ChevronDown size={13} aria-hidden />}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setAnchor({ x: rect.left, y: rect.bottom + 4 });
        }}
      >
        {DETAIL_ROWS.length - hidden.length} of {DETAIL_ROWS.length} on
      </Button>
      <Presence>{anchor ? <Menu {...anchor} label="Thread details rows" items={detailsMenuItems()} onClose={() => setAnchor(null)} /> : null}</Presence>
    </>
  );
}

/** A whole number with its unit beside it, kept within `min` and `max` and saved when the field loses focus. */
export function NumberWithUnit({ value, unit, label, save, disabled, min = 1, max }: { value: unknown; unit: string; label: string; save(next: number): void; disabled: boolean; min?: number; max?: number }) {
  return (
    <span className="unit-field">
      <input type="number" min={min} max={max} aria-label={label} disabled={disabled} defaultValue={typeof value === "number" ? value : ""}
        onBlur={(event) => {
          const text = event.target.value.trim();
          const typed = Number(text);
          // An emptied field goes back to the stored value: saving nothing would leave
          // the row reading differently from what the service holds.
          if (text === "" || !Number.isFinite(typed)) {
            event.target.value = typeof value === "number" ? String(value) : "";
            return;
          }
          const next = Math.min(max ?? Number.POSITIVE_INFINITY, Math.max(min, typed));
          event.target.value = String(next);
          if (next !== value) save(next);
        }} />
      <span className="t3">{unit}</span>
    </span>
  );
}

const unitRow = (key: string, title: string, description: string, unit = "minutes") => ({
  key, title, description,
  render: (value: unknown, save: (next: unknown) => void, control: { disabled: boolean }) => <NumberWithUnit value={value} unit={unit} label={title} save={save} disabled={control.disabled} />,
});

/** A text field with the folder browser beside it. */
function BaseDirectory({ value, save, disabled }: { value: unknown; save(next: unknown): void; disabled: boolean }) {
  const [choosing, setChoosing] = useState(false);
  const [draft, setDraft] = useState(typeof value === "string" ? value : "");
  return (
    <>
      <input aria-label="Base directory" value={draft} disabled={disabled} spellCheck={false}
        onChange={(event) => setDraft(event.target.value)} onBlur={() => { if (draft !== (value ?? "")) save(draft); }} />
      <Button small disabled={disabled} onClick={() => setChoosing(true)}>Choose…</Button>
      {choosing ? (
        <BaseDirectoryDialog onClose={() => setChoosing(false)} onChoose={(path) => { setDraft(path); save(path); setChoosing(false); }} />
      ) : null}
    </>
  );
}

export function GeneralSection() {
  const working = useUi((state) => state.workingSection);
  const projectGrouping = useUi((state) => state.projectGrouping);
  const threadSort = useUi((state) => state.threadSort);
  const projectSort = useUi((state) => state.projectSort);
  const notifications = useUi((state) => state.threadNotifications);
  const inApp = useUi((state) => state.inAppNotifications);
  const confirmUnpin = useUi((state) => state.confirmUnpin);
  const confirmArchive = useUi((state) => state.confirmArchive);
  const confirmDelete = useUi((state) => state.confirmDelete);
  const setPrefs = useUi((state) => state.setPrefs);
  const sound = useUi((state) => state.threadSound);
  // ⌘Q is the macOS desktop app's key, so its confirmation is the only row drawn there.
  const info = useAsync("environment-identity", (signal) => api.info(signal));
  const macos = info.data?.platform === "macos";
  const order = [{ value: "updated_at", label: "Last user message" }, { value: "created_at", label: "Created" }] as const;
  return (
    <>
      <PreferenceGroups groups={[{ title: "", rows: [{
        key: "default_thread_env_mode",
        title: settingsLabel("general-new-threads-start-in"),
        options: [
          { value: "local", label: "Current checkout" },
          { value: "worktree", label: "A new worktree" },
        ],
      }] }]} />
      <SettingGroup>
        <SettingRow title={settingsLabel("general-thread-notifications")} description="When a thread finishes, fails or needs you">
          <Segmented
            label="Thread notifications"
            value={notifications ? (sound ? "both" : "alert") : sound ? "sound" : "off"}
            options={[
              { value: "off", label: "Off" },
              { value: "alert", label: "Notifications" },
              { value: "sound", label: "Sound" },
              { value: "both", label: "Both" },
            ]}
            onChange={(value) => {
              const alert = value === "alert" || value === "both";
              if (alert && "Notification" in window) void Notification.requestPermission();
              setPrefs({ threadNotifications: alert, threadSound: value === "sound" || value === "both" });
            }}
          />
        </SettingRow>
        <Toggle
          title={settingsLabel("general-in-app-notifications")}
          description="A toast when another thread finishes, fails or needs input"
          on={inApp}
          set={(on) => setPrefs({ inAppNotifications: on })}
        />
      </SettingGroup>
      <PreferenceGroups groups={[{ title: "", rows: [
        { key: "terminal_attention_enabled", title: settingsLabel("general-terminal-agents-waiting"),
          description: "Tell you when an agent in a terminal stops and waits for an answer" },
        { key: "terminal_attention_quiet_seconds", title: settingsLabel("general-terminal-agents-quiet-time"),
          description: "How long a terminal stays quiet before it counts as waiting",
          showWhen: (valueOf) => valueOf("terminal_attention_enabled") !== false,
          render: (value, save, control) => <NumberWithUnit value={value} unit="seconds" min={10} max={120} label={settingsLabel("general-terminal-agents-quiet-time")} save={save} disabled={control.disabled} /> },
      ] }]} />
      {macos ? (
        <PreferenceGroups groups={[{ title: "", rows: [{
          key: "quit_confirmation",
          title: settingsLabel("general-quit-confirmation"),
          description: "How ⌘Q quits the desktop app on macOS. The Quit menu item quits.",
          segmented: true,
          options: [
            { value: "direct", label: "Quit at once" },
            { value: "hold", label: "Hold ⌘Q" },
            { value: "double_press", label: "Press ⌘Q twice" },
          ],
        }] }]} />
      ) : null}
      <SettingGroup>
        <SettingRow title={settingsLabel("general-project-grouping")} description="How projects in the same repository are shown">
          <Segmented
            label="Project grouping"
            value={projectGrouping}
            options={[
              { value: "repository", label: "Repository" },
              { value: "repository_path", label: "Repository path" },
              { value: "separate", label: "Separate" },
            ]}
            onChange={(value) => setPrefs({ projectGrouping: value as ProjectGroupingMode })}
          />
        </SettingRow>
        <SettingRow title={settingsLabel("general-project-order")}>
          <Segmented label="Project order" value={projectSort} options={[...order]} onChange={(value) => setPrefs({ projectSort: value })} />
        </SettingRow>
        <SettingRow title={settingsLabel("general-thread-order")}>
          <Segmented label="Thread order" value={threadSort} options={[...order]} onChange={(value) => setPrefs({ threadSort: value })} />
        </SettingRow>
        <Toggle
          title={settingsLabel("general-working-section")}
          description="Fold working threads into a shelf at the top of the sidebar"
          on={working}
          set={(on) => setPrefs({ workingSection: on })}
        />
        <SettingRow title={settingsLabel("general-rows")} description="Choose what the Details popover shows. You can also right-click it.">
          <DetailRowsButton />
        </SettingRow>
      </SettingGroup>
      <PreferenceGroups groups={[{ title: "", rows: [{
        key: "background_activity_profile",
        title: settingsLabel("general-activity-profile"),
        description: "How often the app refreshes in the background",
        segmented: true,
        options: [
          { value: "balanced", label: "Balanced" },
          { value: "saver", label: "Saver" },
          { value: "performance", label: "Performance" },
        ],
      },
      unitRow("automatic_git_fetch_minutes", settingsLabel("general-git-fetch-interval-minutes"), "Background git fetch period"),
      unitRow("provider_health_refresh_minutes", settingsLabel("general-provider-health-interval-minutes"), "How often providers are checked"),
      { key: "crew_turn_budget", title: settingsLabel("general-crew-turn-budget"), description: "Turns each crew may spend. Once spent, no new Crew task starts; running tasks finish. 0 is no budget.",
        render: (value, save, control) => <NumberWithUnit value={value} unit="turns" min={0} label={settingsLabel("general-crew-turn-budget")} save={save} disabled={control.disabled} /> },
      { key: "enable_provider_update_checks", title: settingsLabel("general-check-providers-for-updates") },
      { key: "keep_awake_during_runs", title: settingsLabel("general-keep-the-computer-awake-during-runs"), description: "Desktop app. The display can still sleep." },
      { key: "add_project_base_directory", title: settingsLabel("general-base-directory"), description: "Where new projects are created",
        render: (value, save, control) => <BaseDirectory value={value} save={save} disabled={control.disabled} /> },
      ] }]} />
      <DesktopApp />
      <SnapshotSettingsSection />
      <p className="set-grp">Ask before</p>
      <SettingGroup>
        <Toggle title={settingsLabel("general-unpin-confirmation")} on={confirmUnpin} set={(on) => setPrefs({ confirmUnpin: on })} />
        <Toggle title={settingsLabel("general-archive-confirmation")} on={confirmArchive} set={(on) => setPrefs({ confirmArchive: on })} />
        <Toggle title={settingsLabel("general-delete-confirmation")} on={confirmDelete} set={(on) => setPrefs({ confirmDelete: on })} />
      </SettingGroup>
    </>
  );
}
