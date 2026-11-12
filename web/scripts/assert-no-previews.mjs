// Fails the build if a platform design screen (src/features/platform) or a Crew preview screen (src/features/crew) reached the output.
// Those screens exist only behind the dev-only __ORBYN_PREVIEWS__ flag, which is false here.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const markers = [
  "Available with Orbyn accounts",
  "platform-notice",
  "Sign in to Orbyn",
  "sam@example.com",
  // Crew preview-only code (features/crew/autopilot/review previews): the hash route (plain and as a regex source),
  // a frame id and a class. The real Crew screens ship in production behind the service's own flag, so their text
  // and classes ("Crew is not connected to this app yet", ap-*) are not markers.
  "preview/crew",
  "preview\\/crew",
  "mergequeue-narrow-panel",
  "crew-apq",
  // Crew Designs v20 to v23 (features/crew/{escape,capacity,oracle,backtest,settings,shared}): the stage class, a frame id of each group
  // and the design's own class prefix.
  "cx-stage",
  "cx-frame",
  "escape-narrow-window",
  "capacity-coverage-narrow-window",
  "oracle-narrow-window",
  "backtest-narrow-window",
  "settings-crew",
];
const dir = process.argv[2] ?? "dist";
const walk = (path) =>
  readdirSync(path).flatMap((name) => {
    const full = join(path, name);
    return statSync(full).isDirectory() ? walk(full) : [full];
  });

const offenders = walk(dir)
  .filter((path) => /\.(js|css|html)$/.test(path))
  .filter((path) => {
    const text = readFileSync(path, "utf8");
    return markers.some((marker) => text.includes(marker));
  });

if (offenders.length > 0) {
  console.error(`Platform preview screens found in production output: ${offenders.join(", ")}`);
  process.exit(1);
}
console.log("No platform preview screens in production output.");
