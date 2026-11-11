import { useEffect } from "react";
import { useAsync } from "../../lib/useAsync";
import { accountApi } from "./api";
import { onAccountChange } from "./changes";

/**
 * The account as the service reports it. Read again when the service says it changed
 * (`account`) or this page changed it, and kept so a window shows the last status at once while it reads.
 */
export function useAccount() {
  const read = useAsync("account", (signal) => accountApi.view(signal), {
    keep: true,
    refreshOn: (what) => what === "account",
  });
  const { reload } = read;
  useEffect(() => onAccountChange(reload), [reload]);
  return read;
}
