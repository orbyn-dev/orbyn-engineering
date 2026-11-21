import { navigate } from "../../../../product/state/route";
import { Button } from "../../../../ui/controls";
import { EntryRow } from "../../shared/EntryRow";

/**
 * The other three entry rows of Settings › Crew beside Review capacity (Crew Designs v87, `settings-crew`, `entry-crew-settings-narrow-window`).
 * Each opens a page or, for the oracle, nothing: opening sends and reads nothing, and the page reads on its own. The designs' Escapes value
 * ("Defaults") is not drawn: the owner dropped it, and the row has no value to read.
 */
export function EscapesEntry() {
  return (
    <EntryRow id="escapes-entry-title" title="Escapes" action={<Button small aria-label="Open Escapes" onClick={() => navigate({ name: "settings", section: "crew", crewPage: "escapes" })}>Open</Button>}>
      Every revert, and every bug found after a merge that you don’t revert, is a possible escape. You confirm it; from a confirmed escape you can ask Crew to propose a check. Nothing runs until you accept it.
    </EntryRow>
  );
}

/** Independent oracle is Planned: no code reads or writes it, so its button is drawn disabled and says why. */
export function OracleEntry() {
  const why = "Planned: no code reads or writes the independent oracle yet";
  return (
    <EntryRow
      id="oracle-entry-title"
      title="Independent oracle"
      planned
      action={<Button small disabled title={why} aria-description={why}>Open Independent oracle</Button>}
    >
      Who writes the answer key (an engine other than the builder) and whether a mutation gate checks that the key can catch a break.
    </EntryRow>
  );
}

export function BacktestEntry() {
  return (
    <EntryRow id="backtest-entry-title" title="Reviewer backtest" action={<Button small onClick={() => navigate({ name: "settings", section: "crew", crewPage: "backtest" })}>Open Reviewer backtest</Button>}>
      Replay your review lanes and checks on merged pull requests whose outcome is known, to see what each would have caught. The live runner is not approved, so a backtest can’t start yet.
    </EntryRow>
  );
}
