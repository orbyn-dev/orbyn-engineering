import { useEffect } from "react";
import { useAsync } from "../../lib/useAsync";
import { accountApi } from "./api";
import { onAccountChange } from "./changes";

/**
 * The account as the service reports it. Read again when the service says it changed
 * (`account`) or this page changed it, and kept so a window shows the last status at once while it reads.
 */
export function useAccount() {
  // An event or a person's refresh reads past any read in flight: its answer may be the older one.
  const read = useAsync("account", (signal, how) => accountApi.view(signal, how.background || how.force), {
    keep: true,
    refreshOn: (what) => what === "account",
  });
  // An account change is an event even when this page caused it: it reads past any read in
  // flight, and it reports a failure over the account still on screen, as a mount read does.
  const { reloadNow } = read;
  useEffect(() => onAccountChange(reloadNow), [reloadNow]);
  return read;
}
