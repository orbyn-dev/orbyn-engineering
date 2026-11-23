import { useState } from "react";
import { ChevronDown, Layers } from "lucide-react";
import type { ModelInfo } from "../../../generated/ModelInfo";
import type { TextGeneratorEngine } from "../../../generated/TextGeneratorEngine";
import type { TextModelReason } from "../../../generated/TextModelReason";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { navigate } from "../../../product/state/route";
import { useUi } from "../../../product/state/ui";
import { ErrorState, Loading } from "../../../product/ui/states";
import { Button } from "../../../ui/controls";
import { Menu, type MenuItem } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { listTextGenerators, queryPreferences, updatePreferences } from "../api";
import { SettingRow } from "../rows/SettingRow";
import { useSettingsScope } from "../rows/SettingsScopeContext";
import { settingsLabel } from "../settingsSearch";
import { TEXT_GENERATOR_MODELS, textGeneratorChange } from "./textGeneratorModels";
import "./textGenerators.css";

const modelLabel = (models: ModelInfo[], id: string | null) =>
  id === null ? null : models.find((model) => model.id === id)?.label ?? id;

function sentence(engine: TextGeneratorEngine, resolved: string | null, reason: TextModelReason | null) {
  if (reason === "chosen") return "Your choice";
  if (reason === "legacy") return `${modelLabel(engine.models, resolved) ?? resolved ?? "Title model"} · from Title model`;
  if (reason === "small") return `${modelLabel(engine.models, resolved) ?? resolved ?? "Small model"} · the engine’s small model`;
  if (reason === "thread") return "The thread’s own model writes";
  return engine.models.length === 0
    ? "The engine returned no model list · the thread’s own model writes"
    : "No small model listed · the thread’s own model writes";
}

function Row({ engine, project, chosen, resolved, reason, disabled, error, save }: {
  engine: TextGeneratorEngine;
  project: boolean;
  chosen: string | null;
  resolved: string | null;
  reason: TextModelReason | null;
  disabled: boolean;
  error: string | null;
  save(model: string | null): void;
}) {
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null);
  const chosenLabel = modelLabel(engine.models, chosen);
  const automaticLabel = chosen === null ? modelLabel(engine.models, resolved) : null;
  const projectChoice = project && engine.project?.own === true;
  const items: MenuItem[] = [{
    label: "Automatic",
    selected: chosen === null,
    leadCheck: true,
    content: (
      <span className="menu-planned-row">
        <span>Automatic</span>
        {automaticLabel ? <span className="textgen-menu-right">{automaticLabel}</span> : null}
      </span>
    ),
    onSelect: () => {
      if (chosen !== null) save(null);
    },
  }, ...engine.models.map((model): MenuItem => {
    const tag = model.is_default ? "Default" : model.custom ? "Added in settings" : null;
    return {
      label: model.label,
      selected: chosen === model.id,
      leadCheck: true,
      content: (
        <span className="menu-planned-row">
          <span>{model.label}</span>
          {tag ? <span className="pill">{tag}</span> : null}
        </span>
      ),
      onSelect: () => {
        if (model.id !== chosen) save(model.id);
      },
    };
  })];
  return <>
    <SettingRow title={engine.name} description={sentence(engine, resolved, reason)} error={error ?? undefined}
      actions={projectChoice ? (
        <span className="setting-inheritance-actions" title="Set for this project">
          <span className="icon-btn textgen-scope-icon" role="img" aria-label="Set for this project" data-project-choice="true" data-overridden="true">
            <Layers size={12} aria-hidden />
          </span>
        </span>
      ) : undefined}>
      {engine.models.length === 0 ? (
        <Button small className="textgen-trigger" disabled>No models listed</Button>
      ) : (
        <Button small className="textgen-trigger" aria-haspopup="menu" aria-expanded={anchor !== null} disabled={disabled}
          onClick={(event) => {
            if (anchor) { setAnchor(null); return; }
            const box = event.currentTarget.getBoundingClientRect();
            setAnchor({ x: box.left, y: box.bottom + 4 });
          }}>
          <span className="textgen-trigger-label">{chosenLabel ?? "Automatic"}</span>
          <ChevronDown size={13} aria-hidden />
        </Button>
      )}
    </SettingRow>
    <Presence>
      {anchor ? <Menu x={anchor.x} y={anchor.y} label="Text generator model" items={items} onClose={() => setAnchor(null)} /> : null}
    </Presence>
  </>;
}

/** Settings › Threads: one model per engine for thread titles and source-control text. */
export function TextGenerators() {
  const scope = useSettingsScope();
  const selection = scope?.query.selection;
  const project = selection?.kind === "checkout" ? selection.project_id : null;
  const preferenceScope = scope?.active ? scope.query : {};
  const list = useAsync(`text-generators:${project ?? "global"}`, (signal) => listTextGenerators(project, signal), { keep: true });
  const [error, setError] = useState<{ engine: string; message: string } | null>(null);
  const [saving, setSaving] = useState<string | null>(null);

  const save = (engine: TextGeneratorEngine, model: string | null) => {
    setError(null);
    setSaving(engine.engine);
    void (async () => {
      let wrote = false;
      try {
        const current = await queryPreferences({ scope: preferenceScope, keys: [TEXT_GENERATOR_MODELS] });
        const change = textGeneratorChange(current, preferenceScope, project, engine.engine, model);
        if (!change) return;
        wrote = true;
        await updatePreferences(change);
        useUi.getState().bumpPreferencesRevision();
      } catch (failure) {
        setError({ engine: engine.engine, message: toFailure(failure).message });
      } finally {
        setSaving(null);
        if (wrote) list.reload();
      }
    })();
  };

  const heading = settingsLabel("threads-text-generators");
  return (
    <section className="setting-group text-generators" aria-labelledby="threads-text-generators">
      <h2 id="threads-text-generators" tabIndex={-1}>{heading}</h2>
      {list.status === "loading" ? <Loading label="Loading text generators" rows={3} /> : null}
      {list.status === "error" ? (
        <ErrorState title="Couldn’t load text generators" failure={list.error} onRetry={list.reload} />
      ) : null}
      {list.status === "ready" ? (
        <>
          <div className="setting-group-desc">
            One model per engine writes a thread’s title and the text of a commit or pull request you confirm. Automatic uses the engine’s small model.
          </div>
          {list.data.engines.length === 0 ? (
            <div className="textgen-empty">
              <p className="textgen-empty-title">No engine is on for this project</p>
              <p className="textgen-empty-desc">Titles and commit text need one. Turn an engine on in Providers.</p>
              <Button small onClick={() => navigate({ name: "settings", section: "providers" })}>Open Providers</Button>
            </div>
          ) : (
            <div className="setting-list">
              {list.data.engines.map((engine) => {
                const scoped = project !== null ? engine.project : undefined;
                const chosen = scoped ? scoped.chosen : engine.chosen;
                const resolved = scoped ? scoped.resolved : engine.resolved;
                const reason = scoped ? scoped.reason : engine.reason;
                return <Row key={engine.engine} engine={engine} project={project !== null}
                  chosen={chosen} resolved={resolved} reason={reason}
                  disabled={saving !== null} error={error?.engine === engine.engine ? error.message : null}
                  save={(model) => save(engine, model)} />;
              })}
            </div>
          )}
        </>
      ) : null}
    </section>
  );
}
