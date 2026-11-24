---
title: Quickstart
description: Create a starter team, start it, sign the agents in and send the first message.
---

This page takes you from nothing to a running team of four agents. It assumes
[Rin and Docker are installed](/rin/start/installation/).

## 1. Create a project

```sh
rin init my-team
cd my-team
```

`rin init` writes a starter project to `./my-team`:

```
my-team/
├── rin.yml
└── agents/
    ├── rinlead/INSTRUCTIONS.md
    ├── backend/INSTRUCTIONS.md
    ├── frontend/INSTRUCTIONS.md
    └── devops/INSTRUCTIONS.md
```

The starter team has a leader and three members:

| Agent      | Type   | Harness     | Signs in with |
| :--------- | :----- | :---------- | :------------ |
| `rinlead`  | leader | Claude Code | subscription  |
| `backend`  | member | Codex       | subscription  |
| `frontend` | member | Antigravity | subscription  |
| `devops`   | member | pi          | API key       |

Edit it as you like: drop the agents whose tools you don't use, change models and limits.
Every agent in `rin.yml` needs its `agents/<id>/INSTRUCTIONS.md`, and every directory in
`agents/` must be an agent. [Design your team](/rin/guides/team/) covers the options.

## 2. Add your API keys

Only agents with `authentication: apiKey` need this. In the starter team that is `devops`,
whose model is `anthropic/claude-sonnet-5-5`, so it needs an Anthropic key:

```sh
mkdir -p -m 700 ~/.rin
touch ~/.rin/keys.env && chmod 600 ~/.rin/keys.env
echo 'ANTHROPIC_API_KEY=sk-ant-...' >> ~/.rin/keys.env
```

If you have no key, remove `devops` from `rin.yml` and delete `agents/devops`.
[Subscriptions and API keys](/rin/guides/authentication/) explains how keys are kept away
from the agents.

## 3. Generate the team

```sh
rin generate --instructions agents
```

```
team my-team written to /Users/you/.rin/teams/my-team (now the current team)
```

Rin checks `rin.yml`, copies the instructions and writes everything Docker needs into
`~/.rin/teams/my-team`. That team is now your **current team**: the other commands act on
it from any directory.

## 4. Start it

```sh
rin up
```

The first run builds the images, which takes a few minutes. Later runs reuse them.

## 5. Sign the agents in

Agents on a subscription need you to sign in once. Open a shell in the agent's container,
run its tool and follow its sign-in:

```sh
rin shell rinlead     # then run: claude
rin shell backend     # then run: codex
rin shell frontend    # then run: agy
```

The sign-in is stored in the agent's home volume, so it survives `rin down` and `rin up`.

## 6. Send a message

```sh
rin send rinlead "Reply with exactly: pong"
```

```
pong
```

`rin send` waits for the agent's final answer and prints it. Now give the leader something
real and watch the team work in a second terminal:

```sh
rin send rinlead "Sketch a REST API for a todo app and have backend review it"
```

```sh
rin log -f
```

## 7. Check on it, then stop

```sh
rin status    # each agent's state, running sessions and queued messages
rin down      # stop the team
```

`rin down` keeps every agent's home and the message log. `rin up` brings the same team back.

## Next

- [Design your team](/rin/guides/team/): roles, harnesses, instructions and limits.
- [Talking to agents](/rin/guides/messaging/): sessions, hops, timeouts and deadlocks.
- [CLI reference](/rin/reference/cli/)
