---
title: CLI
description: Every rin command, its arguments and flags.
---

```
rin [--team <NAME>] <COMMAND>
```

| Command | What it does |
| :------ | :----------- |
| [`rin init`](#rin-init) | Write a starter project |
| [`rin generate`](#rin-generate) | Check `rin.yml` and prepare the team; make it current |
| [`rin use`](#rin-use) | Switch the current team |
| [`rin up`](#rin-up) | Start the team |
| [`rin down`](#rin-down) | Stop the team |
| [`rin shell`](#rin-shell) | Open a shell in an agent's container |
| [`rin send`](#rin-send) | Message an agent and print its reply |
| [`rin agents`](#rin-agents) | List the team's agents |
| [`rin status`](#rin-status) | Show state, sessions, queues and warnings |
| [`rin log`](#rin-log) | Show the message history |
| [`rin completions`](#rin-completions) | Print shell completions |

`rin --help` and `rin <command> --help` print the same, and `rin --version` the version.

## Global options

**`--team <NAME>`**\
Act on this team instead of the current one. `NAME` is the team's `name` from its
`rin.yml`; the team must have been generated. `init`, `generate`, `use` and `completions` ignore it.

Every command that acts on a team uses the current team unless `--team` is given, so it
works from any directory. See [Running several teams](/guides/teams/).

On failure, commands print the error and exit with status 1.

## rin init

```
rin init [-d <DIR>] [<NAME>]
```

Writes a starter project to `<DIR>/<NAME>`: a `rin.yml` with four agents, and one
`agents/<id>/INSTRUCTIONS.md` per agent. The `rin.yml` pins this Rin's version and points
editors at its schema.

| Argument | Default | Description |
| :------- | :------ | :---------- |
| `<NAME>` | `rinteam` | Team name, also the project directory's name. Lowercase letters, digits and `-`, starting with a letter, at most 31 characters. |
| `-d`, `--dir <DIR>` | `.` | Directory to create the project directory in. |

`rin init` never overwrites anything: it refuses if `<DIR>/<NAME>` is already a Rin project
or any file it would write exists, and then writes nothing.

## rin generate

```
rin generate --instructions <DIR> [-f <FILE>]
```

Checks the config and the instructions, then writes the team's directory to
`$RIN_HOME/teams/<name>` and makes it the current team.

| Argument | Default | Description |
| :------- | :------ | :---------- |
| `--instructions <DIR>` | required | Directory with one `<agent id>/INSTRUCTIONS.md` per agent. |
| `-f`, `--file <FILE>` | `rin.yml` | The team config. |

It refuses, and writes nothing, when:

- the config breaks the [schema](/reference/config/), or has a duplicate agent or
  [`llmProviders`](/reference/config/#llmproviders) id;
- a pi agent's `model` is not `<provider>/<model>` with a known provider, or an agent of
  another harness uses `ollama` or a custom provider;
- its `version` is not this `rin`'s version;
- an agent has no `INSTRUCTIONS.md`, or a directory in `<DIR>` is no agent;
- the team's Docker names would clash with another generated team's.

It warns when `keys.env` is missing a key the team's API-key agents need, naming the
variables (see [When a key is missing](/guides/authentication/#when-a-key-is-missing)).

Running it again with nothing changed changes nothing. Tokens and Bifrost virtual keys are
kept; delete `tokens.json` first to [rotate tokens](/guides/lifecycle/#rotate-tokens).
Changes reach a running team with `rin up`.

Set [`RIN_GIT_REV`](/reference/files/#environment) to build the images from another
Rin revision.

## rin use

```
rin use <NAME>
```

Makes `<NAME>` the current team. Prints `current team: <NAME>`.

## rin up

```
rin up
```

Starts the team: builds the images (the first time, or when something in them changed) and
starts or recreates the containers. Agents no longer in the team are stopped and removed.
You can run it while the team is up: unchanged containers keep running. A change to the
config or the tokens restarts the hub, and an agent whose settings changed restarts too,
which drops messages in flight. Check `rin status` for running sessions first. Editing only
an `INSTRUCTIONS.md` restarts nothing.

It refuses when port `10302` on `127.0.0.1` is taken, naming the other Rin team that is up
if there is one. If starting fails and nothing was running before, it stops what it started.

It also refuses, before starting anything, when `keys.env` is missing a key the team's
API-key agents need (or doesn't exist and one is needed), naming the variables.

## rin down

```
rin down
```

Stops and removes the team's containers. Agent homes, the message log and Bifrost's data are
kept in their volumes; `rin up` brings the team back as it was.

## rin shell

```
rin shell <ID>
```

Opens `bash` in agent `<ID>`'s container, as the agent's user, in `~/work`. Use it to sign an
agent in, look at its files, or run its tool by hand. The team must be up.

## rin send

```
rin send <ID> <TEXT>...
```

Sends a message to agent `<ID>`, waits for its final answer and prints it. The words of
`<TEXT>` are joined with spaces.

From your machine, the message comes from you (`user`) as hop 1. Inside an agent's container
(a pi agent's tool call, or your `rin shell`), it comes from that agent, and inside a session
one hop further than the message that started it. See
[Talking to agents](/guides/messaging/).

It waits up to the team's `messaging.timeout`. Messages are limited to 120 KiB.

## rin agents

```
rin agents
```

Lists the team as the hub sees it:

```
ID        LABEL               TYPE    HARNESS      SESSIONS  QUEUED
rinlead   Rin Lead            leader  claude-code  1/3       0
backend   Backend Engineer    member  codex        0/3       0
```

`SESSIONS` is running/`maxSessions`, `QUEUED` the messages waiting. Inside a container, the
calling agent is marked `you`. Deadlock warnings follow the table. The team must be up.

## rin status

```
rin status
```

Each agent's container state, with the hub's sessions and queues:

```
team my-team (rin 0.1.2)
ID        LABEL               TYPE    HARNESS      STATE    SESSIONS  QUEUED
rinlead   Rin Lead            leader  claude-code  running  1/3       0
backend   Backend Engineer    member  codex        running  0/3       0
frontend  Front-end Engineer  member  antigravity  stopped  -         -
devops    DevOps Engineer     member  pi           down     -         -
bifrost: running
```

`STATE` is Docker's state for the container (`running`, `restarting`, …), `stopped` if it
exited, `down` if it doesn't exist. Sessions show `-` when the hub can't be reached, and a
`hub:` line says why. Deadlock warnings follow the table; the `bifrost:` line appears only
for teams with API-key agents. Works whether or not the team is up.

## rin log

```
rin log [-f] [<ID>]
```

Prints the message history, oldest first: messages, replies, errors and warnings, with
UTC times.

| Argument | Description |
| :------- | :---------- |
| `<ID>` | Only messages to or from this agent. |
| `-f`, `--follow` | Keep printing new messages. Keeps following across restarts of the team. |

The team must be up. The history itself is kept across `rin down`.

## rin completions

```
rin completions <SHELL>
```

Prints the completion script for `bash`, `zsh`, `fish`, `elvish` or `powershell`.
See [Installation](/start/installation/#shell-completions).
