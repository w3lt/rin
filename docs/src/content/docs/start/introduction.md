---
title: What is Rin?
description: Rin runs a team of AI coding agents on your machine, each in its own Docker container, from one config file.
---

Rin runs a **team of AI agents** on your own machine. You describe the team in one file,
`rin.yml`, and Rin turns it into a set of Docker containers: one per agent, plus a small
hub that carries messages between them.

One agent is the **leader**. You usually talk to the leader, and the leader breaks the work
down and hands parts of it to the other agents, the **members**. Any agent can message any
other, and you can message any of them yourself.

## What you get

- **One container per agent.** Every agent has its own Linux environment and its own home
  directory, kept in a Docker volume. Stopping the team does not lose it.
- **A choice of harness per agent.** A harness is the coding-agent program the agent runs:
  [Claude Code, Codex, Antigravity or pi](/rin/reference/harnesses/). One team can mix them.
- **Subscriptions or API keys.** Agents sign in with their tool's own subscription, or use
  an API key you keep in one file on your machine. Keys never enter the agents' containers.
- **Messages with a reply.** A message starts a fresh session on the receiving agent, and its
  final answer comes back to the sender. Rin limits how long that may take and how far a
  chain of messages may go, and warns you when agents are stuck waiting on each other.
- **A log of everything.** Every message and reply is kept, so you can follow the team's
  conversation live or read it later.

## What a session looks like

```sh
rin init my-team            # write a starter rin.yml and one INSTRUCTIONS.md per agent
cd my-team
rin generate --instructions agents
rin up                      # build the images (first time) and start the team
rin send rinlead "Plan a todo app with a REST API and a web UI"
rin log -f                  # watch the leader delegate and the members answer
rin down                    # stop; homes and the log are kept
```

## What Rin is not

- **Not a cloud service.** Everything runs locally under Docker. Rin publishes one port, on
  `127.0.0.1` only.
- **Not a harness.** Rin does not talk to models itself. It starts the harness you chose,
  gives it Rin's two messaging tools and your instructions, and passes the answers around.
- **Not a sandbox for untrusted code.** Agents run their harness with permission prompts
  turned off, inside their container. The container is the boundary: treat it as a machine
  you gave the agent.

## Next

- [Install Rin](/rin/start/installation/)
- [Run your first team](/rin/start/quickstart/)
- [How the pieces fit together](/rin/concepts/architecture/)
