import { FolderGit2 } from "lucide-react";
import { useState } from "react";
import type { EngineStatus, PermissionMode } from "../../../api/types";
import { ACCESS, offeredModes, refusalSentence, refusedModes } from "../../../product/composer/access";
import { Button } from "../../../ui/controls";
import { NewThreadDialog, useRepos } from "../../repos";
import { ModelPickerTrigger, useTraitsMenu } from "../../thread";
import type { ModelChoice } from "./draftHelpers";

/** The new-thread dialog's Permission control: the composer's own permission menu, with the modes the engine refuses dimmed. */
function PermissionControl({ engine, access, onAccess }: { engine: EngineStatus; access: PermissionMode; onAccess(mode: PermissionMode): void }) {
  const refused = refusedModes(engine);
  const sentence = refusalSentence(engine.name, refused);
  const [pressed, setPressed] = useState(false);
  const traits = useTraitsMenu({
    model: null, effort: null, extras: {}, access, modes: offeredModes(engine),
    ...(refused.length > 0 ? { refused: { modes: refused, sentence, onPick: () => setPressed(true) } } : {}),
    onEffort: () => undefined, onOption: () => undefined, onAccess,
  });
  return (
    <>
      <Button small aria-haspopup="menu" aria-label={`Permissions: ${ACCESS[access].label}`} icon={(() => { const Icon = ACCESS[access].icon; return <Icon size={14} aria-hidden />; })()}
        onClick={(event) => traits.openAt(event.currentTarget, true)}>
        {ACCESS[access].label}
      </Button>
      {traits.menu}
      {pressed ? <span className="sr-only" role="status">{sentence}</span> : null}
    </>
  );
}

/**
 * The new-thread screen's way into a project that has several repositories: a Repositories button in the context row that opens the
 * repository dialog. It draws nothing unless the service reports two or more repositories for the project, so every other project
 * keeps today's New thread. The Engine and Permission rows are the composer's own pickers, so a choice made in either shows in both.
 */
export function RepoStart({ project, engines, engine, model, access, onModel, onAccess }: {
  project: { id: string; name: string };
  engines: EngineStatus[];
  engine: EngineStatus;
  model: string | null;
  access: PermissionMode;
  onModel(choice: ModelChoice): void;
  onAccess(mode: PermissionMode): void;
}) {
  const { view } = useRepos(project.id);
  const [open, setOpen] = useState(false);
  const count = view?.repositories.length ?? 0;
  if (count < 2) return null;
  const refused = refusedModes(engine);
  const unmanaged = !engine.capabilities.managed;
  return (
    <div>
      <dt className="sr-only">Repositories</dt>
      <dd>
        <button type="button" className="context-action" aria-haspopup="dialog" aria-label={`Repositories: ${count}`} onClick={() => setOpen(true)}>
          <FolderGit2 size={12} aria-hidden />
          {count} repositories
        </button>
        {open ? (
          <NewThreadDialog projectId={project.id} projectName={project.name} start={{ engine: engine.id, model, permission_mode: access }}
            engineControl={<ModelPickerTrigger engines={engines} selected={{ engine: engine.id, model }} onPick={onModel} />}
            permissionControl={unmanaged ? <span>Unmanaged</span> : <PermissionControl engine={engine} access={access} onAccess={onAccess} />}
            note={refused.length > 0 ? refusalSentence(engine.name, refused) : null}
            onClose={() => setOpen(false)} />
        ) : null}
      </dd>
    </div>
  );
}
