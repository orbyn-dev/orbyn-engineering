import type { ReactNode } from "react";
import "./PageBar.css";
import type { Route } from "../state/route";
import { NarrowBack } from "./states";

/** The heading line of a page: its title and sub line on the left, its actions on the right. */
export function PageBar({ children, actions, back }: { children: ReactNode; actions?: ReactNode; back?: Route }) {
  return (
    <header className="topbar page-bar">
      <NarrowBack to={back} />
      <div className="crumbs">{children}</div>
      {actions ? <div className="page-actions">{actions}</div> : null}
    </header>
  );
}
