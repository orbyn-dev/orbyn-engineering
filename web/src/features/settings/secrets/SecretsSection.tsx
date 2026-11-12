import { Lock, MoreHorizontal, Plus } from "lucide-react";
import { useState } from "react";
import type { RedactionFormat } from "../../../generated/RedactionFormat";
import type { RedactionPattern } from "../../../generated/RedactionPattern";
import type { RedactionSettings } from "../../../generated/RedactionSettings";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { ConfirmDialog } from "../../../ui/Dialog";
import { Menu } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { Button, IconButton } from "../../../ui/controls";
import {
  changeSecretFormat,
  changeSecretProject,
  changeSecretSurfaces,
  deleteSecretPattern,
  readSecrets,
} from "../api";
import { SettingGroup, SettingRow } from "../rows/SettingRow";
import { SettingsBanner } from "../rows/SettingsBanner";
import { heading } from "../sections/parts";
import { PatternDialog } from "./PatternDialog";
import { ProjectsOffDialog } from "./ProjectsOffDialog";
import "./Secrets.css";

const plural = (label: string) => (label.includes("values") ? label : `${label}s`);

/** What turning a format off changes, in words. */
const lookLike = (format: RedactionFormat) =>
  format.id === "named_value" ? "Environment values named like a secret" : `Values that look like ${plural(format.label)}`;

/** The switch hides nothing today: terminal blocks and the agent context are not built. */
const Pending = () => <span className="secrets-pending">Not applied yet</span>;

const appliesTo = (pattern: RedactionPattern) => (pattern.applies_to === "anywhere" ? "Anywhere" : "Environment values");

/** Settings › Secrets: where values are hidden, the formats Orbyn recognises and the person's own patterns. */
export function SecretsSection() {
  const read = useAsync("settings-secrets", (signal) => readSecrets(signal), { keep: true });
  const [latest, setLatest] = useState<RedactionSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [turningOff, setTurningOff] = useState<RedactionFormat | null>(null);
  const [projects, setProjects] = useState(false);
  const [editing, setEditing] = useState<{ pattern: RedactionPattern | null } | null>(null);
  const [removing, setRemoving] = useState<RedactionPattern | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; pattern: RedactionPattern } | null>(null);
  const settings = latest ?? read.data;

  const write = async (change: Promise<RedactionSettings>) => {
    setError(null);
    try {
      setLatest(await change);
    } catch (failure) {
      setError(toFailure(failure).message);
      throw failure;
    }
  };
  const quiet = (change: Promise<RedactionSettings>) => write(change).catch(() => undefined);

  if (!settings) {
    return (
      <>
        {heading("Secrets")}
        {read.error ? (
          <SettingsBanner tone="err" actions={<Button small onClick={read.reload}>Try again</Button>}>
            Secrets could not be read. {read.error.message}
          </SettingsBanner>
        ) : (
          <p className="muted" role="status">Loading…</p>
        )}
      </>
    );
  }

  return (
    <>
      {heading("Secrets")}
      <p className="secrets-intro">
        Orbyn hides values that look like secrets in the Crew ledger and in what Crew stores. Your own patterns also mask
        ledger text when you read it, old rows included. Terminal blocks and what agents read follow later.
      </p>
      <SettingsBanner>
        Hiding in terminal blocks and in what agents read is not applied yet. These choices are saved and take effect when those
        parts arrive. Crew already blocks every format, always.
      </SettingsBanner>
      {error ? <SettingsBanner tone="err">{error}</SettingsBanner> : null}
      <SettingGroup title="Where values are hidden">
        <SettingRow
          title="Terminal blocks"
          description="Replaced by a grey mask."
        >
          <Pending />
          <input
            type="checkbox"
            role="switch"
            aria-label="Terminal blocks"
            checked={settings.terminal}
            onChange={(event) => void quiet(changeSecretSurfaces({ terminal: event.target.checked }))}
          />
        </SettingRow>
        <SettingRow title="What agents read" description="On by default. Switched off per project.">
          <Pending />
        </SettingRow>
        <SettingRow
          title="Crew ledger and Crew objects"
          description="Locked on. Crew blocks every recognised format, whatever is set here, so a recognised secret cannot be stored in the ledger or in a Crew object. It can’t be switched off, for any project."
        >
          <span className="secrets-lock"><Lock size={13} aria-hidden />Always on</span>
        </SettingRow>
        <SettingRow
          title="Projects with redaction off"
          description={
            settings.projects_off.length
              ? `${settings.projects_off.map((project) => project.name).join(", ")}. Agents read their real values.`
              : "None. Redaction is on in every project."
          }
        >
          <Button small onClick={() => setProjects(true)}>Manage</Button>
        </SettingRow>
      </SettingGroup>

      <SettingGroup title="Recognised formats" description="Not applied yet to terminal blocks and what agents read. Crew uses every format whatever is set here.">
        {settings.formats.map((format) => (
          <SettingRow key={format.id} title={format.label} description={format.example}>
            <input
              type="checkbox"
              role="switch"
              aria-label={format.label}
              checked={format.enabled}
              onChange={(event) => (event.target.checked ? void quiet(changeSecretFormat(format.id, true)) : setTurningOff(format))}
            />
          </SettingRow>
        ))}
      </SettingGroup>

      <SettingGroup title="Your patterns" description="Today they apply to the Crew ledger when you read it, old rows included. They do not apply to terminal blocks or what agents read yet.">
        {settings.patterns.length ? (
          <div className="set-tbl" role="table" aria-label="Your patterns">
            <div role="row" className="set-tr h secrets-cols">
              <span role="columnheader">Name</span>
              <span role="columnheader">Type</span>
              <span role="columnheader">Applies to</span>
              <span />
            </div>
            {settings.patterns.map((pattern) => (
              <div key={pattern.id} role="row" className="set-tr secrets-cols">
                <span role="cell" className="name">{pattern.name}</span>
                <span role="cell">{pattern.kind === "text" ? "Text" : "Regular expression"}</span>
                <span role="cell">{appliesTo(pattern)}</span>
                <span role="cell">
                  <IconButton
                    label={`Menu for ${pattern.name}`}
                    aria-haspopup="menu"
                    onClick={(event) => {
                      const box = event.currentTarget.getBoundingClientRect();
                      setMenu({ x: box.left, y: box.bottom + 4, pattern });
                    }}
                  >
                    <MoreHorizontal size={14} aria-hidden />
                  </IconButton>
                </span>
              </div>
            ))}
          </div>
        ) : null}
        <div className="secrets-add">
          <Button small icon={<Plus size={13} aria-hidden />} onClick={() => setEditing({ pattern: null })}>Add pattern</Button>
        </div>
      </SettingGroup>

      <Presence>
        {menu ? (
          <Menu
            x={menu.x}
            y={menu.y}
            label="Pattern actions"
            onClose={() => setMenu(null)}
            items={[
              { label: "Edit", onSelect: () => setEditing({ pattern: menu.pattern }) },
              { label: "Remove", destructive: true, onSelect: () => setRemoving(menu.pattern) },
            ]}
          />
        ) : null}
      </Presence>

      {turningOff ? (
        <ConfirmDialog
          title={`Turn off ${plural(turningOff.label)}?`}
          confirmLabel="Turn off"
          cancelFocus
          onConfirm={() => write(changeSecretFormat(turningOff.id, false))}
          onClose={() => setTurningOff(null)}
        >
          <p>
            {lookLike(turningOff)} will be shown in terminal blocks and may be included in what agents read. You can turn
            this back on at any time. Values already hidden stay hidden.
          </p>
        </ConfirmDialog>
      ) : null}
      {projects ? (
        <ProjectsOffDialog
          off={settings.projects_off}
          onChange={(id, redaction) => write(changeSecretProject(id, redaction))}
          onClose={() => setProjects(false)}
        />
      ) : null}
      {editing ? (
        <PatternDialog
          pattern={editing.pattern}
          limits={settings.limits}
          onSaved={setLatest}
          onClose={() => setEditing(null)}
        />
      ) : null}
      {removing ? (
        <ConfirmDialog
          title={`Remove ${removing.name}?`}
          confirmLabel="Remove"
          tone="danger"
          cancelFocus
          onConfirm={() => write(deleteSecretPattern(removing.id))}
          onClose={() => setRemoving(null)}
        >
          <p>Values that match it will no longer be hidden.</p>
        </ConfirmDialog>
      ) : null}
    </>
  );
}
