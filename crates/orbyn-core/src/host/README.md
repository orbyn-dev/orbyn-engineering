# Host

The operating system side: processes, their environment, terminals and the desktop shell.

| Module | Owns |
| --- | --- |
| `process` | Child processes as owned resources that die with their owner |
| `child_env` | What a spawned program inherits |
| `confine` | The restriction a crew worker's engine process runs under (macOS), and the refusal elsewhere |
| `presence` | One in-memory record per action of a person's approval given in the desktop app: bound fields, one nonce, 60 seconds, used once |
| `terminal` | PTY sessions with bounded scrollback; ConPTY on Windows |
| `shell` | The desktop shell's side of the service |

The crate's [group table](../../README.md#groups) lists every group.
