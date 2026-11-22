import { useEffect, useState } from "react";
import { setBlocksApi } from "./blocksContract";
import { CommandBlocks } from "./CommandBlocks";
import "./preview.css";

/** The mock's scenario words, kept here so the links show before the mock has loaded. */
const SCENARIOS = ["default", "failed", "running", "long", "fullscreen", "many", "empty", "error"] as const;

/**
 * Dev-only page for the command blocks (`#/preview/command-blocks`, with `?failed`, `?running`, `?long`, `?fullscreen`, `?many`,
 * `?empty` or `?error`, and `&narrow` for a narrow drawer). It runs against the mock client rather than the service and has no terminal:
 * Run again and Stop show what they would have written. Nothing here is in a production build.
 */
export default function CommandBlocksPreview() {
  const hash = window.location.hash;
  const scenario = SCENARIOS.find((name) => hash.includes(`?${name}`)) ?? "default";
  const narrow = hash.includes("narrow");
  const [written, setWritten] = useState("Nothing written to the terminal yet.");
  // Each scenario gets its own mock; the blocks wait for it so they never read a stale one.
  const [installed, setInstalled] = useState<string | null>(null);
  useEffect(() => {
    document.title = "Command blocks preview";
    let live = true;
    void import("../../../../../mock/commandBlocks").then((module) => {
      if (!live) return;
      setBlocksApi(module.createCommandBlocksMock({ delayMs: 250, scenario }));
      setInstalled(scenario);
    });
    return () => {
      live = false;
      setBlocksApi(null);
    };
  }, [scenario]);
  const link = (name: string, wide: boolean) => `#/preview/command-blocks${name === "default" ? "" : `?${name}`}${wide ? "" : name === "default" ? "?narrow" : "&narrow"}`;
  return (
    <main className="cbp">
      <nav className="cbp-bar" aria-label="Scenarios">
        {SCENARIOS.map((name) => (
          <a key={name} href={link(name, !narrow)} aria-current={name === scenario ? "page" : undefined}>
            {name}
          </a>
        ))}
        <a href={link(scenario, narrow)}>{narrow ? "wide" : "narrow"}</a>
      </nav>
      <div className="cbp-drawer" data-narrow={narrow || undefined}>
        {installed === scenario ? (
          <CommandBlocks
            key={scenario}
            threadId="preview"
            terminalId="main"
            write={(text) => setWritten(`Written to the terminal input, no newline: ${JSON.stringify(text)}`)}
            interrupt={() => setWritten("Ctrl-C written to the terminal.")}
          />
        ) : null}
      </div>
      <p className="cbp-written" role="status">{written}</p>
    </main>
  );
}
