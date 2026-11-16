import { settingsLabel } from "../settingsSearch";
import { useEffect, useState } from "react";
import { Segmented } from "../../../ui/Segmented";
import { SettingGroup, SettingRow } from "../rows/SettingRow";
import { PreferenceGroups } from "../rows/PreferenceRows";
import { WorktreeCleanup } from "../maintenance/WorktreeCleanup";
import { heading } from "./parts";

const NAMING_MODES = [
  { value: "static", label: "Static", description: "The prefix plus a fixed name." },
  { value: "semantic", label: "From the thread", description: "Named from the thread automatically." },
  { value: "custom", label: "Custom", description: "Named using your branch name instructions." },
];

/** The three ways a branch is named; the row's description says what the chosen one does. */
function BranchNaming({ value, save }: { value: string; save(next: string): void }) {
  return <Segmented label="Branch naming" value={value} options={NAMING_MODES.map(({ value, label }) => ({ value, label }))} onChange={save} />;
}

/** The prefix without its slash; the row's description shows the branch name it makes. */
function BranchPrefix({ value, save, disabled, mixed }: { value: string; save(next: string): void; disabled: boolean; mixed?: boolean }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => { setDraft(value); }, [value]);
  const clean = draft.trim().replace(/\/+$/, "");
  return <input aria-label="Branch prefix" className="mono" spellCheck={false} disabled={disabled} value={draft} placeholder={mixed ? "Mixed" : undefined}
    onChange={(event) => setDraft(event.target.value)}
    onBlur={() => { if (clean !== value) save(clean); }}
    onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); event.currentTarget.blur(); } }} />;
}

export function WorktreesSection() {
  return (
    <>
      {heading("Worktrees")}
      <SettingGroup>
        <SettingRow title="Worktree folder" description="Next to each project"><span className="mono set-muted">&lt;project&gt;-wt/&lt;purpose&gt;</span></SettingRow>
      </SettingGroup>
      <PreferenceGroups
        groups={[
          {
            title: "",
            rows: [
              { key: "new_worktrees_start_from_origin", title: settingsLabel("worktrees-start-from-origin"), description: "Base new worktrees on the remote branch, not the local one" },
              {
                key: "worktree_submodules",
                title: settingsLabel("worktrees-submodules"),
                description: "Initialise submodules in each new worktree",
                options: [
                  { value: "recursive", label: "Recursive" },
                  { value: "top-level", label: "Top level only" },
                  { value: "none", label: "Skip" },
                ],
              },
              {
                key: "branch_naming_mode",
                title: settingsLabel("worktrees-naming"),
                describe: (valueOf) => NAMING_MODES.find((mode) => mode.value === valueOf("branch_naming_mode"))?.description,
                render: (value, save) => <BranchNaming value={String(value ?? "")} save={save} />,
              },
              {
                key: "branch_name_prefix",
                title: settingsLabel("worktrees-branch-prefix"),
                describe: (valueOf) => {
                  const prefix = String(valueOf("branch_name_prefix") ?? "").trim().replace(/\/+$/, "");
                  return `Added in front of every branch Orbyn creates. A / is added after the prefix${prefix ? `, so ${prefix} gives ${prefix}/fix-login` : ""}.`;
                },
                render: (value, save, control) => <BranchPrefix value={String(value ?? "")} save={save} disabled={control.disabled} mixed={control.mixed} />,
              },
              { key: "branch_name_instructions", title: settingsLabel("worktrees-branch-name-instructions"), description: "Extra guidance for the names Orbyn generates", multiline: true, placeholder: "Optional. For example: keep names under 40 characters.",
                showWhen: (valueOf) => valueOf("branch_naming_mode") === "custom" },
            ],
          },
        ]}
      />
      <WorktreeCleanup />
    </>
  );
}
