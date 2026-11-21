import { Layers } from "lucide-react";
import { useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import type { Thread } from "../../api/types";
import { useWorkspace } from "../../product/state/workspace";
import { ConfirmDialog } from "../../ui/Dialog";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { profilesApi, useProfiles, useProfilesOffered } from "./api";
import type { ProfileGrant, Profile } from "./contract";
import { useEngineOptions } from "./ProfilesSettings";
import { availabilityText, grantsFromFailure, grantsOf, grantText, plain, profileLine, usable } from "./model";
import "./profiles.css";

/** What a thread last got from a profile, kept for this session only: the contract has no read for it. */
interface Application { name: string; model: string | undefined; mode: string | undefined; engine: string | undefined }
const applied = new Map<string, Application>();
/** For tests and a swapped client: the chip forgets what it last applied. */
export const forgetApplications = (): void => applied.clear();

const FAILURE_TEXT = new Map<string, string>([
  ["revision_conflict", "That profile changed. Open the menu again to see what it sets now."],
  ["run_active", "The thread’s engine can’t change during a run. Wait for the turn to end, then apply the profile."],
  ["not_found", "That profile or thread is gone."],
]);

/**
 * The composer's profile picker (frame `profile-picker-composer`; docs/execution-profiles-and-library.md, "Where a profile can be
 * applied"). Hidden unless the service has the profiles route. Choosing a profile is one Person write that sets the thread's engine,
 * model and permission mode once; the picks beside it then show the result. A profile that grants full access or Autopilot Full asks
 * for one confirm that lists the grants, every time. A profile whose engine is not startable is disabled with its reason: Orbyn
 * never falls back to another engine. The chip reads "Name, edited" once the thread's model or permission mode differs from what
 * the profile set. Planned: the chip after a reload (no read gives the thread's profile), and the picker in the folded narrow bar.
 */
export function ProfilePicker(props: { thread: Thread; disabled: boolean; onError(message: string | null): void }) {
  // The list is read only once the service has answered the probe, so a service without the feature is asked once, not per composer.
  return useProfilesOffered() ? <PickerBody {...props} /> : null;
}

function PickerBody({ thread, disabled, onError }: { thread: Thread; disabled: boolean; onError(message: string | null): void }) {
  const { list, reload } = useProfiles();
  const { name: engineName } = useEngineOptions();
  const refreshThread = useWorkspace((state) => state.refreshThread);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [confirm, setConfirm] = useState<{ profile: Profile; grants: ProfileGrant[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const [, bump] = useState(0);
  if (!list) return null;

  const mine = applied.get(thread.id);
  const edited = mine !== undefined && (thread.model !== mine.model || thread.lifecycle?.permission_mode !== mine.mode);
  const chip = mine ? (edited ? `${plain(mine.name, 24)}, edited` : plain(mine.name, 24)) : "No profile";

  const apply = async (profile: Profile, ack: ProfileGrant[]) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    onError(null);
    try {
      const answer = await profilesApi().applyToThread(thread.id, { profile_id: profile.id, seen_revision: profile.revision, ...(ack.length > 0 ? { ack_grants: ack } : {}) });
      applied.set(thread.id, { name: profile.name, model: answer.applied.model, mode: answer.applied.permission_mode, engine: answer.applied.engine });
      await refreshThread(thread.id);
      // The thread now carries the values the service set; the chip compares against them.
      const fresh = useWorkspace.getState().threads[thread.id];
      applied.set(thread.id, { name: profile.name, model: fresh?.model ?? answer.applied.model, mode: fresh?.lifecycle?.permission_mode ?? answer.applied.permission_mode, engine: answer.applied.engine });
      bump((n) => n + 1);
    } catch (error) {
      if (error instanceof ApiFailure && error.code === "confirm_required" && ack.length === 0) {
        const named = grantsFromFailure(error.detail);
        setConfirm({ profile, grants: named.length > 0 ? named : grantsOf(profile) });
      } else if (error instanceof ApiFailure) {
        if (error.code === "revision_conflict") reload();
        const known = FAILURE_TEXT.get(error.code);
        onError(`Couldn’t apply ${plain(profile.name, 40)}. ${known ?? plain(error.message, 200)}`);
      } else {
        onError(`Couldn’t apply ${plain(profile.name, 40)}. Something went wrong.`);
      }
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  const choose = (profile: Profile) => {
    const grants = grantsOf(profile);
    if (grants.length > 0) setConfirm({ profile, grants });
    else void apply(profile, []);
  };

  const items: MenuItem[] = [...list.profiles].sort((a, b) => Number(b.builtin) - Number(a.builtin)).map((profile) => {
    const state = availabilityText(profile.availability, engineName(profile.engine));
    const ok = usable(profile);
    return {
      label: plain(profile.name, 40),
      selected: mine?.name === profile.name,
      disabled: !ok,
      title: ok ? undefined : `${state?.text ?? "This engine can’t run now."} Edit the profile to pick another engine.`,
      content: <span className="prf-pick"><b>{plain(profile.name, 40)}</b><small>{profileLine(profile, engineName(profile.engine))}</small></span>,
      onSelect: () => choose(profile),
    };
  });

  return (
    <>
      <button type="button" className="composer-control prf-chip" aria-haspopup="menu" aria-label={`Profile: ${chip}`} disabled={disabled || busy}
        onClick={(event) => { const box = event.currentTarget.getBoundingClientRect(); setMenu({ x: box.left, y: box.top - 4 }); }}>
        <Layers size={14} aria-hidden /><span>{chip}</span>
      </button>
      <Presence>
        {menu ? <Menu x={menu.x} y={menu.y} origin="bottom left" label="Profiles" items={items} footer="Choosing a profile sets the engine, model and permission mode for this thread. You can still change each one." onClose={() => setMenu(null)} /> : null}
      </Presence>
      {confirm ? (
        <ConfirmDialog
          title={`Apply ${plain(confirm.profile.name, 40)}?`}
          confirmLabel="Apply"
          tone="danger"
          cancelFocus
          onClose={() => setConfirm(null)}
          onConfirm={async () => { const held = confirm; setConfirm(null); await apply(held.profile, held.grants); }}
        >
          <p>This profile grants:</p>
          <ul className="prf-grants">{confirm.grants.map((code) => <li key={code}>{grantText(code)}</li>)}</ul>
          <p className="field-hint">You are asked again the next time you apply it.</p>
        </ConfirmDialog>
      ) : null}
    </>
  );
}
