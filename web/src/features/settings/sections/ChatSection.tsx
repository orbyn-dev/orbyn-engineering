import { settingsLabel } from "../settingsSearch";
import { useUi } from "../../../state/ui";
import { Segmented } from "../../../ui/Segmented";
import { SettingGroup, SettingRow } from "../../../ui/SettingRow";
import { PreferenceGroups } from "../rows/PreferenceRows";
import { heading, Toggle } from "./parts";

const MOD = typeof navigator !== "undefined" && /mac/i.test(navigator.platform) ? "⌘" : "Ctrl";

export function ChatSection() {
  const math = useUi((state) => state.mathRendering);
  const wordWrap = useUi((state) => state.wordWrap);
  const chatWidth = useUi((state) => state.chatWidth);
  const timestampFormat = useUi((state) => state.timestampFormat);
  const sendShortcut = useUi((state) => state.sendShortcut);
  const followUpBehavior = useUi((state) => state.followUpBehavior);
  const persistent = useUi((state) => state.persistentComposerContext);
  const proactive = useUi((state) => state.proactivePanels);
  const searchEngine = useUi((state) => state.previewSearchEngine);
  const linkTarget = useUi((state) => state.linkTarget);
  const autoFloat = useUi((state) => state.autoFloatPreview);
  const skillsInSlash = useUi((state) => state.skillsInSlashMenu);
  const setPrefs = useUi((state) => state.setPrefs);
  return (
    <>
      {heading("Chat")}
      <SettingGroup>
        <SettingRow title={settingsLabel("chat-chat-width")}>
          <Segmented
            label="Chat width"
            value={chatWidth}
            options={[
              { value: "comfortable", label: "Comfortable" },
              { value: "wide", label: "Wide" },
              { value: "full", label: "Full" },
            ]}
            onChange={(next) => setPrefs({ chatWidth: next })}
          />
        </SettingRow>
        <SettingRow title={settingsLabel("chat-send-shortcut")} description="What sends a message">
          <select aria-label="Send shortcut" value={sendShortcut} onChange={(event) => setPrefs({ sendShortcut: event.target.value as "enter" | "mod-enter-multiline" | "mod-enter" })}>
            <option value="enter">Enter</option>
            <option value="mod-enter-multiline">{MOD} Enter when the message has several lines</option>
            <option value="mod-enter">{MOD} Enter</option>
          </select>
        </SettingRow>
        <SettingRow title={settingsLabel("chat-while-a-turn-runs-enter")} description="Steer sends it into the turn that is running.">
          <select aria-label="While a turn runs, Enter" value={followUpBehavior} onChange={(event) => setPrefs({ followUpBehavior: event.target.value as "queue" | "steer" })}>
            <option value="queue">Queues the message</option>
            <option value="steer">Steers the running turn</option>
          </select>
        </SettingRow>
        <Toggle title={settingsLabel("chat-math-rendering")} on={math} set={(on) => setPrefs({ mathRendering: on })} />
      </SettingGroup>
      <PreferenceGroups groups={[{ title: "", rows: [
        { key: "response_streaming_mode", title: settingsLabel("chat-show-answers"),
          options: [
            { value: "paragraph", label: "By paragraph" },
            { value: "turn", label: "When the turn ends" },
          ] },
        { key: "default_permission_mode", title: settingsLabel("chat-default-access-mode"),
          options: [
            { value: "supervised", label: "Supervised" },
            { value: "auto_edits", label: "Auto-accept edits" },
            { value: "auto", label: "Auto" },
            { value: "full_access", label: "Full access" },
          ] },
      ] }]} />
      <SettingGroup>
        <SettingRow title={settingsLabel("chat-timestamp-format")}>
          <Segmented
            label="Timestamp format"
            value={timestampFormat}
            options={[
              { value: "locale", label: "System default" },
              { value: "12-hour", label: "12-hour" },
              { value: "24-hour", label: "24-hour" },
            ]}
            onChange={(next) => setPrefs({ timestampFormat: next })}
          />
        </SettingRow>
        <Toggle title={settingsLabel("chat-word-wrap")} description="Wrap long lines in code" on={wordWrap} set={(on) => setPrefs({ wordWrap: on })} />
        <Toggle title={settingsLabel("chat-keep-checkout-and-branch-visible")} description="In the composer’s context line" on={persistent} set={(on) => setPrefs({ persistentComposerContext: on })} />
        <Toggle title={settingsLabel("chat-show-skills-in-the-slash-menu")} on={skillsInSlash} set={(on) => setPrefs({ skillsInSlashMenu: on })} />
        <Toggle title={settingsLabel("chat-float-pages-agents-open")} description="Float a page over the chat instead of replacing the panel" on={autoFloat} set={(on) => setPrefs({ autoFloatPreview: on })} />
        <Toggle title={settingsLabel("chat-open-changes-after-large-turns")} description="Opens the Changes panel by itself" on={proactive} set={(on) => setPrefs({ proactivePanels: on })} />
      </SettingGroup>
      <SettingGroup>
        <SettingRow title={settingsLabel("chat-search-engine")} description="For the Browser panel’s address bar">
          <select aria-label="Search engine" value={searchEngine} onChange={(event) => setPrefs({ previewSearchEngine: event.target.value })}>
            <option value="duckduckgo">DuckDuckGo</option>
            <option value="google">Google</option>
            <option value="bing">Bing</option>
            <option value="brave">Brave</option>
            <option value="kagi">Kagi</option>
            <option value="startpage">Startpage</option>
          </select>
        </SettingRow>
        <SettingRow title={settingsLabel("chat-open-links-in")}>
          <Segmented
            label="Open links in"
            value={linkTarget}
            options={[
              { value: "system", label: "System browser" },
              { value: "in-app", label: "Browser panel" },
            ]}
            onChange={(next) => setPrefs({ linkTarget: next })}
          />
        </SettingRow>
      </SettingGroup>
      <PreferenceGroups
        groups={[
          {
            title: "",
            rows: [
              { key: "open_web_links_in_app", title: settingsLabel("chat-open-links-from-other-apps"), description: "macOS links open in the Browser panel" },
            ],
          },
        ]}
      />
      <p className="set-grp">Idle threads and limits</p>
      <PreferenceGroups
        groups={[
          {
            title: "",
            rows: [
              { key: "compact_idle_threads", title: settingsLabel("chat-compact-idle-threads") },
              {
                key: "idle_compaction",
                title: settingsLabel("chat-idle-compaction"),
                description: "When an idle thread is compacted",
                segmented: true,
                options: [
                  { value: "on_return", label: "On return" },
                  { value: "before_cache_expiry", label: "Before cache expiry" },
                  { value: "never", label: "Never" },
                ],
              },
              { key: "snooze_limited_threads", title: settingsLabel("chat-snooze-until-limits-reset") },
              { key: "auto_resume_limited_threads", title: settingsLabel("chat-resume-after-usage-limits") },
            ],
          },
        ]}
      />
    </>
  );
}
