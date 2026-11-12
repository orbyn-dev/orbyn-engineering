# Crew beta rows

Appendices A, B and C of the [Crew beta plan](../crew.md), a snapshot of Products Research's plan as of 2026-11-11.
They list the unfinished app rows, the designed screens not yet built and the rows that still have no design. The
current state of a row is always in [parity.md](../parity.md) and [design-inventory.md](../design-inventory.md); these
tables are the list the beta commits to. Frame ids are the ids in the private `orbyn-designs` repository, which this
file does not link.

## Appendix A: every unfinished beta row (app parity)

From [parity.md](../parity.md) on main at `ae67b34a`, counted with `scripts/count-parity.py`: 533 in-scope rows, of which 396 are done and **100 are partial, 33 open and 4 not doable**. Excluded rows (mobile, remote pairing, hosted) stay out. "Remaining" is the row's own note, shortened; read the row in parity.md before starting.

### A. Threads and sidebar (2)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| A54 | not doable | V1 thread migration: copies old database, imports transcripts in stages | Orbyn has never shipped a V1 thread database or a legacy reader, so there is no predecessor state.sqlite to copy and no legacy transcript source to import in stages; its only persisted thread… |
| A61 | partial | Sidebar grouping and sort: repository/path/separate; project and thread sort; overrides | native desktop folder rendering and the repaired flow on macOS remain unobserved; synthetic Windows browser evidence does not establish a full hands-on Windows walkthrough |

### B. Composer (6)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| B8 | open | HEIC conversion and resizing: JPEG conversion, shrinks oversized photos |  |
| B41 | partial | PR chips: `#` browse, number and text search | a prompt link is implemented; the referenced PR chip is not |
| B42 | partial | Thread chips: `@` lookup, on-demand read by the agent | a prompt link is implemented; the referenced thread chip is not |
| B43 | partial | Drag threads to composer: multi-select, cross-environment transcript file | a local thread link is implemented; transcript-file drag is not established |
| B47 | partial | Question attachments: files in custom answers, kept per question |  |
| B61 | partial | Steer delivery states: each mid-turn message reports pending, delivered or failed by id; a failed one returns to the queue; engines with no mid-turn input hold it for the next turn | per-message pending, delivered and failed timeline marks are missing |

### C. Timeline and rendered output (6)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| C4 | partial | Orchestrator summaries: action-specific wording, failures not counted | native qualification on a provider other than the observed Codex 0.160.0, still needs a hands-on run in the packaged app; the packaged CI jobs start no turn and drive no webview; the same success,… |
| C10 | partial | Claude task lists: Tasks row via env flag | a live Claude turn that actually calls the modern task tools; the text-only creation fixture has a null parent ID, so nested subagent traffic is not established; the engine opts into them itself,… |
| C19 | partial | Media in messages: image gallery, video, save/copy path | a sent attachment opens in the dialog on its own instead of joining its message's gallery, remote video, HTTP images and unsupported remote image formats remain unavailable under the closed… |
| C31 | partial | Right panel tabs: add-tab menu (Browser, Terminal, Files, Diff, Pull request, Linked pull requests, Device, Side chat), mute tab, close, maximize, resize handle, sheet on small screens | checked: no Device surface exists anywhere (H20 is open), so the disabled Device entry is correct and was not built; browser mute is refused because a previewed page is streamed JPEG frames with… |
| C37 | partial | Sentence tool trace: each tool call is a verb-object row with kind icon; calls group in fives with a count summary; live view bounded to 30 rows; turn folds to a summary at the end | native qualification on a provider other than the observed Codex 0.160.0, still needs a hands-on run in the packaged app; the packaged CI jobs start no turn and drive no webview; the real Codex… |
| C40 | partial | Rich question content: agent questions render Markdown, charts and sanitised SVG or static HTML in an isolated root; remote images become links | live-provider question and native qualification. A real Codex 0.166.0 turn through the real service on 5 November 2026 answered the request to ask a question with "I can't use AskUserQuestion here… |

### D. Files, search and attachments (2)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| D19 | partial | File comments: annotations feed review chips | packaged macOS/Windows observation; comments use the existing chip shelf above the textarea, and annotation positions do not remap while editing file contents; local browser qualification used an… |
| D21 | partial | Outside-workspace files: read-only, isolated HTML | Windows path cases run only on the CI matrix; a OneDrive placeholder opened with `FILE_FLAG_OPEN_REPARSE_POINT` may read as empty or fail; packaged desktop observation not made |

### E. Diffs, checkpoints and revert (1)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| E19 | partial | Write-attributed changes: a turn's change list holds only files the thread declared (begin and complete handshake) or the engine reported, frozen at tool completion; never inferred from status; caps 200 paths, 1 MiB per file, 32 MiB per turn | remove the visible per-file restore deviation via PR 72 and verify the integrated package |

### F. Source control, worktrees and pull requests (7)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| F3 | open | Multi-account host picker: account per host, host off |  |
| F12 | partial | Add existing folder: picker with filesystem browse | the row records service completion without client evidence |
| F16 | partial | Initialize Git: vcs init from project | the row records service completion without client evidence |
| F18 | partial | AI commit and PR text: generated messages, titles, descriptions | generation supports the guarded Claude one-shot path only; Codex and unproven drivers are refused because a strict tool-free boundary is unproven, with manual text retained; qualification uses… |
| F19 | partial | Writing style: repo conventions or custom, follows templates | uses F18's guarded-provider subset; scoped settings, history/custom instructions and base-tree templates have offline fixture qualification, no live model quality or packaged macOS writing observation |
| F25 | partial | Branch naming: static, semantic or custom | semantic and custom branch naming are missing |
| F30 | partial | Storage cleanup: worktree rules, keep-when policy, reports | inactive, merged, deleted and unchanged worktree rules, keep-when choices, automatic scheduling, concurrent thread-creation admission and final native/Windows qualification |

### H. Browser preview and devices (19)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| H6 | partial | Agent cursor: visual trace of agent input | packaged macOS and Windows desktop qualification |
| H8 | partial | Recording: frame rate, key and mouse press overlays | packaged macOS and Windows desktop qualification |
| H10 | partial | Browser profiles: named profiles, clear profile | the profile settings surface is missing |
| H16 | partial | Take control: agent/human handover, read-only watch | agent control of a preview is missing |
| H17 | partial | Remote clipboard, file picker, downloads: bridged to device | not observed in the packaged desktop app, where the clipboard and a blob download go through the system web view |
| H20 | open | Device panel: live iOS Simulator or Android Emulator |  |
| H21 | open | Device setup: three-step hub, platform, agent access |  |
| H22 | open | Device tabs and float: multiple tabs, rename, float over chat |  |
| H23 | open | Device input: touch, type, Home, Back, Recents, rotate, power |  |
| H24 | open | 3D device view: model with fold and hinge controls |  |
| H25 | open | Android fold controls: fold or unfold |  |
| H26 | open | Device tools drawer: appearance, text, accessibility, location, permissions, push |  |
| H27 | open | Agent devices: `agent-device` CLI, permission toggle |  |
| H31 | open | Device tool versions: check and auto install |  |
| H33 | open | SnapShots: window capture to draft with app metadata |  |
| H34 | open | SnapShot setup by platform: accessibility data, GNOME, KDE, Hyprland, Niri, shortcut, sound |  |
| H35 | not doable | Preview more menu: hard reload, DevTools, separate preview window, move to another panel, device toolbar toggle, light/dark/system appearance, zoom, clear cookies, clear cache | native DevTools cannot inspect the remote page through its streamed JPEG frame; the other menu actions use the live preview service |
| H36 | partial | Device toolbar: responsive mode, viewport presets, rotate, drag handles to resize the page | no typed size and no aspect-ratio lock, so a drag is free-form; the page keeps its box after the toolbar is hidden but offers no handles then |
| H42 | partial | Agent browser access: global switch, per-project override, and device support onboarding flag | the local device-support flag is stored but has no effect |

### I. Providers (29)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| I9 | partial | Codex feedback: send feedback upstream | the route and dialog are covered by a fake-engine service test and component tests, not by a run against installed Codex; Windows and the packaged app are unobserved |
| I10 | open | App access approvals: MCP elicitation handling |  |
| I11 | partial | Claude provider: uses Claude Code login and config | installed Claude Code 2.1.318 on the build host reported `loggedIn: false, authMethod: none` (`claude auth status`), so an authenticated turn against the real provider is unobserved; Windows is… |
| I15 | partial | Claude skills: config, project and plugin sources | discovery reads names and descriptions only, and the build host has no authenticated Claude session, so an installed CLI loading and running these skills is unobserved; Windows is unobserved |
| I17 | open | Cursor provider: official SDK local agent runtime | `cursor-agent` is not installed on the build host, so its ACP launch, models and approvals are unobserved; the SDK runtime is not built |
| I20 | open | Grok provider: ACP adapter with xAI extension | xAI's Grok Build (`@xai-official/grok`) is not installed here, so its ACP session, models and the xAI extension are unobserved and not built; Windows unobserved |
| I21 | partial | OpenCode provider: local or external server, 1.x and 2.x | OpenCode 1.x and Windows are unobserved; an external OpenCode server and stored passwords stay excluded |
| I22 | open | Antigravity provider: Google ACP agent | an Antigravity ACP server, its session and models are unobserved; Windows unobserved |
| I24 | not doable | Antigravity runtime: managed install and removal | (#174) a managed Antigravity runtime means Orbyn downloading and removing the engine itself, which rule 9 and the install rule forbid: an engine is installed only by the person, through the… |
| I25 | open | Pi provider: RPC integration with MCP injection | no `pi` program on the build host, so the protocol, its 0.80.5 minimum and MCP injection cannot be observed and no driver is built; a mock-only driver would complete nothing |
| I26 | open | Muse Code provider: opt-in, Supervised and Full access | no `muse` program on the build host, so its protocol and Supervised and Full access flags are unobserved and no driver is built; its login and keys stay excluded |
| I27 | open | fx provider: Supervised and Auto with effort | no `fx` program on the build host, so its launch, Supervised `ask` mode, effort option and `fx status` default model are unobserved; Windows unobserved |
| I31 | partial | ACP v1/v2 negotiation: preview protocol with fallback | Orbyn does not speak ACP 2 (its renamed initialize, session and auth shapes), and no agent on the build host speaks it, so neither the v2 path nor the v1-shaped 2 fallback is observed against a… |
| I34 | open | Named ACP agents: Claude, Copilot, Devin, Droid adapters | `copilot`, `devin` and `droid` are not installed on the build host, so their launches are proved with the fake agent only; Droid's elicitation question forms are not built, because Orbyn's ACP… |
| I38 | partial | Provider install flow: start, cancel, subscribe, remove | Windows unobserved |
| I39 | partial | Provider updates: per-provider and update-all | Windows unobserved |
| I40 | partial | Model catalog: refreshed discovery with manifest | Claude discovery on Windows and Claude in the packaged app are unobserved; the macOS engine copies are task-owned, so their installer ownership is not claimed |
| I41 | partial | Status banners: provider health in composer | the banner itself inside the packaged Windows/macOS webview is not driven, only the service it reads; authenticated provider health is not established by fixtures |
| I45 | partial | Permission-mode matrix per provider: fx Supervised and Auto; Muse Supervised and Full; Pi Supervised, Auto-accept edits, Full (stored Auto shows Supervised); OpenCode Auto equals Supervised; mode change restarts the provider session | fx, Muse and Pi have no provider yet (I25, I26, I27), so their matrix rows are data with no engine to run them; no signed-in OpenCode turn was observed, so the Supervised prompt on a saved Auto is… |
| I46 | open | fx behavior: shared machine login, Default model from `fx status`, Medium effort start, `$` skill dedupe rules, steer stops the turn, mode saved as fx default, sub-agent threads with shortened replies that wait on permission, unsupported list (plan, full access, commit and PR text, titles) | I27 and an installed fx with its own login |
| I47 | open | Muse behavior: host key from login, server-inherited key ignored, per-instance key wins, custom models for inaccessible ones, no `$` skills, fork copies context, no rewind, no in-app install or sign-in | I26 and an installed Muse with its own login |
| I48 | open | Pi behavior: 0.80.5 minimum, launch-argument validation, remote login paste, native resume, rollback and fork, extension dialogs in composer, notifications in work log, `$` skills, untrusted-folder notice, Pi default fallback model, thinking-level defaults | I25 and an installed Pi at 0.80.5 or later |
| I49 | open | OpenCode behavior: 1.14.19 minimum, 2.x detection, Limited support label on 1.x, local helper vs external URL and password, `.env` approvals, Allow once vs for workspace, 30-second config cache, reconnect on next prompt | the 1.14.19 minimum belongs in the signed tested-versions manifest, which only the owner's key can sign, and the Limited support label on 1.x follows from it; Windows unobserved |
| I50 | open | Cursor behavior: per-instance sign-in, API key override, Change account and Sign out stop that instance's threads, `auto` maps to default model, read-only task sub-agent threads, interrupt-and-restart steering, Windows sandbox rules | I17 and an installed Cursor agent with its own login |
| I51 | partial | Version compatibility warning: provider card flags versions with known problems and offers recommended version install | the recommendation is the signed `recommended` pin or the newest tested version, so the owner maintains advisories by signing them into `agents/engines/tested-versions.json`; both card states are… |
| I52 | partial | Update route by installer: Homebrew, npm, pnpm, Yarn, Bun, Volta, Vite+ detected; mise handled manually; Cursor and Antigravity update with the app | Cursor and Antigravity app-bundled updates need those providers (I17, I22) |
| I53 | partial | Provider discovery: server-host PATH lookup, binary path override, Cursor `cursor-agent` name |  |
| I61 | partial | Warm next turn: start the next Claude process in the background and adopt it only when every launch input matches | Claude 2.1.318 here is signed out, so no turn completes through real orbynd and no real warm process was adopted; whether to turn it on by default is the owner's call, given the hook and MCP side… |
| I62 | open | Extra engines: Kimi Code, DeepSeek, Command Code, Cline, Kilo Code, Hermes Agent and ZCode with their documented quirks (Cline effort by flag, ZCode private protocol) | real runs of each, Cline's effort flag and ZCode's private protocol |

### J. Usage and limits (4)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| J4 | partial | Environment filter: scanning status, dims stale figures | the dimming of pending figures was not watched inside the native window (WKWebView or WebView2), only in component and real-orbynd browser tests; Windows rests on CI, not a hands-on run; remote… |
| J12 | partial | Claude usage limits: reads subscription windows | PR77 implements the promptless CLI route with parser and real-service protocol evidence; live authenticated subscription windows and their integrated UI remain unverified |
| J14 | partial | Other provider limits: Antigravity, OpenCode Go, Cursor, Grok | establish and test the allowed route separately for Antigravity, OpenCode Go, Cursor and Grok; unsupported routes remain explicit |
| J26 | partial | Resume at reset: when an engine reports a reset time, offer to send Continue at that time; overdue resumes recovered at startup before queued messages | match Resume after usage limits, Cancel auto-resume and Wake now controls and verify integrated behavior |

### K. Settings, appearance and notifications (18)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| K1 | partial | Settings scope: choose project and environments, fan-out edit | Windows native qualification; it needs a hands-on packaged walkthrough |
| K2 | partial | Inheritance display: chain, Mixed, Reset all | Windows native qualification; it needs a hands-on packaged walkthrough |
| K5 | partial | Project defaults: model, mode, workspace, scripts | complete model/provider/options default selection and fallback controls, remaining K9 action/editor auditing; final integrated native and Windows qualification |
| K6 | partial | Project page: name, icon, actions, checkouts, removal | final integrated native and Windows qualification; native and Windows qualification need a hands-on packaged walkthrough |
| K7 | partial | Project icons: icon, emoji, monogram, image, automatic | hands-on dialog qualification in unmodified packaged macOS and Windows builds, and other final native qualification |
| K8 | partial | t3.json project file: scripts, icon, workspace defaults, submodules | final integrated Windows qualification of the new checked-read boundary and the remaining K7 icon controls |
| K9 | partial | Project actions: run on worktree create, settle, async, preview URL | native GUI composer submission and native key-to-command execution, macOS hands-on interaction, and unmodified packaged-app qualification; editor and action-save auditing are done (#250) |
| K25 | partial | Dock badge: unread count | service `activity` counts unread threads and the shell badge adds them to waiting runs (unit and real-service tests); the badge on a packaged macOS app was not observed |
| K26 | partial | Background activity profiles: fetch and health cadence, pause rules | profiles and cadence values are stored without runtime consumers; lock, battery and low-power pause behavior is not implemented |
| K27 | partial | Quit confirmation: direct, hold, double-click | `quit_confirmation` reaches the shell, which keeps `Cmd+Q` on Quit only for direct; the page's hold and double-press overlay is unit-tested. A packaged macOS run, including whether WKWebView… |
| K29 | partial | Diagnostics: processes, signals, traces, resource history | actual trace history and identity-fenced cross-platform signals beyond Stop are not implemented; native resource-history proof remains open |
| K30 | partial | OpenTelemetry export: per-signal OTLP URL with test | traces have no producer because the service records no spans; native and Windows qualification |
| K34 | partial | Triage: playbook thread with source for server version | no release tags or build revision exist yet, so the link is the main branch until releases set `ORBYN_SOURCE_REVISION`; the source is linked, not checked out |
| K35 | partial | Welcome wizard: computers, agents, project import | not walked end to end against a real `orbynd` with real Claude or Codex history |
| K36 | partial | Agent history import: scans Claude, Codex, Pi with limits | Pi is not an engine here; imported threads show no earlier messages; a real Claude or Codex resume of an imported session was not observed |
| K38 | partial | GitHub star prompt: occasional prompt | desktop restart persistence across per-launch origins and packaged Windows/macOS walkthrough |
| K48 | partial | Reserved keys: mod+w never closes the desktop window; browsers need alt+w style rebind | the packaged desktop window was not observed with mod+w |
| K49 | open | Snap Shot settings: shortcut (including both Shift keys), sound choice, flash, animations, include accessibility data, setup dialog |  |

### L. Remote access, pairing and auth (1)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| L27 | open | Scoped access: 16 permission scopes, guarded RPCs |  |

### N. MCP tools for agents (1)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| N11 | open | Devices: `device_list`, `_open`, `_screenshot`, `_close` |  |

### O. CLI (9)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| O1 | partial | `t3` / `t3 start`: starts server and opens browser | the configured opener is proved; default Windows OS opener and macOS CLI-launched browser have not been run |
| O3 | partial | `t3 app`: opens project in desktop app | no-path invocation does not select the current directory and the deep link is not acknowledged by the desktop; packaged project-opening proof remains open |
| O10 | partial | `t3 service`: install, update, restart, status, uninstall | real macOS service installation, update, restart, status and uninstall have not been exercised in an isolated OS service manager; Windows unsupported response awaits matrix execution |
| O13 | partial | `t3 update`: exact version, channel, downgrade | no signed release endpoint exists, so no real CLI-started install was observed; Windows execution awaits CI |
| O14 | partial | `t3 uninstall`: removes launcher, service, versions | real launchd removal is not exercised in an isolated service manager (as O10); Windows execution awaits CI |
| O19 | partial | `t3 theme`: set, clear, show | UI adoption is once per newer published theme at startup or on focus (`web/src/app/App.tsx` and `environmentTheme.ts`); CLI writes preferences, with no live preference/file watcher; native OS… |
| O22 | partial | `t3 triage`: starts Claude or Codex in terminal | real macOS/Windows terminal and Ctrl-C observations; context deliberately excludes live service state and raw diagnostics |
| O26 | partial | Try without install: `npx t3@latest`; `t3 help` full reference | no standalone npm launcher package or npx distribution; no try-without-install run is claimed |
| O27 | partial | Render benchmark: replay a seeded 50 KB reply in fixed deltas into this UI and the reference UI in fresh browsers, record main-thread time, heap and mutations | comparative reference numbers and a certified quiet-machine baseline are still missing; owner applications remained open, with no current macOS run |

### P. Desktop shell (18)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| P3 | partial | Auto updater: state machine with release notes | no signed release or update endpoint exists, so a real check, download, install and relaunch has not run on macOS or Windows |
| P4 | partial | Update everything: providers, servers, desktop in order | the desktop update step has not run through a released update |
| P6 | open | Update tracks: Stable and Nightly switching | a published Nightly release and its own update endpoint; the CLI installers already take ORBYN_CHANNEL=nightly (P26) |
| P9 | partial | Deep links: protocol and Linux URL handler | Windows deep-link opening has not been observed |
| P12 | partial | CLI shim: installs `t3` from app | Windows CLI installation has unit evidence only |
| P13 | partial | Application menu: native menus | Windows application menus have not been exercised |
| P15 | partial | Running-thread keep-alive: blocks sleep during work | Windows keep-awake behavior has not been observed |
| P20 | not doable | V1 local storage merge: drafts, stash, layout, theme | Orbyn has no predecessor released local-storage format to merge; there is no legacy source for this upgrade |
| P22 | partial | Windows foreground: focus handling | no Windows run has shown the window coming to the foreground, and CI does not check focus |
| P23 | open | View menu: reload, force reload, DevTools, actual size, zoom in and out (including keypad keys), full screen |  |
| P24 | open | Edit and Window menus: paste as plain text, macOS Speech, minimize, zoom, File menu Settings and Close Window |  |
| P25 | partial | Desktop packaging channels: winget, Homebrew cask, deb (self-updating with password prompt), AUR stable and nightly, Nix, GitHub Releases | no public signed release exists, so no winget manifest or Homebrew cask can name an asset and checksum; GitHub Releases has no published assets yet |
| P26 | partial | CLI install scripts: shell and PowerShell installers to `~/.local/bin`, nightly channel and exact-version pins, Intel Mac desktop only | no release has published the archives, no nightly release exists, install.ps1 has run only in CI, and the macOS archive binaries are not Developer ID signed; Intel Macs get the command too, since… |
| P30 | partial | Idle-gated update install: install and restart only with no run active and two idle minutes; modes Automatic, Notify, Off | the shell-side update behavior still needs native evidence |
| P31 | partial | Deterministic tool search path: ordered install folders, version-manager defaults and npm prefix read from config, added once at startup without sourcing a shell | the shell-side search path still needs native evidence |
| P32 | partial | Windows installer settings: per-user install, silent WebView2 bootstrapper, static C runtime, native arm64 build | the native arm64 package has not been run |
| P34 | partial | Release evidence: per-asset checksums, installer and lockfile SBOMs, coverage report where an empty scan reads "incomplete", actions pinned by commit | installer contents are not covered by the recorded SBOM scan |
| P35 | partial | Notarization check: require credentials for Developer ID builds, verify with stapler and spctl, retry stapling | no Developer ID build has run it against Apple's notary |

### R. RPC surface, grouped (`packages/contracts/src/rpc.ts`, plus `orchestrationV2.ts`) (14)

| Row | State | What | Remaining |
| --- | --- | --- | --- |
| R1 | partial | Projects (10): listEntries, listIgnoredFiles, readFile, searchContents, searchEntries, writeFile, mutate, ensureScratch, createNew, createFromThread | no general file `mutate` handler in the registered routes; save is not create/rename/delete parity. Exact `createFromThread` behavior beyond scratch promotion remains unqualified under… |
| R2 | partial | Shell, filesystem, sessions, assets (9): shell.openInEditor; filesystem.browse, getMetadata; agentSessions.scan, import; assets.createUrl, persistChatAttachments; attachments.createUploadUrl, delete | `getMetadata`, broad Claude/Codex/Pi scan/import (K36, [#180](https://github.com/orbyn-dev/orbyn-engineering/issues/180)), and equivalence of asset-URL creation, chat-attachment persistence and… |
| R3 | partial | MCP Apps (4): callTool, toolInfo, readResource, updateModelContext | toolInfo has no route; a page's own `resources/read` is refused by the host, so readResource beyond the page itself is not served; Codex is the only source |
| R4 | partial | Provider (15): uploadFeedback; auth start, complete, respond, cancel, logout, subscribe; consumeResetCredit; chatgpt reconnect-profile, import-profile, handoff.subscribe; install start, cancel, subscribe, remove | preserve I9's qualification limits and [#186](https://github.com/orbyn-dev/orbyn-engineering/issues/186); install cancellation/progress gaps belong to I38/I39… |
| R5 | partial | VCS and git (11): vcs pull, refreshStatus, listRefs, createWorktree, removeWorktree, createRef, switchRef, init; git runStackedAction, resolvePullRequest, preparePullRequestThread | no explicit VCS refreshStatus/removeWorktree handler in the router (worktree removal happens only through project-policy cleanup, which `worktree_cleanup.rs` tests against real Git worktrees, not… |
| R6 | partial | Review and terminal (10): review getBranchCommits, getDiffPreview, getDiffFileContents; terminal open, attach, observe, write, resize, clear, close | retain the existing service completion declaration without inferring a fresh desktop/platform run. Terminal context chips and resumed engine consoles are G5/G14… |
| R7 | partial | Preview and devices (19): preview open, resize, adjust, close, list, clearProfile, reportProfiles, reportStatus, stopLocalServer; device configure, list, testHost, easAccount, simlockIdentity, open, close, shutdown, detail, action | device configuration/list/open/close/shutdown/detail/action and local toolchain support H20–H27/H31 are unimplemented, not mobile exclusions; retain H24's existing 3D-asset and H25's… |
| R8 | partial | Server core (14): probe, getConfig, getNpmRelease, refreshProviders, updateProvider, updateServer, updateServerWithProgress, commitDesktopUpdate, upsertKeybinding, removeKeybinding, runStorageCleanup, getStorageCleanupReport, getDatabaseStorage, updateSettings | UI removal and full progress/commit semantics remain unqualified in [#185](https://github.com/orbyn-dev/orbyn-engineering/issues/185). K1/K2/K9 gaps belong to… |
| R9 | partial | ACP Registry (11): search, prepareAgent, uninstallManagedBinary, acceptUrlAuth, listSessions, importSession, deleteSession, listProviders, setProvider, disableProvider, logout | exact prepareAgent and internal provider-selection method parity remains in [#185](https://github.com/orbyn-dev/orbyn-engineering/issues/185); I31/I32/I34 negotiation, CLI-owned sign-in and… |
| R10 | partial | Diagnostics and usage (13): discoverSourceControl, getTraceDiagnostics, getProcessDiagnostics, getHostResources, getProcessResourceHistory, getResourceTelemetryHistory, retryResourceTelemetry, checkOtlpEndpoint, prepareTriage, signalProcess, reportClientActivity, getUsageSummary, refreshUsageRates | warning events are not trace diagnostics; aggregate service-tree memory history is not full host resources, per-process history or resource-telemetry history/retry parity. Stop is not arbitrary… |
| R11 | partial | Links, tasks, secrets, sharing, cloud (16): peerLinks list, link, unlink, answerRequest; scheduledTasks subscribe, upsert, setEnabled, delete, runNow, rotateWebhookToken, listWebhookDeliveries, getWebhookDelivery; secrets.answerRequest; guestShare.create; cloud getRelayClientStatus, installRelayClient | full subscribe/detail equivalence remains in [#185](https://github.com/orbyn-dev/orbyn-engineering/issues/185). [#181](https://github.com/orbyn-dev/orbyn-engineering/issues/181) tracks M work and… |
| R12 | partial | Pull requests (34): list, listStats, filterOptions, summary, routing, routingIdentity, stack, linkedThreads, detail, preview, checks, activity, reviewGuide, generateReviewGuide, threadComments, diffFileContents, filesViewed, changeBreakdown, setFilesViewed, runAction, update, comment, updateComment, submitReview, replyToThread, setThreadResolution, setReaction, invalidate, reportState, subscribeRefreshes, reviewerCandidates, requestReviewers, labelCandidates, setLabels | reviewGuide and generateReviewGuide are `GET`/`POST /api/projects/{id}/pulls/{number}/guide` from F42 [#168](https://github.com/orbyn-dev/orbyn-engineering/issues/168), where generation is a… |
| R13 | partial | Source control, clones, subscriptions (23): sourceControl lookupRepository, cloneRepository, publishRepository, signInStart, signInWait, signInCancel, appAccess, starred; projectClone start, cancel, retry; worktreeSetup.cancel; subscribe ProjectClones, VcsStatus, WorktreeSetup, TerminalMetadata, PreviewEvents, DiscoveredLocalServers, DeviceState, ServerConfig, ServerLifecycle, AuthAccess, ResourceTelemetry | repository lookup and general starred-repository listing remain unqualified: the existing star route only checks whether this app's repository is starred (Core `source_control/forge`), not a… |
| R14 | partial | Orchestration (19): dispatchCommand, getTurnDiff, getFullThreadDiff, searchThreads, searchThread, searchThreadStream, getArchivedShellSnapshot, getThreadFamily, getWorktreeThreadIds, getOlderSettledThreads, getThreadProjection, getWorkflowScript, stopWorkflow, forceStopThread, getTurnItem, launchThread, subscribeShell, subscribeThread, subscribeCommandOutput | getWorkflowScript/stopWorkflow map to M13's `/api/threads/{id}/workflows/{call}/script` and `/stop` for Claude Code workflows (coverage `crates/orbynd/tests/workflows.rs`); N19's… |

## Appendix B: designed screens not yet built

From [design-inventory.md](../design-inventory.md) on main: 160 screens missing, partial or preview-only in code.

### Tool calls, every state (35)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `tc-collapsed` | Collapsed groups | partial | P2 |
| `tc-rows` | Every kind of row, collapsed | partial | P2 |
| `tc-command-open` | Command expanded | partial | P2 |
| `tc-output-capped` | Output capped at 12 lines | missing | P2 |
| `tc-output-all` | All output, scrolling inside the block | partial | P2 |
| `tc-running` | Running, output streaming | partial | P2 |
| `tc-running-group` | Running group, collapsed and open | partial | P2 |
| `tc-failed` | Failed, with stderr in red | partial | P2 |
| `tc-failed-collapsed` | Failed, collapsed | partial | P2 |
| `tc-timeout` | Timed out | missing | P2 |
| `tc-denied` | Blocked by the access mode, needs you | partial | P2 |
| `tc-denied-resolved` | After the choice | partial | P2 |
| `tc-cancelled` | Cancelled | missing | P2 |
| `tc-output-file` | Output too large, saved to a file | missing | P2 |
| `tc-binary` | Binary output | missing | P2 |
| `tc-long-hover` | Very long command, collapsed | partial | P2 |
| `tc-long-open` | Very long command, expanded | partial | P2 |
| `tc-long-lines` | Very long lines in output | partial | P2 |
| `tc-many` | Many groups, Expand all and Collapse all | missing | P2 |
| `tc-focus` | Keyboard focus on a row | partial | P2 |
| `tc-terminal` | Open in terminal | missing | P2 |
| `tc-read` | Read file, with a line range | partial | P2 |
| `tc-edit` | Edit file, with a mini diff | partial | P2 |
| `tc-create` | Create file | missing | P2 |
| `tc-search` | Search, with a result list | partial | P2 |
| `tc-list` | List files | missing | P2 |
| `tc-fetch` | Web fetch, with the page title | partial | P2 |
| `tc-mcp` | MCP tool, with server and arguments | partial | P2 |
| `tc-subagent` | Subagent, with a link to its thread | partial | P2 |
| `tc-image` | Image viewed | partial | P2 |
| `tc-side-chat` | Side chat | partial | P2 |
| `tc-side-needs-you` | Side chat, needs you | partial | P2 |
| `tc-phone-list` | Phone: groups and rows | missing | P2 |
| `tc-phone-sheet` | Phone: output in a sheet | missing | P2 |
| `tc-phone-needs-you` | Phone: needs you | missing | P2 |

### Pull requests, every state (25)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `pr-list-none` | List: no pull requests | partial | P2 |
| `pr-list-offline` | List: offline | missing | P2 |
| `pr-list-signedout` | List: signed out of GitHub | missing | P2 |
| `pr-list-filtered` | List: filtered with no results | partial | P2 |
| `pr-list-failed` | List: checks failed | partial | P2 |
| `pr-list-review` | List: review requested | partial | P2 |
| `pr-list-conflict` | List: merge conflict | missing | P2 |
| `pr-detail-none` | Detail: no pull request | partial | P2 |
| `pr-detail-offline` | Detail: offline | missing | P2 |
| `pr-detail-signedout` | Detail: signed out of GitHub | missing | P2 |
| `pr-detail-filtered` | Detail: files filter with no results | missing | P2 |
| `pr-detail-running` | Detail: checks running | partial | P2 |
| `pr-detail-failed` | Detail: checks failed | partial | P2 |
| `pr-detail-review` | Detail: review requested | partial | P2 |
| `pr-detail-changes` | Detail: changes requested | partial | P2 |
| `pr-detail-approved` | Detail: approved and ready to merge | partial | P2 |
| `pr-detail-conflict` | Detail: merge conflict | partial | P2 |
| `pr-detail-merged` | Detail: merged | partial | P2 |
| `pr-detail-conversation` | Detail: conversation with comments | partial | P2 |
| `pr-detail-files` | Detail: Files changed | partial | P2 |
| `pr-detail-checks` | Detail: Checks with a log | partial | P2 |
| `pr-detail-merge-menu` | Detail: merge options menu | partial | P2 |
| `pr-detail-thread` | Detail: thread linked to the pull request | partial | P2 |
| `pr-phone-list` | Phone: Pull requests list | missing | P2 |
| `pr-phone-detail` | Phone: pull request detail | missing | P2 |

### Tools (17)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `t-memory` | Memory | missing | P3 |
| `t-memory-detail` | A memory in detail | missing | P3 |
| `t-memory-forget` | Forget a memory | missing | P3 |
| `t-memory-states` | Memory: empty, offline and MCP | missing | P3 |
| `t-gate` | Review gate in a thread | missing | P3 |
| `t-gate-log` | Review gate history | missing | P3 |
| `t-gate-settings` | Settings · Review gate | missing | P3 |
| `t-skills` | Skills library | missing | P4 |
| `t-skill` | A skill in detail | missing | P4 |
| `t-diagrams` | Diagrams | missing | P4 |
| `t-diagrams-new` | New diagram | missing | P4 |
| `t-audit-start` | Start a security audit | missing | P5 |
| `t-audit` | Security audit report | missing | P5 |
| `t-audit-finding` | A finding | missing | P5 |
| `t-inspect` | Inspect | missing | P5 |
| `t-inspect-empty` | Inspect: empty and error | missing | P5 |
| `t-phone` | Memory and review on a phone | missing | P3 |

### Side panel, every state (24)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `x-diff-binary` | Diff: binary file | partial | P2 |
| `x-diff-huge` | Diff: huge file collapsed | partial | P2 |
| `x-diff-conflict` | Diff: conflict markers | partial | P2 |
| `x-diff-staged` | Diff: staged and unstaged | missing | P2 |
| `x-diff-search` | Diff: filter files | partial | P2 |
| `x-diff-menu` | Diff: panel menu open | missing | P2 |
| `x-files-empty` | Files: nothing selected | partial | P2 |
| `x-files-readonly` | Files: read-only file | missing | P2 |
| `x-files-large` | Files: large tree | missing | P2 |
| `x-files-menu` | Files: file menu open | partial | P2 |
| `x-browser-loading` | Browser: loading a page | missing | P2 |
| `x-browser-error` | Browser: page error | partial | P2 |
| `x-browser-offline` | Browser: offline | missing | P2 |
| `x-browser-console` | Browser: console open | missing | P2 |
| `x-activity-running` | Activity: running | partial | P2 |
| `x-activity-failed` | Activity: failed step with log | partial | P2 |
| `x-activity-filtered` | Activity: filtered | partial | P2 |
| `x-side-error` | Side chat: error | partial | P2 |
| `x-gate-running` | Review gate: reviewing | missing | P3 |
| `x-gate-passed` | Review gate: passed | missing | P3 |
| `x-gate-selected` | Review gate: finding opened | missing | P3 |
| `x-gate-error` | Review gate: couldn't run | missing | P3 |
| `x-panel-tabs-more` | Panel: tabs overflow | partial | P1 |
| `x-panel-offline` | Panel: offline | missing | P1 |

### Threads, every state (12)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `x-chat-interrupted` | Interrupted, then resumed | partial | P2 |
| `x-chat-code` | Long code block with copy | partial | P2 |
| `x-chat-ratelimit` | Rate limited, with time to wait | partial | P2 |
| `x-chat-context` | Context nearly full | partial | P2 |
| `x-chat-compacting` | Compacting the thread | partial | P2 |
| `x-chat-model-switch` | Model switched mid-thread | partial | P2 |
| `x-chat-edit` | Edit a message and branch | partial | P2 |
| `x-chat-long` | Very long thread, jump to latest | partial | P2 |
| `x-chat-offline` | Offline composer, queued message | missing | P2 |
| `x-chat-slash-none` | Slash command with no match | partial | P2 |
| `x-chat-voice` | Voice dictation | missing | P2 |
| `x-chat-archived` | Archived thread, read-only | partial | P2 |

### Terminal, every state (8)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `x-term-running` | Terminal: a running process | partial | P2 |
| `x-term-failed` | Terminal: a failed command | partial | P2 |
| `x-term-search` | Terminal: search in output | missing | P2 |
| `x-term-menu` | Terminal: tab menu | missing | P2 |
| `x-term-agent` | Terminal: started by the agent | partial | P2 |
| `x-term-ended` | Terminal: process ended | partial | P2 |
| `x-term-maximised` | Terminal: maximised | partial | P2 |
| `x-term-closed` | Terminal: closed | missing | P2 |

### Terminal blocks, slice 1C (16)

Decided 2026-11-12: a block is one command or script the app itself runs (project scripts, Crew check runs, agent tool commands shown in a terminal), with output and duration kept by the service. The Designer redraws 14 frames as app-run blocks (tab named by its source, no prompt line, read-only, Run again on app-run blocks). Blocks for commands typed in an interactive shell need shell integration (OSC 133 for bash, zsh, fish and pwsh) and are **After the beta**. The service does not yet keep per-command output; that backend slice comes first.

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `block-actions` | Block actions on an app-run block | missing | P2 |
| `block-ask-crew` | Ask Crew about a block | missing | P2 |
| `block-collapsed` | A collapsed block | missing | P2 |
| `block-default` | A block | missing | P2 |
| `block-empty` | No blocks yet: "Commands Orbyn runs appear here as blocks" | missing | P2 |
| `block-failed` | A failed block | missing | P2 |
| `block-filter` | Filter blocks | missing | P2 |
| `block-find` | Find in blocks | missing | P2 |
| `block-long` | A long block | missing | P2 |
| `block-menu` | Block menu (no keys on Copy output and Run again) | missing | P2 |
| `block-narrow` | Blocks at a narrow width | missing | P2 |
| `block-running` | A running block with Stop | missing | P2 |
| `block-send-chip` | Send a block to the message | missing | P2 |
| `block-start-crew` | Start Crew from a block (display-only until the Crew backend) | missing | P2 |
| `block-fullscreen` | Full-screen program in a block | After the beta | n/a |
| `block-fullscreen-ended` | A full-screen program ended | After the beta | n/a |

### Phone, more (2)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `x-phone-stream` | Phone: streaming and offline | missing | P2 |
| `x-phone-panel` | Phone: Diff and checks | missing | P2 |

### Orbyn account and other devices (2)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `pair-phone` | Remote access and pairing | preview-only | P2 |
| `phone` | On a phone | preview-only | P2 |

### Connection states (4)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `conn-defs` | Connection states: the four definitions | missing | P1 |
| `conn-compare` | Connection states side by side | missing | P1 |
| `conn-service-down` | Can't reach the Orbyn service | missing | P1 |
| `conn-service-restart` | Restarting the Orbyn service | missing | P1 |

### Segmented control (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `segmented` | thumb transform translateX, label color | missing | P1 |

### Input focus (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `input` | border-color, box-shadow (3px ring) | missing | P1 |

### Error reveal (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `inputerr` | border-color; message opacity and translateY -4px to 0; no shake, no height change | missing | P1 |

### Select open (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `select` | menu opacity, translateY -4px, scale .98 from the trigger; chevron rotate 180deg | missing | P1 |

### Launcher search (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `palette` | scrim opacity; panel opacity and translateY -8px; results opacity, translateY to the next slot | missing | P1 |

### Phone bottom sheet (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `sheet` | sheet translateY 100% to 0; scrim opacity; drag follows the pointer 1:1 | missing | P1 |

### Side panel tabs (3)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `sidetabs` | tab background-color and color; content opacity cross-fade | missing | P1 |
| `sidetabs` | tab flex-basis, max-width and padding to 0, opacity out; neighbours slide in as it collapses (one small strip, layout) | missing | P1 |
| `sidetabs` | tab flex-basis, max-width and padding from 0 to full, opacity in | missing | P1 |

### Terminal drawer (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `termdrawer` | drawer transform translateY 100% to 0 over the thread | missing | P2 |

### Settings navigation (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `settings` | highlight transform translateY; pane opacity and translateY 6px to 0 | missing | P1 |

### Banner (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `banner` | opacity, translateY -8px to 0 in a reserved slot, nothing below shifts | missing | P1 |

### Status labels (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `status` | label opacity cross-fade; background and text colour per state | missing | P1 |

### Send a message (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `send` | message opacity and translateY 8px to 0 from the composer; composer text cleared | missing | P2 |

### Streaming text (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `stream` | each word opacity 0 to 1; send and stop icons cross-fade | missing | P2 |

### Agent step trail (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `steps` | step opacity and translateY 8px to 0; spinner to tick cross-fade | missing | P2 |

### Approval card (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `approval` | card opacity and translateY 8px to 0; amber to neutral background and border; content cross-fade | missing | P2 |

### Question card (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `question` | card as an approval; option background-color; question to answer cross-fade | missing | P2 |

### Plan proposed (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `plan` | card opacity and translateY 8px to 0; status and button cross-fade; step tick | missing | P2 |

### Subagent chips (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `subagents` | chip opacity and translateY 8px to 0; spinner to tick cross-fade | missing | P2 |

### Composer grow (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `composer` | one-line and three-line shells cross-fade; the thread translateY by the extra height; no height animation | missing | P2 |

### Diff hunk expand (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `hunk` | revealed lines opacity; the lines below translateY by the revealed height | missing | P2 |

### Review-gate findings (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `review` | finding opacity and translateY 8px to 0 as each check reports; gate label cross-fade | missing | P3 |

### Launcher update progress (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `update` | progress transform scaleX; button, bar and caption opacity cross-fade; blue while downloading | missing | P1 |

### Page change (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `pagechange` | outgoing opacity; incoming opacity and translateY 6px to 0 | missing | P1 |

### Phone push and pop (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `pushpop` | pushed screen transform translateX 100% to 0; the screen below stays still | missing | P1 |

### Theme switch (1)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `themes` | background-color, color, border-color, box-shadow on every surface | missing | P1 |

### Reduced motion (4)

| Frame | Screen | State | Priority |
| --- | --- | --- | --- |
| `reduced` | card opacity and translateY 8px to 0; amber to neutral background and border; content cross-fade | missing | P2 |
| `reduced` | card opacity and translateY 8px to 0; amber to neutral background and border; content cross-fade | missing | P2 |
| `reduced` | sheet translateY 100% to 0; scrim opacity; drag follows the pointer 1:1 | missing | P1 |
| `reduced` | sheet translateY 100% to 0; scrim opacity; drag follows the pointer 1:1 | missing | P1 |

## Appendix C: unfinished rows that have no design yet

Checked 2026-11-11 against the 573 frame ids in `engineer-designs.html` in `orbyn-designs`. The Designer has this list. Build these only from merged frames.

| Rows | Needs designing | Closest existing frames |
| --- | --- | --- |
| H20–H27, H31, N11 | Device panel: setup hub (platform, agent access), live iOS Simulator / Android Emulator, device tabs and float over chat, touch/type/Home/Back/Recents/rotate/power, 3D view with fold and hinge, tools drawer (appearance, text, accessibility, location, permissions, push), agent-device permission toggle, tool versions check and install | `x-browser-device` only |
| H33, H34, K49 | SnapShots: window capture to a draft with app metadata, per-platform setup, Settings (shortcut, sound, flash, animations) | none |
| H35 | Preview more menu: hard reload, DevTools, separate window, move to another panel, device tools | partial in preview frames |
| I10 | MCP app access approvals (elicitation) | none |
| I17, I20, I22, I25–I27, I46–I50, I62 | Per-provider states: Cursor per-instance sign-in and API key override; Grok, Antigravity, Pi, Muse, fx, Kimi and the extra engines (sign-in, models, limited-support label, unobserved-platform notes) | generic `s-x-provider-*` |
| I31 | ACP v1/v2 negotiation: "Preview protocol" label and fallback notice | none |
| I38, I39 | Provider install (start, progress, cancel, remove) and Update all | `s-x-provider-add`, `s-x-provider-detail-update` |
| J4, J12, J14 | Usage: environment filter with scanning and stale figures; Claude subscription windows; Antigravity, OpenCode Go, Cursor and Grok limits | `usage-*`, `thread-details-limit-*` |
| K35 | Welcome wizard steps for computers and agents | `welcome-*` (project steps only) |
| F3 | Multi-account host picker (an account per host, host off) | none |
| L27 | Scoped access: the 16 permission scopes and guarded actions | none |
| C10, C40 | Claude task lists row; rich question content (Markdown, charts, SVG/HTML in an isolated root, remote images as links) | `question`, `tc-plan` |
| C4 | Orchestrator summaries: action-specific wording, failures not counted | `tc-group-counts` |
| P6 | Update tracks: Stable and Nightly switching | `update-*`, `s-x-maint-updates*` |
| K25, P23, P24 | Dock badge and native View/Edit/Window menus: a short spec sheet, not full frames | none |

No design needed (behaviour, packaging or protocol only): B8, O-rows, P3 engine side, P22, P25, P26, P35, R1–R14. Not doable stays as recorded: A54, P20, I24.
