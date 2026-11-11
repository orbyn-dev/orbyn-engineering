import { useState } from "react";
import { Button } from "../../../../ui/controls";
import { navigate } from "../../../../product/state/route";
import { crewApi } from "./api";
import { MODE_COPY } from "./copy";
import { useAutopilotSettings } from "./useCrew";

/** A new repo is on Full by default; until the person has changed or kept it, one line says so. */
export function FirstRunBanner() {
  const read = useAutopilotSettings();
  const [failed, setFailed] = useState(false);
  if (read.status !== "ready" || read.data.confirmed) return null;
  const { repo, mode } = read.data;
  const name = MODE_COPY[mode].name;
  return (
    <div className="ap-banner ap-banner-plain" role="region" aria-label="Autopilot">
      <p>
        <b>Autopilot is {name} for {repo}.</b>{" "}
        {mode === "full"
          ? "The lead answers agents’ requests, verified work merges by itself, and you review what reached main in the Review tab."
          : MODE_COPY[mode].text}
      </p>
      <span className="ap-banner-acts">
        <Button small onClick={() => navigate({ name: "settings", section: "crew" })}>Change</Button>
        <Button
          small
          onClick={() => {
            setFailed(false);
            crewApi.confirmMode().then(read.reload, () => setFailed(true));
          }}
        >
          Keep {name}
        </Button>
      </span>
      {failed ? <span role="alert" className="ap-error">Couldn’t save that. Try again.</span> : null}
    </div>
  );
}
