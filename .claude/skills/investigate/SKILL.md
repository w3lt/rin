---
name: investigate
description: Find the root cause of a Rin bug before fixing it - pin down the symptom, locate it by feature and by host or container, prove it with a failing test, then fix. Use when a bug, crash, failing test or unexpected behavior is reported, before proposing a fix.
argument-hint: "[issue number or symptom]"
---

# Investigate

Find why it happens before changing anything. From the user: $ARGUMENTS

Current state:

!`git status --short`

!`git log --oneline -1`

If the user asked only for a diagnosis, stop after step 4: leave the failing test uncommitted
and name it in the report.

## 1. Pin down the symptom

- An issue number: `gh issue view <n>`. Bug reports follow `.github/ISSUE_TEMPLATE/bug.md`:
  the command, its output, what was expected, `rin --version`, OS, Compose version, harness.
- Get the exact command, its full output and the expected result. Ask the user only for what
  you cannot find yourself, one question at a time.
- Compare the reporter's `rin --version` with `version` in `Cargo.toml`. A team's images run
  `rin` built from `RIN_GIT_REV` (default the `v<version>` tag), not from this tree, so the bug
  may be fixed on `main` already: `git log v<version>..origin/main -- <paths>`.

## 2. Locate it

Find which part runs the failing code path; that decides how you can reproduce it.

| Part | Feature | Code | Reproduce with |
| :--- | :------ | :--- | :------------- |
| CLI on the host: `generate`, `up`, `status`, `log` … | `host` | `src/host_cli.rs`, `src/host_cli/`, `src/compose.rs`, `src/shared/config.rs` | a unit test, or `cargo run` against a throwaway `RIN_HOME` |
| Hub: routing, sessions, queues, tokens, message log | `hub` | `src/hub.rs`, `src/hub/` | a unit test, or an end-to-end test in `src/tests.rs` |
| Agent runtime: harness launch, harness settings | `agent` | `src/agent_runtime.rs`, `src/agent_runtime/` | a unit test, or an end-to-end test in `src/tests.rs` |
| Image build, container start, Compose wiring | — | `images/Dockerfile`, `src/compose.rs` | `scripts/verify_docker.sh` only |

- Throwaway `RIN_HOME`, never the user's `~/.rin`:
  `cd examples/team && RIN_HOME="$(mktemp -d)" cargo run -q -- generate --instructions agents`.
- `src/tests.rs` runs a real hub and runtimes over HTTP with a fake `claude` shell script as
  every harness; it needs all three features (`cargo test --all-features <name>`).
- A team that is already up gives evidence read-only: `rin status`, `rin log [<id>]`, and
  `docker compose -p rin-<name> -f ~/.rin/teams/<name>/docker-compose.yml logs <service>`.
  Ask before `rin up`, `rin down` or `rin generate` on it: a restart drops messages in flight.
  Never remove its volumes (the message log, agent homes) or edit `~/.rin/teams/<name>/`.
- A regression with a reproducing command: bisect in a throwaway worktree so the user's tree
  is untouched (`git worktree add <tmp> HEAD`, then `git bisect run <command>` there, with any
  script it runs kept outside the worktree), then `git bisect reset` and `git worktree remove <tmp>`.

## 3. Find the root cause

- One hypothesis at a time, each tested by the smallest experiment: a test, a log line, a
  `dbg!`. Remove the experiments afterwards.
- The root cause is where the state first goes wrong, not where the error surfaces. Look for
  the same flaw in other callers (`git grep`) and other feature subsets.
- After three hypotheses that did not hold, stop and report what you ruled out instead of
  guessing further.

## 4. Prove it

- Write a test that fails for that cause: in the module's `#[cfg(test)]` tests, or in
  `src/tests.rs` when it needs the hub and a runtime together. Run it with
  `cargo test --all-features <name>` and check it fails with the symptom, not some other error.
- Container-only, so no test here reaches it: say so. The fix can only be checked by
  `scripts/verify_docker.sh` after HEAD is pushed (see AGENTS.md).
- Cannot reproduce: report what you tried and what the reporter could add. Do not fix on a guess.

## 5. Fix

- The smallest change at the root cause that makes the test pass. No unrelated cleanup.
- Then run the `review-loop` skill, and the `pr` skill when the user wants a PR
  (`Closes #<issue>` when it came from one).

## Report

End with: the symptom; the root cause at `file:line`; the evidence (the failing test, or the
command that shows it), marking what you inferred rather than saw; the fix, or "not fixed" and
why; container or Linux paths nobody ran here; and harness issues per AGENTS.md "Harness
feedback", or "Harness: none".
