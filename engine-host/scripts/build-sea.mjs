// Builds one single-executable binary: bundle with esbuild, make a SEA blob, inject it with postject.
// Signing and notarization are a separate step (see README); this script only produces the unsigned binary.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, writeFileSync, chmodSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "dist");
mkdirSync(out, { recursive: true });

await build({
  entryPoints: [join(root, "src/main.mjs")],
  bundle: true,
  platform: "node",
  format: "cjs", // SEA main scripts are CommonJS
  target: "node22",
  outfile: join(out, "host.cjs"),
  logLevel: "warning",
});

writeFileSync(
  join(out, "sea-config.json"),
  JSON.stringify({ main: join(out, "host.cjs"), output: join(out, "host.blob"), disableExperimentalSEAWarning: true }),
);
execFileSync(process.execPath, ["--experimental-sea-config", join(out, "sea-config.json")], { stdio: "inherit" });

const bin = join(out, process.platform === "win32" ? "orbyn-engine-host.exe" : "orbyn-engine-host");
copyFileSync(process.execPath, bin);
chmodSync(bin, 0o755);
if (process.platform === "darwin") execFileSync("codesign", ["--remove-signature", bin]);

const args = [join(root, "node_modules/postject/dist/cli.js"), bin, "NODE_SEA_BLOB", join(out, "host.blob"), "--sentinel-fuse", "NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2"];
if (process.platform === "darwin") args.push("--macho-segment-name", "NODE_SEA");
execFileSync(process.execPath, args, { stdio: "inherit" });
if (process.platform === "darwin") execFileSync("codesign", ["--sign", "-", bin]);
console.log(`built ${bin}`);
