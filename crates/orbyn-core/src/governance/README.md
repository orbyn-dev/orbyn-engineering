# Governance

What may happen and who is told: every engine action passes here before it has an effect.

| Module | Owns |
| --- | --- |
| `policy` | Pure policy evaluation: deny beats ask beats allow |
| `approvals` | The single gate every engine action passes: policy, then a person |
| `audit` | The audit log as a person reads it: stored rows, each `detail` masked on the way out |
| `computer` | The computer gateway: resolve, evaluate, audit, then act |
| `network` | Every request Orbyn makes unasked, listed for a person to read |

The crate's [group table](../../README.md#groups) lists every group.

`computer/browser_stderr` retains a bounded, cleaned stderr tail while a browser
starts. Startup failures carry it in their existing error; a successful startup
clears it and keeps draining the pipe until the browser session closes.
`computer/cdp/start` waits for that start: a browser still running when its first
bounded wait ends gets one more bounded wait, while an exited browser fails at once
with the retained stderr and the startup flags. A browser that died by a fault gets
one more process, because that crash is the browser's; a refusal is reported at once.
