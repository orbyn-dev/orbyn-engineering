import { SCOPE_LINE_SECTIONS, SettingsScopeProvider } from "./rows/SettingsScopeContext";
import { ScopeSentence } from "./rows/ScopeSentence";
import { SettingRowIds } from "./rows/SettingRow";
import { SETTINGS, searchSettings, settingSearchId } from "./settingsSearch";
import { api } from "./api";
import { onShellChange } from "./api";
import { useAsync } from "../../lib/useAsync";
import { navigate, useRoute } from "../../product/state/route";
import { Info, Search, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNarrowPage } from "./useNarrowPage";
import { SettingsCrumbs } from "./SettingsCrumbs";
import { SettingsSubPageContext, SUBPAGES } from "./subPage";
import { routeHref, type SettingsSection, type SettingsSubPage } from "../../product/state/route";
import { Button, IconButton } from "../../ui/controls";
import { AppearanceSection } from "./pages/Appearance";
import { KeybindingsSection } from "./pages/Keybindings";
import { AccountSection, useAccountAvailable } from "../account";
import { CrewSettingsSection, useCrewFlag } from "../crew";
import { BrowserProfilesSection } from "./browser/BrowserProfiles";
import { MaintenanceSection } from "./maintenance/Maintenance";
import { ProjectsSection } from "./projects/Projects";
import { ProvidersSection } from "./providers/Providers";
import { AboutSection } from "./sections/AboutSection";
import { ArchiveSection } from "./sections/ArchiveSection";
import { ChatSection } from "./sections/ChatSection";
import { DiffsSection } from "./sections/DiffsSection";
import { GeneralSection } from "./sections/GeneralSection";
import { LabsSection } from "./sections/LabsSection";
import { SourceControlSection } from "./sections/SourceControlSection";
import { WorktreesSection } from "./sections/WorktreesSection";
import { SecretsSection } from "./secrets/SecretsSection";
import { ScheduledTasksSection } from "./scheduled-tasks/ScheduledTasks";
import { ThreadsSection } from "./pages/Threads";
import { ToolsConnectorsSection } from "./tools-connectors/ToolsConnectors";
import "./settingsDesign.css";

export const labels: Record<SettingsSection, string> = {
  general: "General",
  account: "Account",
  appearance: "Appearance",
  chat: "Chat",
  diffs: "Diffs & pull requests",
  keybindings: "Keybindings",
  providers: "Providers",
  tools: "Tools and connectors",
  threads: "Threads",
  tasks: "Scheduled tasks",
  projects: "Projects",
  "source-control": "Source control",
  worktrees: "Worktrees",
  browser: "Browser",
  crew: "Crew",
  secrets: "Secrets",
  maintenance: "Maintenance",
  labs: "Labs",
  about: "About",
  archive: "Archive",
};

/** The reference product's order. Sections without a backend here stay out. */
const groups: { label: string | null; sections: SettingsSection[] }[] = [
  { label: null, sections: ["general", "account", "appearance", "chat", "diffs", "keybindings"] },
  { label: "Agents", sections: ["providers", "tools", "threads", "tasks", "crew", "secrets"] },
  { label: "Code", sections: ["projects", "source-control", "worktrees", "browser"] },
  { label: null, sections: ["maintenance", "labs", "about", "archive"] },
];

const views: Record<SettingsSection, () => React.JSX.Element | null> = {
  general: GeneralSection,
  account: AccountSection,
  appearance: AppearanceSection,
  chat: ChatSection,
  diffs: DiffsSection,
  tasks: ScheduledTasksSection,
  "source-control": SourceControlSection,
  worktrees: WorktreesSection,
  browser: BrowserProfilesSection,
  crew: CrewSettingsSection,
  secrets: SecretsSection,
  labs: LabsSection,
  about: AboutSection,
  archive: ArchiveSection,
  keybindings: KeybindingsSection,
  providers: ProvidersSection,
  tools: ToolsConnectorsSection,
  threads: ThreadsSection,
  projects: ProjectsSection,
  maintenance: MaintenanceSection,
};

/** Settings fill the main area of the frame: a section list on the left, the open section on the right. */
export function SettingsPage({ section, sub = null, unknown, row, onClose }: { section: SettingsSection | null; sub?: SettingsSubPage | null; unknown?: string; row?: string; onClose(): void }) {
  return <SettingsScopeProvider section={section ?? "general"}><SettingsPageContent section={section} sub={sub} unknown={unknown} row={row} onClose={onClose} /></SettingsScopeProvider>;
}

/** The rows an address can name: `chat/search-engine` is the row `chat-search-engine`. A Map, so a key such as `constructor` finds nothing. */
const ADDRESS_ROWS = new Map<string, string>(SETTINGS.flatMap((item) => (item.id.startsWith(`${item.section}-`) ? [[`${item.section}/${item.id.slice(item.section.length + 1)}`, item.id] as [string, string]] : [])));

/** A page name from an address, as the note shows it: plain text, shortened. */
const shown = (name: string) => (name.length > 40 ? `${name.slice(0, 40)}…` : name);

function SettingsPageContent({ section, sub, unknown, row: addressRow, onClose }: { section: SettingsSection | null; sub: SettingsSubPage | null; unknown?: string; row?: string; onClose(): void }) {
  const route = useRoute();
  const sectionRoute = (next: SettingsSection) => ({ name: "settings" as const, section: next, selection: route.name === "settings" ? route.selection : undefined, machine: route.name === "settings" ? route.machine : undefined });
  const ref = useRef<HTMLElement | null>(null);
  const [measure, narrow] = useNarrowPage();
  const setRef = useCallback((element: HTMLElement | null) => { ref.current = element; return measure(element); }, [measure]);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [target, setTarget] = useState<{ id: string; anchor?: boolean } | null>(null);
  const shell = useAsync("settings-search-shell", (signal) => api.shell(signal), { keep: true });
  // The Account category exists only while the sign-in gate is on: no dead entry, nothing disabled-looking.
  const accountOn = useAccountAvailable();
  // Crew is Preview: its category exists only while the service says Crew is on.
  const crewOn = useCrewFlag();
  // A page that is off on this device opens General with a note that says so (a page that does not exist has its own note).
  const off = (section === "account" && !accountOn) || (section === "crew" && !crewOn);
  const current = off ? "general" : (section ?? "general");
  const note = unknown !== undefined
    ? { key: `unknown:${unknown}`, text: `There is no setting called “${shown(unknown)}” in this app. Showing General instead.` }
    : off && section ? { key: `off:${section}`, text: `That page is off on this device: “${shown(labels[section])}”. Showing General instead.` } : null;
  const View = views[current];
  // A sub-page belongs to its own page only.
  const open = sub && SUBPAGES[sub].parent === current ? sub : null;

  // Escape closes settings from anywhere, unless a dialog, menu or field is handling its own.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      // A dialog owns Escape even when its focused button has just been disabled and focus fell to the page.
      if (document.querySelector("dialog[open]")) return;
      const target = event.target;
      if (target instanceof Element) {
        if (target.closest('dialog[open], [role="dialog"], [role="menu"], [role="listbox"], [role="combobox"], [contenteditable="true"]')) return;
        if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return;
        if (target instanceof HTMLInputElement && !["checkbox", "radio", "range", "button"].includes(target.type)) return;
      }
      event.preventDefault();
      onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const sectionResults = groups.flatMap((group) => group.sections).filter((entry) => (entry !== "account" || accountOn) && (entry !== "crew" || crewOn) && labels[entry].toLowerCase().includes(query.trim().toLowerCase()));
  const reloadShell = shell.reload;
  useEffect(() => onShellChange(reloadShell), [reloadShell]);
  const results = useMemo(() => {
    const available = new Set<"cli" | "accessibility" | "screen_recording">();
    if (shell.data?.facts?.cli) available.add("cli");
    for (const permission of shell.data?.facts?.permissions ?? []) {
      if (permission.id === "accessibility" || permission.id === "screen_recording") available.add(permission.id);
    }
    return searchSettings(query, shell.data?.attached === true, available);
  }, [query, shell.data]);

  // An address that names a row on the open page: reveal it, and ring it until the next click or key.
  useEffect(() => {
    const id = section && addressRow !== undefined && section === current ? ADDRESS_ROWS.get(`${section}/${addressRow}`) : undefined;
    if (id) setTarget({ id, anchor: true });
    else setTarget((now) => (now?.anchor ? null : now));
  }, [section, addressRow, current]);

  useEffect(() => {
    if (!target) return;
    const page = ref.current;
    if (!page) return;
    let frame = 0;
    let finished = false;
    let ringed: HTMLElement | null = null;
    const unring = () => {
      ringed?.removeAttribute("data-anchored");
      document.removeEventListener("pointerdown", unring, true);
      document.removeEventListener("keydown", unring, true);
      ringed?.removeEventListener("blur", unring);
      ringed = null;
    };
    const reveal = () => {
      if (finished) return;
      const row = page.querySelector<HTMLElement>(`[id="${target.id}"]`);
      if (!row) return;
      row.dispatchEvent(new Event("reveal-setting", { bubbles: true }));
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (row.closest("[hidden]")) return;
        row.scrollIntoView?.({ block: "center" });
        row.focus({ preventScroll: true });
        if (target.anchor) {
          // No timer: the ring stays while the row has focus, and goes with the next click or key.
          ringed = row;
          row.setAttribute("data-anchored", "true");
          document.addEventListener("pointerdown", unring, true);
          document.addEventListener("keydown", unring, true);
          row.addEventListener("blur", unring);
        }
        finished = true;
        observer.disconnect();
      });
    };
    const observer = new MutationObserver(reveal);
    observer.observe(page, { childList: true, subtree: true, attributes: true, attributeFilter: ["hidden"] });
    reveal();
    return () => { observer.disconnect(); cancelAnimationFrame(frame); unring(); };
  }, [target, current]);

  // The section list scrolls (down on a short window, sideways on a narrow one), so the open section is kept in view.
  const searching = Boolean(query.trim());
  useEffect(() => {
    if (searching) return;
    ref.current?.querySelector<HTMLElement>('.settings-nav [aria-current="page"]')?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [current, searching]);

  return (
    <section
      ref={setRef}
      className="settings-page"
      role="region"
      aria-label="Settings"
    >
      <div className="settings-side">
        <div className="settings-search">
          <Search size={14} aria-hidden />
          <input
            type="search"
            aria-label="Search settings"
            placeholder="Find a setting"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") ref.current?.querySelector<HTMLElement>(".settings-search-menu a")?.click();
            }}
          />
        </div>
        {searching ? (
          <div className="settings-search-menu" aria-label="Search results">
            {results.length || sectionResults.length ? <>
              {results.length ? <p className="mh">Settings</p> : null}
              {results.map((item) => (
                <a className="settings-search-result" key={item.id} aria-label={`${item.title} · ${labels[item.section]}`} href={routeHref(sectionRoute(item.section))}
                  onClick={(event) => { event.preventDefault(); setTarget({ id: item.id }); navigate({ name: "settings", section: item.section }); }}>
                  <span>{item.title}</span><small>{labels[item.section]}</small>
                </a>
              ))}
              {sectionResults.length ? <p className="mh">Sections</p> : null}
              {sectionResults.map((section) => <a key={section} onClick={() => setTarget(null)} href={routeHref(sectionRoute(section))}>{labels[section]}</a>)}
            </> : <p role="status">No settings found</p>}
          </div>
        ) : null}
        <nav className="settings-nav" aria-label="Settings sections">
          {groups.map((group) => (
            <div key={group.label ?? group.sections[0]} className="settings-group-list">
              {group.label ? <p className="settings-group">{group.label}</p> : null}
              {group.sections.filter((entry) => (entry !== "account" || accountOn) && (entry !== "crew" || crewOn)).map((entry) => (
                <a
                  key={entry}
                  onClick={() => setTarget(null)}
                  href={routeHref(sectionRoute(entry))}
                  aria-current={entry === current ? "page" : undefined}
                >
                  {labels[entry]}
                </a>
              ))}
            </div>
          ))}
        </nav>
      </div>
      <div className="settings-main">
        <SettingsCrumbs section={current} label={labels[current]} sub={open} narrow={narrow} labelOf={(entry) => labels[entry]} />
        <header className="settings-head">
          <span className="settings-title" aria-hidden>{open ? SUBPAGES[open].label : labels[current]}</span>
          <IconButton label="Close settings" onClick={onClose}>
            <X size={16} aria-hidden />
          </IconButton>
        </header>
        <div className="settings-scroll">
          <div className="settings-body">
            {note && dismissed !== note.key ? (
              <div className="settings-note" role="status">
                <span>{note.text}</span>
                <Button small onClick={() => setDismissed(note.key)}>Dismiss</Button>
              </div>
            ) : null}
            {SCOPE_LINE_SECTIONS.has(current) ? <ScopeSentence /> : null}
            {current === "general" ? <p className="settings-scope"><Info size={13} aria-hidden />{shell.data?.attached ? "Desktop app · applies to this computer" : "Applies to this computer"}</p> : null}
            {current === "maintenance" ? <p className="settings-scope"><Info size={13} aria-hidden />Applies to this computer</p> : null}
            <SettingRowIds.Provider value={(title) => settingSearchId(current, title)}>
              <SettingsSubPageContext.Provider value={open}><View key={current} /></SettingsSubPageContext.Provider>
            </SettingRowIds.Provider>
          </div>
        </div>
      </div>
    </section>
  );
}
