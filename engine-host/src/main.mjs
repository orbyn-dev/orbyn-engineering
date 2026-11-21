import { runHost } from "./host.mjs";
import { fake } from "./families/fake.mjs";

const families = { fake };
const name = process.argv[2];
const family = families[name];
if (!family) {
  process.stderr.write(`unknown family: ${name ?? "(none)"}\n`);
  process.exit(2);
}

runHost({
  family,
  input: process.stdin,
  output: process.stdout,
  onExit: (code) => process.exit(code),
});
