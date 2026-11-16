import { useEffect } from "react";
import { useAsync } from "../../lib/useAsync";
import { accountApi } from "./api";
import { onAccountChange } from "./changes";

/** The account's devices as the service lists them; read again when the account changes. */
export function useDevices() {
  const read = useAsync("account-devices", (signal) => accountApi.devices(signal), { keep: true });
  const { reload } = read;
  useEffect(() => onAccountChange(reload), [reload]);
  return read;
}
