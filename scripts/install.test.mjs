// Packs a stand-in CLI with pack-cli.mjs, serves it from loopback as a release would,
// and runs this platform's installer against it: install.sh on macOS, install.ps1 on
// Windows. Every folder the installer writes is a throwaway one.
import assert from "node:assert/strict";
import { execFile, execFileSync } from "node:child_process";
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { machine, tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { packCli } from "./pack-cli.mjs";

const scripts = dirname(fileURLToPath(import.meta.url));
const windows = process.platform === "win32";
const arm = machine() === "arm64" || machine() === "aarch64";
const target = windows
  ? `${arm ? "aarch64" : "x86_64"}-pc-windows-msvc`
  : `${arm ? "aarch64" : "x86_64"}-apple-darwin`;
const skip = !["darwin", "win32"].includes(process.platform) && "the installers target macOS and Windows";

let root;
let archive;
let server;
let base;
let corrupt = false;
let serveDir;
const requests = [];

before(async () => {
  if (skip) return;
  root = mkdtempSync(join(tmpdir(), "orbyn-install-test-"));
  const binaries = join(root, "binaries");
  mkdirSync(binaries);
  for (const program of ["orbyn", "orbynd"]) {
    if (windows) {
      // A real program to launch: Node itself answers `--version`.
      cpSync(process.execPath, join(binaries, `${program}.exe`));
    } else {
      const file = join(binaries, program);
      writeFileSync(file, `#!/bin/sh\necho "${program} $*"\n`);
      chmodSync(file, 0o755);
    }
  }
  const build = join(root, "build");
  mkdirSync(join(build, "ui"), { recursive: true });
  writeFileSync(join(build, "ui", "index.html"), "<!doctype html>");
  mkdirSync(join(build, "library"), { recursive: true });
  writeFileSync(join(build, "library", "REVISION"), "test\n");
  archive = packCli({ target, binaries, build, out: join(root, "release") });
  serveDir = dirname(archive);

  server = createServer((request, response) => {
    requests.push(request.url);
    const name = basename(request.url);
    const file = join(serveDir, name);
    if (!name.startsWith("orbyn-cli-") || !existsSync(file)) {
      response.writeHead(404).end();
      return;
    }
    const body = readFileSync(file);
    if (corrupt && name.endsWith(".tar.gz")) body[body.length - 1] ^= 0xff;
    response.writeHead(200).end(body);
  });
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  base = `http://127.0.0.1:${server.address().port}`;
});

after(() => {
  server?.close();
  if (root) rmSync(root, { recursive: true, force: true });
});

function install(env = {}) {
  const home = mkdtempSync(join(root, "home-"));
  const settings = { ...process.env };
  delete settings.ORBYN_VERSION;
  delete settings.ORBYN_CHANNEL;
  Object.assign(settings, {
    HOME: home,
    USERPROFILE: home,
    ORBYN_INSTALL_BASE: base,
    ORBYN_INSTALL_DIR: join(home, "bin"),
    ORBYN_HOME: join(home, "share"),
    ...env,
  });
  return runIn(home, settings);
}

function runIn(home, env) {
  const [command, args] = windows
    ? ["powershell.exe", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", join(scripts, "install.ps1")]]
    : ["sh", [join(scripts, "install.sh")]];
  return new Promise((done) =>
    execFile(command, args, { env, encoding: "utf8" }, (error, stdout, stderr) =>
      done({ status: error ? (error.code ?? 1) : 0, stdout, stderr, home, env }),
    ),
  );
}

const launcher = (home) => join(home, "bin", windows ? "orbyn.cmd" : "orbyn");

function launch(home, ...args) {
  const [command, list] = windows ? ["cmd.exe", ["/d", "/c", launcher(home), ...args]] : [launcher(home), args];
  return new Promise((done, fail) =>
    execFile(command, list, { encoding: "utf8" }, (error, stdout) => (error ? fail(error) : done(stdout.trim()))),
  );
}

test("the latest stable release installs and its launcher runs the command", { skip }, async () => {
  requests.length = 0;
  const result = await install();
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(requests, [
    `/latest/download/orbyn-cli-${target}.tar.gz`,
    `/latest/download/orbyn-cli-${target}.tar.gz.sha256`,
  ]);
  const cli = join(result.home, "share", "cli");
  for (const file of ["ui/index.html", "library/REVISION", windows ? "orbynd.exe" : "orbynd"]) {
    assert.ok(existsSync(join(cli, file)), file);
  }
  const output = await launch(result.home, "--version");
  assert.equal(output, windows ? process.version : "orbyn --version");
});

test("the Windows launcher preserves Unicode paths, arguments, exit status and code page", { skip: !windows }, async () => {
  const home = join(root, "r\u00e9sum\u00e9-\u65e5\u672c\u8a9e with spaces");
  const bin = join(home, "command bin");
  const result = await install({ ORBYN_HOME: join(home, "data"), ORBYN_INSTALL_DIR: bin });
  assert.equal(result.status, 0, result.stderr);
  const probe = join(home, "argument probe.cjs");
  const receipt = join(home, "arguments.json");
  writeFileSync(probe, `require("node:fs").writeFileSync(process.env.ORBYN_TEST_RECEIPT, JSON.stringify(process.argv.slice(2))); process.exit(37);`);
  const driver = join(root, "launch.cmd");
  const shadowCwd = join(root, "shadow cwd");
  const shadowPath = join(root, "shadow path");
  const shadowReceipt = join(root, "shadow-called");
  for (const folder of [shadowCwd, shadowPath]) {
    mkdirSync(folder);
    writeFileSync(join(folder, "chcp.cmd"), '@echo shadow>"%ORBYN_TEST_SHADOW_RECEIPT%"\r\n');
  }
  // Environment expansion supplies Unicode paths without encoding them in this driver.
  writeFileSync(driver, [
    "@echo off",
    '"%SystemRoot%\\System32\\chcp.com" %ORBYN_TEST_CODEPAGE% >nul',
    'call "%ORBYN_TEST_LAUNCHER%" "%ORBYN_TEST_PROBE%" "two words" "" "a&b" "%ORBYN_TEST_ARGUMENT%"',
    'set "result=%errorlevel%"',
    '"%SystemRoot%\\System32\\chcp.com"',
    "exit /b %result%",
    "",
  ].join("\r\n"));
  for (const [codepage, shadow] of [[437, "cwd"], [932, "cwd"], [65001, "cwd"], [437, "path"]]) {
    for (const shell of ["cmd.exe", "powershell.exe"]) {
      const args = shell === "cmd.exe"
        ? ["/d", "/c", driver]
        : ["-NoProfile", "-Command", "& $env:ORBYN_TEST_DRIVER; exit $LASTEXITCODE"];
      const actual = await new Promise((done) => execFile(shell, args, {
        encoding: "utf8",
        cwd: shadow === "cwd" ? shadowCwd : root,
        env: {
          ...process.env,
          PATH: `${shadowPath};${process.env.PATH}`,
          ORBYN_TEST_DRIVER: driver,
          ORBYN_TEST_LAUNCHER: join(bin, "orbyn.cmd"),
          ORBYN_TEST_PROBE: probe,
          ORBYN_TEST_RECEIPT: receipt,
          ORBYN_TEST_CODEPAGE: String(codepage),
          ORBYN_TEST_ARGUMENT: "r\u00e9sum\u00e9 \u65e5\u672c\u8a9e",
          ORBYN_TEST_SHADOW_RECEIPT: shadowReceipt,
        },
      }, (error, stdout, stderr) => done({ status: error?.code ?? 0, stdout, stderr })));
      assert.equal(actual.status, 37, `${shell}, code page ${codepage}: ${actual.stderr}`);
      assert.ok(!existsSync(shadowReceipt), `${shell} executed the ${shadow} chcp shadow`);
      assert.deepEqual(JSON.parse(readFileSync(receipt, "utf8")), ["two words", "", "a&b", "r\u00e9sum\u00e9 \u65e5\u672c\u8a9e"]);
      assert.match(actual.stdout, new RegExp(`\\b${codepage}\\b`), "the caller's code page changed");
    }
  }
});

test("the Windows launcher saves only the digits of the code page, in any locale", { skip: !windows }, async () => {
  const result = await install({ ORBYN_HOME: join(root, "cp-data"), ORBYN_INSTALL_DIR: join(root, "cp-bin") });
  assert.equal(result.status, 0, result.stderr);
  const line = readFileSync(join(root, "cp-bin", "orbyn.cmd"), "utf8").split(/\r?\n/).find((l) => l.startsWith("for /f"));
  assert.ok(line, "the launcher reads the code page");
  for (const [output, digits] of [["Active code page: 437", "437"], ["Aktive Codepage: 850.", "850"], ["Page de codes active : 1252", "1252"]]) {
    const driver = join(root, "codepage.cmd");
    // Replace chcp with a fixed line of output so every locale's form can be parsed here.
    writeFileSync(driver, ["@echo off", line.replace(/\('.*'\)/, `('echo ${output}')`), "echo [%_orbyn_cp%]", ""].join("\r\n"));
    const actual = await new Promise((done) => execFile("cmd.exe", ["/d", "/c", driver], { encoding: "utf8" }, (error, stdout) => done(stdout)));
    assert.equal(actual.trim(), `[${digits}]`, output);
  }
});

test("an exact version and the nightly channel pick their own release", { skip }, async () => {
  for (const [env, folder] of [
    [{ ORBYN_VERSION: "v1.2.3" }, "/download/v1.2.3/"],
    [{ ORBYN_VERSION: "1.2.3", ORBYN_CHANNEL: "nightly" }, "/download/v1.2.3/"],
    [{ ORBYN_CHANNEL: "nightly" }, "/download/nightly/"],
  ]) {
    requests.length = 0;
    const result = await install(env);
    assert.equal(result.status, 0, result.stderr);
    assert.ok(requests.every((path) => path.startsWith(folder)), `${JSON.stringify(env)}: ${requests}`);
  }
});

test("an unknown channel is refused before any download", { skip }, async () => {
  requests.length = 0;
  const result = await install({ ORBYN_CHANNEL: "beta" });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /stable or nightly/);
  assert.deepEqual(requests, []);
});

test("a checksum mismatch installs nothing and keeps the earlier install", { skip }, async () => {
  const first = await install();
  assert.equal(first.status, 0, first.stderr);
  const marker = join(first.home, "share", "cli", "kept");
  writeFileSync(marker, "");
  corrupt = true;
  try {
    const second = await runIn(first.home, first.env);
    assert.notEqual(second.status, 0);
    assert.match(second.stderr, /does not match its checksum/);
    assert.ok(existsSync(marker), "the earlier install was replaced");
  } finally {
    corrupt = false;
  }
});

test("an unreachable release says so and installs nothing", { skip }, async () => {
  const missing = await install({ ORBYN_INSTALL_BASE: "http://127.0.0.1:9" });
  assert.notEqual(missing.status, 0);
  assert.match(missing.stderr, /could not download/);
  assert.ok(!existsSync(launcher(missing.home)));
});

// An archive built by hand, served in place of the real one with a matching checksum, so the installer's own checks
// on what the archive holds are what answer. Unix only: the shell installer's tar.
const unixOnly = skip || (windows && "the shell installer and tar run on macOS");

function serveArchive(entries) {
  const dir = mkdtempSync(join(root, "alt-"));
  const stage = mkdtempSync(join(root, "stage-"));
  const archiveName = basename(archive);
  const file = join(dir, archiveName);
  const spec = JSON.stringify({ file, entries });
  execFileSync("python3", [
    "-I",
    "-c",
    [
      "import io, json, sys, tarfile",
      "spec = json.loads(sys.argv[1])",
      "with tarfile.open(spec['file'], 'w:gz') as tar:",
      "    for name, mode in spec['entries']:",
      "        data = b'#!/bin/sh\\necho x\\n'",
      "        info = tarfile.TarInfo(name)",
      "        info.size = len(data)",
      "        info.mode = mode",
      "        tar.addfile(info, io.BytesIO(data))",
    ].join("\n"),
    spec,
  ]);
  const sum = createHash("sha256").update(readFileSync(file)).digest("hex");
  writeFileSync(`${file}.sha256`, `${sum}  ${archiveName}\n`);
  rmSync(stage, { recursive: true, force: true });
  return dir;
}

test("an archive without orbynd installs nothing and keeps the earlier install", { skip: unixOnly }, async () => {
  const first = await install();
  assert.equal(first.status, 0, first.stderr);
  const marker = join(first.home, "share", "cli", "kept");
  writeFileSync(marker, "");
  const real = serveDir;
  serveDir = serveArchive([["./orbyn", 0o755]]);
  try {
    const second = await runIn(first.home, first.env);
    assert.notEqual(second.status, 0);
    assert.match(second.stderr, /does not hold orbyn and orbynd/);
    assert.ok(existsSync(marker), "the earlier install was replaced by an incomplete archive");
  } finally {
    serveDir = real;
  }
});

test("an archive that names paths outside its folder writes nothing outside it", { skip: unixOnly }, async () => {
  const first = await install();
  const scratch = mkdtempSync(join(root, "tmp-"));
  const inner = join(scratch, "inner");
  mkdirSync(inner);
  const absolute = join(first.home, "absolute-marker");
  const real = serveDir;
  serveDir = serveArchive([
    ["./orbyn", 0o755],
    ["./orbynd", 0o755],
    ["../../escaped-marker", 0o644],
    [absolute, 0o644],
  ]);
  try {
    // The installer works in a folder made under TMPDIR, so `../..` from its extraction folder is TMPDIR itself.
    await runIn(first.home, { ...first.env, TMPDIR: inner });
    assert.ok(!existsSync(join(inner, "escaped-marker")), "an entry climbed out of the extraction folder");
    assert.ok(!existsSync(join(scratch, "escaped-marker")), "an entry climbed out of TMPDIR");
    assert.ok(!existsSync(absolute), "an absolute entry was written where it named");
  } finally {
    serveDir = real;
  }
});
