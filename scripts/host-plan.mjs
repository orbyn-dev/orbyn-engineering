// A host is shipped only where it was built and checked to run on that machine class: macOS arm64 and
// Windows x64. The build copies the Node that runs it into the single executable, so the host's architecture
// is the build machine's (Intel Macs crash; Windows arm64 would need an arm64 Node), and the SDK's native
// package must match the machine that runs the host. Every other combination, including each cross build,
// stages the empty placeholder instead.

const SHIPPED = new Map([
  ["aarch64-apple-darwin", { platform: "darwin", arch: "arm64" }],
  ["x86_64-pc-windows-msvc", { platform: "win32", arch: "x64" }],
]);

/**
 * What to ship for a build: `ship` is true only when the target and the build machine are a pair the host
 * was built and checked for, and `sdk` is then the platform and arch `prepare:sdks` must fetch. Otherwise
 * `{ ship: false, sdk: null }`.
 */
export function hostPlan({ target, platform, arch }) {
  const expected = SHIPPED.get(target);
  if (!expected || expected.platform !== platform || expected.arch !== arch) return { ship: false, sdk: null };
  return { ship: true, sdk: { platform, arch } };
}
