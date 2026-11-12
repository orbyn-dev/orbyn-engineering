import { createContext, useContext } from "react";

/** Closes the Details popover once a row has opened something elsewhere (a tab, a page, an editor), by the shared focus rule. */
export const DetailsDismiss = createContext<() => void>(() => undefined);
export const useDismissDetails = () => useContext(DetailsDismiss);
