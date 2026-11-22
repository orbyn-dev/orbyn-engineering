import { useEffect, useState } from "react";
import { useUi } from "../../../../product/state/ui";
import { setAttentionApi } from "./contract";
import { TerminalAttention } from "./TerminalAttention";
import { useAttention } from "./store";
import { useTerminalAttention } from "./useAttention";
import "./preview.css";

/** The mock's scenario words, kept here so the links show before the mock has loaded. */
const SCENARIOS = ["default", "yesno", "secret", "many", "cleared", "hostile"] as const;

/**
 * Dev-only page for the terminal-agent attention item (`#/preview/attention`, with `?yesno`, `?secret`, `?many`, `?cleared` or
 * `?hostile`, and `&narrow` for a narrow thread). It runs against the mock client and has no terminal: Open terminal only says what it
 * would have focused, and nothing is typed. Nothing here is in a production build.
 */
export default function AttentionPreview() {
  const hash = window.location.hash;
  const scenario = SCENARIOS.find((name) => hash.includes(`?${name}`)) ?? "default";
  const narrow = hash.includes("narrow");
  const [said, setSaid] = useState("Nothing opened or written yet.");
  const [installed, setInstalled] = useState<string | null>(null);
  useEffect(() => {
    document.title = "Attention preview";
    let live = true;
    useAttention.getState().reset();
    useUi.setState({ toasts: [] });
    void import("../../../../../mock/attention").then((module) => {
      if (!live) return;
      setAttentionApi(module.createAttentionMock({ scenario }));
      setInstalled(scenario);
    });
    return () => {
      live = false;
      setAttentionApi(null);
      useAttention.getState().reset();
    };
  }, [scenario]);
  return (
    <main className="atp">
      <nav className="atp-bar" aria-label="Scenarios">
        {SCENARIOS.map((name) => (
          <a key={name} href={`#/preview/attention${name === "default" ? "" : `?${name}`}${narrow ? (name === "default" ? "?narrow" : "&narrow") : ""}`} aria-current={name === scenario ? "page" : undefined}>
            {name}
          </a>
        ))}
        <a href={`#/preview/attention${scenario === "default" ? "" : `?${scenario}`}${narrow ? "" : scenario === "default" ? "?narrow" : "&narrow"}`}>{narrow ? "wide" : "narrow"}</a>
      </nav>
      <div className="atp-thread" data-narrow={narrow || undefined}>
        {installed === scenario ? <Live onOpen={(text) => setSaid(text)} /> : null}
      </div>
      <p className="atp-said" role="status">{said}</p>
    </main>
  );
}

function Live({ onOpen }: { onOpen(text: string): void }) {
  useTerminalAttention(null);
  return <TerminalAttention threadId="preview" onOpen={(target) => onOpen(`Would focus terminal ${JSON.stringify(target.terminalId)} of this thread. Nothing was written to it.`)} />;
}
