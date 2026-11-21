import type { ReactNode } from "react";

/**
 * One row of Settings › Crew that opens a page of its own (Crew Designs v87, frame `settings-crew`): the title, what the page is
 * for, and the button that opens it. `planned` draws the Planned pill beside the title for a row whose page does not exist; its
 * button is then disabled by the caller. Nothing is read or sent by the row.
 */
export function EntryRow({ id, title, planned, children, action }: { id: string; title: string; planned?: boolean; children: ReactNode; action: ReactNode }) {
  return (
    <section className="ar-set ar" aria-labelledby={id}>
      <div className="ar-setrow">
        <div>
          <b id={id}>
            {title}
            {planned ? <span className="ar-pv" style={{ marginLeft: "8px" }} title="Planned">Planned</span> : null}
          </b>
          <small>{children}</small>
        </div>
        {action}
      </div>
    </section>
  );
}
