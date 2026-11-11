// Fails the build if a platform design screen (src/features/platform) or a Crew preview screen (src/features/crew) reached the output.
// Those screens exist only behind the dev-only __ORBYN_PREVIEWS__ flag, which is false here.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const markers = [
  "Available with Orbyn accounts",
  "platform-notice",
  "Sign in to Orbyn",
  "sam@example.com",
  // Crew Preview screens: the notice, the hash route (plain and as a regex source), a frame id and a class.
  "Crew is not connected to this app yet",
  "preview/crew",
  "preview\\/crew",
  "mergequeue-narrow-panel",
  "crew-apq",
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
