import { Ellipsis } from "lucide-react";
import { useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { useWorkspace } from "../../product/state/workspace";
import { Button, IconButton, Pill } from "../../ui/controls";
import { ConfirmDialog } from "../../ui/Dialog";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { profilesApi, useProfiles } from "./api";
import type { Profile, ProfileDraft } from "./contract";
import { ProfileEditor, type EngineOption } from "./ProfileEditor";
import { availabilityPill, availabilityText, draftOf, duplicateName, emptyDraft, plain, profileLine, usable } from "./model";
import "./profiles.css";

type Open = { kind: "edit"; profile: Profile } | { kind: "new"; seed: ProfileDraft } | { kind: "delete"; profile: Profile };

/** The engine's display name: from the installed list, else the id as given, cut. `null` is "Keep the thread's engine". */
export function useEngineOptions(): { options: EngineOption[]; name(id: string | null): string } {
  const engines = useWorkspace((state) => state.engines.data);
  const options = (engines ?? []).filter((engine) => engine.installed).map((engine) => ({ id: engine.id, name: engine.name }));
  const names = new Map((engines ?? []).map((engine) => [engine.id, engine.name] as const));
  return { options, name: (id) => (id === null ? "Keep the thread’s engine" : (names.get(id) ?? plain(id, 40))) };
}

/**
 * Settings › Crew: execution profiles (frames `profile-list`, `profile-editor`, `profile-duplicate`, `profile-invalid`, `profile-in-use`,
 * `profile-narrow`; docs/execution-profiles-and-library.md, Part 1). Hidden entirely when the service has no profiles route. Built-ins
 * are listed first and are read-only: Duplicate makes an editable copy. A profile whose engine is not startable stays in the list with
 * its reason; Orbyn never swaps the engine. A profile is copied on apply, so deleting one never changes a thread or crew.
 * Planned, not drawn: Default profile and Make default (no default in the contract), the count of threads that use a profile
 * before deleting it (no read gives it), and the Sign in link on a profile whose engine is signed out.
 */
export function ProfilesSettings() {
  const { list, error, settled, reload } = useProfiles();
  const { options, name } = useEngineOptions();
  const [open, setOpen] = useState<Open | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; profile: Profile } | null>(null);
  const [notice, setNotice] = useState("");
  if (list === null && !error) return settled ? null : <section className="prf" aria-busy="true" aria-label="Execution profiles" />;
  if (list === null) return <section className="prf"><p className="field-error" role="alert">{plain(error?.message, 200)}</p></section>;

  const profiles = [...list.profiles].sort((a, b) => Number(b.builtin) - Number(a.builtin));
  const saved = profiles.filter((one) => !one.builtin).length;
  const atLimit = saved >= list.limit;
  const finish = (text: string) => { setOpen(null); setNotice(text); reload(); };

  const items = (profile: Profile): MenuItem[] => [
    ...(profile.builtin ? [] : [{ label: "Edit", onSelect: () => setOpen({ kind: "edit", profile }) }]),
    { label: "Duplicate", disabled: atLimit, title: atLimit ? `You have ${list.limit} saved profiles. Delete one to add another.` : undefined, onSelect: () => setOpen({ kind: "new", seed: { ...draftOf(profile), name: duplicateName(profile.name, profiles.map((one) => one.name)) } }) },
    ...(profile.builtin ? [] : [{ label: "Delete", destructive: true, separatorBefore: true, onSelect: () => setOpen({ kind: "delete", profile }) }]),
  ];

  return (
    <section className="prf" aria-label="Execution profiles">
      <div className="prf-head">
        <div>
          <h2 className="prf-title">Execution profiles</h2>
          <p className="field-hint">A profile is a named preset: the engine, the model tier, the permission mode and the Autopilot mode. Pick one for a thread or for a crew. It is copied when you apply it: changing or deleting it later changes nothing that already used it.</p>
        </div>
        <Button small disabled={atLimit} title={atLimit ? `You have ${list.limit} saved profiles. Delete one to add another.` : undefined} onClick={() => setOpen({ kind: "new", seed: emptyDraft() })}>Add profile</Button>
      </div>
      {notice ? <p className="prf-note" role="status">{notice}</p> : null}
      <ul className="prf-list" aria-label="Profiles">
        {profiles.map((profile) => {
          const state = availabilityText(profile.availability, name(profile.engine));
          const pill = availabilityPill(profile.availability);
          return (
            <li className="prf-row" key={profile.id} data-usable={usable(profile) || undefined}>
              <div className="prf-main">
                <span className="prf-name">{plain(profile.name, 60)}</span>
                {profile.builtin ? <Pill>Built-in</Pill> : null}
                {pill ? <Pill tone="warn">{pill}</Pill> : null}
              </div>
              <span className="prf-line">{profileLine(profile, name(profile.engine))}</span>
              {profile.description ? <span className="prf-desc">{plain(profile.description, 200)}</span> : null}
              {state ? <span className="prf-reason">{state.text}{state.fix ? " Edit the profile to pick another engine." : ""}</span> : null}
              <IconButton label={`Actions for ${plain(profile.name, 40)}`} className="prf-more" aria-haspopup="menu" onClick={(event) => { const box = event.currentTarget.getBoundingClientRect(); setMenu({ x: box.right, y: box.bottom + 4, profile }); }}>
                <Ellipsis size={16} aria-hidden />
              </IconButton>
            </li>
          );
        })}
      </ul>
      <p className="prf-planned"><Pill>Default profile · Planned</Pill></p>
      <Presence>
        {menu ? <Menu x={menu.x} y={menu.y} origin="top left" label={`Profile ${plain(menu.profile.name, 40)}`} items={items(menu.profile)} onClose={() => setMenu(null)} /> : null}
      </Presence>
      {open?.kind === "new" || open?.kind === "edit" ? (
        <ProfileEditor
          profile={open.kind === "edit" ? open.profile : null}
          seed={open.kind === "edit" ? draftOf(open.profile) : open.seed}
          others={profiles}
          engines={options}
          onSaved={(profile) => finish(`Saved ${plain(profile.name, 40)}.`)}
          onClose={() => setOpen(null)}
          onReload={() => { setOpen(null); setNotice("The list is up to date. Open the profile again to see what changed."); reload(); }}
        />
      ) : null}
      {open?.kind === "delete" ? (
        <DeleteProfile profile={open.profile} onDone={() => finish(`Deleted ${plain(open.profile.name, 40)}.`)} onGone={() => finish("That profile was already gone. The list is up to date.")} onClose={() => setOpen(null)} />
      ) : null}
    </section>
  );
}

function DeleteProfile({ profile, onDone, onGone, onClose }: { profile: Profile; onDone(): void; onGone(): void; onClose(): void }) {
  const pending = useRef(false);
  return (
    <ConfirmDialog
      title={`Delete ${plain(profile.name, 40)}?`}
      confirmLabel="Delete"
      tone="danger"
      cancelFocus
      keepOpen
      onClose={onClose}
      onConfirm={async () => {
        if (pending.current) return;
        pending.current = true;
        try {
          await profilesApi().remove(profile.id);
          onDone();
        } catch (error) {
          if (error instanceof ApiFailure && error.code === "not_found") { onGone(); return; }
          throw error;
        } finally {
          pending.current = false;
        }
      }}
    >
      <p>Threads and crews that used it keep the choices they started with. This can’t be undone.</p>
    </ConfirmDialog>
  );
}
