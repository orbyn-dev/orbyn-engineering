import type { ComponentType, ReactNode } from "react";
import { unavailable } from "../autopilot/review/Parts";
import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";
import { viewOf } from "./assemble";
import { kindChips, NeedsBar, NeedsCard, NeedsEmpty, NeedsList, CHIP_NOTE, type Chip } from "./NeedsYou";
import { NeedsYouInbox } from "./NeedsYouInbox";
import type { NeedsItem } from "./model";
import { BACKGROUND_ITEMS, DEMOTED_ITEMS, FULL_ITEMS, MAIN_ITEM, QUESTIONS_ITEM, MANY_GROUPS, MANY_ITEMS, NARROW_ITEMS, OFF_ITEMS, STOP_ITEMS } from "./samples";
import type { NeedsYouFrameId } from "./names";

const SUB = "Login redirect crew · Lead: Fix login redirect on Claude Code";
const USAGE = "Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m";
const VIEWS = ["Flow", "Review · 7", "Map", "Board", "Lanes"];

/** The Crew page as drawn around the inbox: title, the Crew views with Inbox chosen, and the usage line. */
function Page({ inbox, workers = 3, children }: { inbox: string; workers?: number; children: ReactNode }) {
  return (
    <div className="cx-cwpg">
      <PageHead sub={`${SUB} · ${workers} workers`} acts />
      <div className="cx-cwtabs">
        <Seg kind="segx" on={5} items={[...VIEWS, inbox, "Report"]} />
        <span className="cx-end">{USAGE}</span>
      </div>
      <div className="ny-root">{children}</div>
    </div>
  );
}

const inboxOf = (items: NeedsItem[]) => `Inbox · ${items.length}`;

function List({ items, errors, plain }: { items: NeedsItem[]; errors?: string[]; plain?: boolean }) {
  const chips = kindChips(viewOf(items), items.length);
  return (
    <>
      <NeedsBar chips={plain ? chips.slice(0, 1) : chips} active="all" rule={CHIP_NOTE} />
      <NeedsList items={items} handlers={{}} errors={errors} />
    </>
  );
}

const listFrame = (items: NeedsItem[], options: { errors?: string[]; plain?: boolean; dialog?: ReactNode } = {}): ComponentType => function NeedsYouList() {
  return (
    <Frame height={options.dialog ? 560 : undefined}>
      <Page inbox={inboxOf(items)}>
        <List items={items} errors={options.errors} plain={options.plain} />
      </Page>
      {options.dialog}
    </Frame>
  );
};

function ApproveDialog() {
  return (
    <div className="cx-cwov">
      <div className="cx-cwdlg" role="dialog" aria-modal="true" aria-label="Approve a command" style={{ width: 460 }}>
        <h4>
          Approve a command <span className="cx-cwpv">Preview</span>
        </h4>
        <p>Claude Code · Sonnet 5.5 asks to run a command in its worktree crew/redirect-impl.</p>
        <pre className="ny-guard"><code>npm publish --dry-run --workspace auth</code></pre>
        <div className="cx-fbx">
          <div className="cx-fkv">
            <span>Waiting holds</span>
            <b>1 task and its lease on auth/redirect.ts</b>
          </div>
        </div>
        <div className="cx-ra">
          <Btn className="cx-ghost">Deny</Btn>
          <Btn className="cx-btn">Approve once</Btn>
        </div>
      </div>
    </div>
  );
}

function Empty() {
  return (
    <Frame>
      <Page inbox="Inbox">
        <NeedsEmpty title="Nothing needs you" action={<Btn className="cx-ghost">Open the Flow view</Btn>}>
          When something needs you, it appears here.
        </NeedsEmpty>
      </Page>
    </Frame>
  );
}

function Many() {
  const chips: Chip[] = [
    { key: "all", label: "All projects", count: 27 },
    { key: "eng", label: "orbyn-engineering", count: 21 },
    { key: "docs", label: "orbyn-docs", count: 6 },
  ];
  return (
    <Frame height={560}>
      <Page inbox="Inbox · 27" workers={9}>
        <NeedsBar chips={chips} active="all" rule="Showing 4 of 27 · the rest are grouped below" label="Filter by project" />
        <NeedsList items={MANY_ITEMS} handlers={{}} />
        <ul className="ny-groups" aria-label="The rest, by crew">
          {MANY_GROUPS.map((group) => (
            <li key={group.name}>
              <button type="button" className="ny-group" aria-expanded={false} {...unavailable}>
                <Ic n="right" sm />
                {group.name}
                <span className="ny-n">{group.count}</span>
              </button>
            </li>
          ))}
        </ul>
      </Page>
    </Frame>
  );
}

/** The 320 px panel's head: the same Crew tabs as the other panel frames, with Inbox chosen. */
function PanelHead() {
  const tabs = [["Changes", "diff"], ["Activity", "activity"], ["Side chat", "side"], ["Crew", "flow"]] as const;
  return (
    <>
      <div className="cx-ptabs cx-btabs cx-ico">
        {tabs.map(([label, icon]) => (
          <span key={label} className={label === "Crew" ? "cx-ptab cx-on" : "cx-ptab"} aria-current={label === "Crew" ? "true" : undefined} aria-label={label} title={label}>
            <Ic n={icon} sm />
          </span>
        ))}
        <span className="cx-padd" aria-label="Open a tab"><Ic n="plus" sm /></span>
        <span className="cx-end" style={{ marginLeft: "auto", display: "flex" }}>
          <Btn className="cx-icb" aria-label="Maximise panel"><Ic n="max" sm /></Btn>
          <Btn className="cx-icb" aria-label="Close side panel"><Ic n="x" sm /></Btn>
        </span>
      </div>
      <div className="cx-phd2">
        <b>Crew</b>
        <span className="cx-cwpv">Preview</span>
        <span className="cx-pha">
          <Btn className="cx-ghost" style={{ height: "24px", fontSize: "12px", padding: "0 9px" }}>Open in Crew</Btn>
          <Btn className="cx-icb" aria-label="Tab menu"><Ic n="more" sm /></Btn>
        </span>
      </div>
      <div className="cx-cwpseg" style={{ padding: "8px 10px", flex: "none" }}>
        <span className="cx-segx" aria-label="Crew views">
          {["Flow", "Review", "Map", "Board", "Inbox 4"].map((name) => (
            <span key={name} className={name === "Inbox 4" ? "cx-on" : undefined} aria-current={name === "Inbox 4" ? "true" : undefined}>{name}</span>
          ))}
          <Btn className="cx-ovf" aria-label="More Crew views" title="More Crew views"><Ic n="more" sm /></Btn>
        </span>
      </div>
    </>
  );
}

function NarrowPanel() {
  return (
    <Frame panel>
      <PanelHead />
      <ol className="ny-cards" aria-label="Needs you">
        {NARROW_ITEMS.map((item) => <NeedsCard key={item.id} item={item} handlers={{}} />)}
      </ol>
    </Frame>
  );
}

const RULES: [string, string, string][] = [
  ["1 · Risk", "What could go wrong if the answer is wrong: a command that touches the network, a deletion, a merge.", "“A change waiting for your review”: High"],
  ["2 · Reversibility", "How easily the result can be undone: a branch can be reset; a publish can’t.", "“Merge to main”: Hard to undo"],
  ["3 · Cost of waiting", "What stays blocked while it waits: tasks, leases, workers, and whether any engine is idle on a paid plan.", "“Holds 2 tasks and 1 lease”"],
  ["Batching", "Questions from one worker arrive together; low-risk ones wait up to 10 minutes before they interrupt.", "“3 questions from OpenCode”"],
];

function Budget() {
  return (
    <Frame>
      <section className="ny-sheet" aria-label="How the inbox is ranked">
        <h2>
          How the inbox is ranked <span className="cx-cwpv">Preview</span>
        </h2>
        <p>Crew puts what matters most at the top and batches what can wait, so the person is interrupted less and never surprised.</p>
        <div className="ny-rules" role="table" aria-label="How the inbox is ranked">
          <div role="row" style={{ display: "contents" }}>
            {["Ranked by", "What it means", "Example"].map((head) => <div key={head} role="columnheader" data-head>{head}</div>)}
          </div>
          {RULES.map(([name, means, example]) => (
            <div key={name} role="row" style={{ display: "contents" }}>
              <div role="cell"><b>{name}</b></div>
              <div role="cell">{means}</div>
              <div role="cell">{example}</div>
            </div>
          ))}
        </div>
        <div className="ny-box">
          <b>What you control</b>
          <p>How long low-risk questions may wait before they interrupt (up to 10 minutes by default, set in Settings › Crew), and quiet hours. High-risk items (irreversible, security, money or data loss) interrupt at once, and neither the wait nor quiet hours holds them back.</p>
        </div>
      </section>
    </Frame>
  );
}

/** The inbox on the app's own routes, inside the Crew page as the designs place it. Needs a running service. */
function Live() {
  return (
    <Frame height={560}>
      <Page inbox="Inbox">
        <NeedsYouInbox />
      </Page>
    </Frame>
  );
}

/** Each drawn frame, on sample data. Loaded only through the Preview route. */
export const NEEDSYOU_SCREENS: Record<NeedsYouFrameId, ComponentType> = {
  "needsyou-inbox": listFrame(FULL_ITEMS),
  "needsyou-inbox-off": listFrame(OFF_ITEMS),
  "needsyou-approval": listFrame(OFF_ITEMS, { dialog: <ApproveDialog /> }),
  "needsyou-empty": Empty,
  "needsyou-many": Many,
  "needsyou-narrow-panel": NarrowPanel,
  "needsyou-budget": Budget,
  "needsyou-demoted": listFrame(DEMOTED_ITEMS),
  "needsyou-stop": listFrame(STOP_ITEMS),
  "needsyou-background": listFrame(BACKGROUND_ITEMS),
  "needsyou-source-error": listFrame([QUESTIONS_ITEM, MAIN_ITEM], { errors: ["held commands"], plain: true }),
  "needsyou-live": Live,
};
