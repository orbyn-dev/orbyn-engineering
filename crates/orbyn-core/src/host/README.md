# Host

The operating system side: processes, their environment, terminals and the desktop shell.

| Module | Owns |
| --- | --- |
| `process` | Child processes as owned resources that die with their owner |
| `child_env` | What a spawned program inherits, including the login-shell `PATH` |
| `confine` | The restriction a crew worker's engine process runs under (macOS), and the refusal elsewhere |
| `presence` | One in-memory record per action of a person's approval given in the desktop app: bound fields, one nonce, 60 seconds, used once |
| `blocks` | Command blocks: OSC 133/633 marks read from PTY output, bounded redacted text per command, in memory |
| `terminal` | PTY sessions with bounded scrollback; ConPTY on Windows |
| `shell` | The desktop shell's side of the service |

On Unix the service runs the person's login shell once, interactive login (`$SHELL -ilc`),
with one fixed command that contains no user input and reads back only `PATH`. It runs with
a cleared environment and a 3 second limit, falls back silently on any failure, and can be
switched off with `ORBYN_NO_LOGIN_SHELL=1`.

The crate's [group table](../../README.md#groups) lists every group.
