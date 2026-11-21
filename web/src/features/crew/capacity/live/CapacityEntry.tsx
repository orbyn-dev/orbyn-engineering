import { navigate } from "../../../../product/state/route";
import { Button } from "../../../../ui/controls";
import { EntryRow } from "../../shared/EntryRow";

/**
 * The Review capacity row of Settings › Crew (Crew Designs v23, `settings-crew`): the title, what the setting is for and the
 * button that opens the page. The design's value ("Not set") is not drawn: the page reads it from the service, and a row that
 * read it here would need a project this page does not pick. Opening sends and reads nothing; the page reads on its own.
 */
export function CapacityEntry() {
  return (
    <EntryRow id="cap-entry-title" title="Review capacity" action={<Button small onClick={() => navigate({ name: "settings", section: "crew", crewPage: "capacity" })}>Open Review capacity</Button>}>
      How many High and Medium changes you can have waiting before Crew pauses new Low-priority starts, and whether low-risk batches are sampled.
    </EntryRow>
  );
}
