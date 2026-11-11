# Source control

Source control on a thread's checkout, always through the person's own installed tools and login.

| Module | Owns |
| --- | --- |
| `scm` | Source control actions on a thread's checkout: git actions and branches (`mod.rs`), pull request reads and linking (`pulls`), comments, reviews and merges (`review`), stacks and checkouts (`stack`), pull request watching (`watch`), forge status and publishing (`forges`), the kept answers pull request and forge reads come from (`read_cache`) |
| `scm::writing` | Diff-derived text for a confirmed Git action: the writing request (`mod.rs`), the bounded checkout reads (`candidate`) and the writing style (`style`) |
| `git` | Git through the installed CLI, worktrees in the sibling `<repo>-wt` root |
| `jj` | Non-colocated Jujutsu checkouts, read through the installed `jj` |
| `forge` | GitHub and GitLab through their own CLIs: `Forge` and the public functions (`mod.rs`), one CLI call (`process`), answer parsing (`parse`, `github`, `gitlab`), list details (`details`), reads (`pulls`), comments and reviews (`review`), edits and merges (`merge`), creating and publishing (`create`), sign-in status (`status`) |
| `forge::cache` | Short-lived memory of GitHub pull request details |
| `pull_guides` | An agent's reading order for one pull request |

Each folder's `mod.rs` is its front door: the old paths such as `source_control::forge::list_pulls` still resolve through its re-exports. Larger folders keep their tests in a `tests.rs` beside `mod.rs` (`forge` also has `revision_tests.rs`); small modules such as `forge::cache`, `scm::writing::style`, `git`, `jj` and `pull_guides` keep theirs inline.

The crate's [group table](../../README.md#groups) lists every group.
