// One fake adb, emulator and xcrun program, run by two suites: the unit device tests
// (orbyn-core/src/devices/test_support.rs) and the edge device tests
// (orbynd/tests/devices.rs). Both copy it beside a temp dir and invoke it as
// `node <script> <tool> <fake-dir> <args...>`; the files in the fake dir pick the
// answer, so each case is a fixture file rather than a different program.
//
// Every call is appended to calls.ndjson before it answers. Files the tests use:
//   calls.ndjson          every {tool, args} this program was run with
//   action_started        the pid of the action that is running now
//   action_delay_ms       sleep before an action answers (the busy case)
//   action_timeout        hang an action past the service's timeout
//   action_exit           exit code for an action (2 is tool_failed)
//   action_stdout_bytes   bytes an action writes before exiting
//   screenshot_bytes      size of the PNG a screenshot answers
//   screenshot_text       text placed in the PNG after its magic bytes
//   screenshot_png        "0" answers bytes that are not a PNG
//   android_mode          "flat", "foldable", "timeout" or "fail"
//   ios_mode              "normal", "timeout" or "fail"
//   booted, ios_booted    "0" lists the device as missing or shutdown
//   fold, hinge           the emulator's posture and hinge angle
//
// The unit tests came first and own the defaults below; the edge tests set the files
// they need, so one program is the source of truth for both.

const fs = require('node:fs');
const path = require('node:path');

const [tool, dir, ...args] = process.argv.slice(2);
fs.mkdirSync(dir, { recursive: true });
fs.appendFileSync(path.join(dir, 'calls.ndjson'), JSON.stringify({ tool, args }) + '\n');

const read = (name, fallback) => {
  try { return fs.readFileSync(path.join(dir, name), 'utf8').trim(); }
  catch { return fallback; }
};
const write = (name, value) => fs.writeFileSync(path.join(dir, name), String(value));
const sleep = (ms) => {
  const cell = new Int32Array(new SharedArrayBuffer(4));
  Atomics.wait(cell, 0, 0, ms);
};
const exit = (code, text) => {
  if (text) fs.writeSync(2, text + '\n');
  process.exit(code);
};
const png = (size) => {
  const magic = Buffer.from('89504e470d0a1a0a', 'hex');
  const text = Buffer.from(read('screenshot_text', ''));
  const body = Buffer.concat([text, Buffer.alloc(Math.max(0, size - magic.length - text.length), 0x41)]);
  return Buffer.concat([magic, body.subarray(0, Math.max(0, size - magic.length))]);
};
const writeAll = (data) => {
  const bytes = Buffer.isBuffer(data) ? data : Buffer.from(data);
  let offset = 0;
  while (offset < bytes.length) offset += fs.writeSync(1, bytes, offset);
};
const action = (args) =>
  (args.includes('emu') && (args.includes('fold') || args.includes('unfold') || args.includes('kill') || args.includes('set')))
  || (args.includes('settings') && args.includes('put'))
  || (args.includes('input') && args.includes('keyevent'));
const maybeAction = () => {
  if (!action(args)) return false;
  write('action_started', process.pid);
  if (read('action_timeout', '0') === '1') {
    write('pid', process.pid);
    sleep(30000);
    process.exit(0);
  }
  const delay = Number(read('action_delay_ms', '0'));
  if (delay > 0) sleep(delay);
  const code = Number(read('action_exit', '0'));
  if (code !== 0) exit(code, 'tool refused');
  const bytes = Number(read('action_stdout_bytes', '0'));
  if (bytes > 0) writeAll(Buffer.alloc(bytes, 0x78));
  return true;
};

if (tool === 'adb') {
  if (read('android_mode', 'flat') === 'fail') exit(2, 'adb broke');
  if (args[0] === 'devices' && read('android_mode', 'flat') === 'timeout') {
    write('pid', process.pid);
    sleep(30000);
    process.exit(0);
  }
  if (args[0] === 'devices') {
    const delay = Number(read('list_delay_ms', '0'));
    if (delay > 0) sleep(delay);
    fs.writeSync(1, 'List of devices attached\n');
    if (read('booted', '1') !== '0') fs.writeSync(1, 'emulator-5554\tdevice\n');
    process.exit(0);
  }
  if (args.includes('emu') && args.includes('avd') && args.includes('name')) {
    if (read('avd_name_fail', '0') === '1') exit(2, 'no avd name');
    const name = read('android_mode', 'flat') === 'foldable' ? 'Pixel_Fold' : 'Pixel_8';
    fs.writeSync(1, name + '\nOK\n');
    process.exit(0);
  }
  if (args.includes('getprop') && args.includes('ro.build.version.release')) {
    fs.writeSync(1, '14\n');
    process.exit(0);
  }
  if (args.includes('dumpsys') && args.includes('input')) {
    fs.writeSync(1, 'Input Reader\n  SurfaceOrientation: ' + read('orientation', '0') + '\n');
    process.exit(0);
  }
  if (args.includes('cmd') && args.includes('device_state') && args.includes('print-states')) {
    if (read('foldable_read', '1') === '0') exit(2, 'no device_state');
    if (read('android_mode', 'flat') === 'foldable') {
      fs.writeSync(1, "Supported states: [\n  DeviceState{identifier=0, name='CLOSED'},\n  DeviceState{identifier=1, name='HALF_OPENED'},\n  DeviceState{identifier=2, name='OPENED'},\n]\n");
    } else {
      fs.writeSync(1, "Supported states: [\n  DeviceState{identifier=0, name='UNFOLDED'},\n]\n");
    }
    process.exit(0);
  }
  if (args.includes('cmd') && args.includes('device_state') && args.includes('print-state')) {
    if (read('android_mode', 'flat') === 'foldable') {
      const fold = read('fold', 'opened');
      fs.writeSync(1, (fold === 'closed' ? '0' : fold === 'half_opened' ? '1' : '2') + '\n');
    } else {
      fs.writeSync(1, '0\n');
    }
    process.exit(0);
  }
  if (args.includes('sensor') && args.includes('get') && args.includes('hinge-angle0')) {
    fs.writeSync(1, read('hinge', '180') + '\nOK\n');
    process.exit(0);
  }
  if (args.includes('exec-out') && args.includes('screencap')) {
    const delay = Number(read('screenshot_delay_ms', '0'));
    if (delay > 0) sleep(delay);
    const size = Number(read('screenshot_bytes', '64'));
    writeAll(read('screenshot_png', '1') === '1' ? png(size) : Buffer.from('not a png'));
    process.exit(0);
  }
  if (args.includes('emu') && args.includes('fold')) { write('fold', 'closed'); write('hinge', '0'); if (maybeAction()) process.exit(0); }
  if (args.includes('emu') && args.includes('unfold')) { write('fold', 'opened'); write('hinge', '180'); if (maybeAction()) process.exit(0); }
  if (args.includes('emu') && args.includes('kill')) { write('booted', '0'); if (maybeAction()) process.exit(0); }
  if (args.includes('sensor') && args.includes('set') && args.includes('hinge-angle0')) {
    const angle = args[args.length - 1];
    write('hinge', angle);
    write('fold', angle === '0' ? 'closed' : angle === '180' ? 'opened' : 'half_opened');
    if (maybeAction()) process.exit(0);
  }
  if (args.includes('settings') && args.includes('user_rotation')) {
    write('orientation', args[args.length - 1]);
    if (maybeAction()) process.exit(0);
  }
  if (maybeAction()) process.exit(0);
  process.exit(0);
}

if (tool === 'emulator') {
  if (read('android_mode', 'flat') === 'fail') exit(2, 'emulator broke');
  if (args.includes('-list-avds')) {
    fs.writeSync(1, read('android_mode', 'flat') === 'foldable' ? 'Pixel_Fold\nTablet\n' : 'Pixel_8\nTablet\n');
    process.exit(0);
  }
  process.exit(0);
}

if (tool === 'xcrun') {
  if (read('ios_mode', 'normal') === 'fail') exit(2, 'simctl broke');
  if (read('ios_mode', 'normal') === 'timeout') {
    write('pid', process.pid);
    sleep(30000);
    process.exit(0);
  }
  if (args.includes('list') && args.includes('--json')) {
    const booted = read('ios_booted', '1') !== '0';
    const available = read('ios_available', '1') !== '0';
    const payload = { devices: {
      'com.apple.CoreSimulator.SimRuntime.iOS-26-1': [
        { udid: 'UDID-1', name: 'iPhone 17 Pro', state: booted ? 'Booted' : 'Shutdown', isAvailable: true },
        { udid: 'UDID-2', name: 'iPhone Fold', state: 'Shutdown', isAvailable: available },
      ],
      'com.apple.CoreSimulator.SimRuntime.watchOS-11-0': [
        { udid: 'WATCH-1', name: 'Watch', state: 'Shutdown', isAvailable: true },
      ],
    } };
    fs.writeSync(1, JSON.stringify(payload));
    process.exit(0);
  }
  if (maybeAction()) process.exit(0);
  if (args.includes('io') && args.includes('screenshot')) {
    const file = args[args.length - 1];
    const fail = Number(read('screenshot_exit', '0'));
    if (fail !== 0) exit(fail, 'screenshot failed');
    if (read('screenshot_timeout', '0') === '1') {
      write('pid', process.pid);
      sleep(30000);
      process.exit(0);
    }
    if (read('screenshot_symlink', '0') === '1') {
      fs.symlinkSync(path.join(dir, 'outside.png'), file);
      process.exit(0);
    }
    const size = Number(read('screenshot_bytes', '64'));
    fs.writeFileSync(file, png(size));
    process.exit(0);
  }
  if (args.includes('ui') && args.includes('home')) process.exit(0);
  if (args.includes('shutdown')) { write('ios_booted', '0'); process.exit(0); }
  process.exit(0);
}

exit(3, 'unknown tool');
