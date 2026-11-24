---
title: How Rin works
description: The containers Rin runs, how a message travels, and what keeps agents and keys apart.
---

`rin generate` turns `rin.yml` into a Docker Compose project in `~/.rin/teams/<name>`, and
`rin up` starts it. A team is a handful of containers on a private Compose network:

```
 you ──► rin send · rin log · rin status
              │
              ▼  127.0.0.1:10302
          ┌───────┐
          │  hub  │  routes, queues and logs every message
          └───┬───┘
    ┌─────────┼──────────┬───────────┐
    ▼         ▼          ▼           ▼
 rinlead   backend   frontend     devops     one container per agent
                                     │
                                     ▼
                                  bifrost ──► model providers (API-key agents only)
```

Agents message each other through the hub too: every message, whoever sends it, goes
through it.

## The containers

**The hub** carries every message. It checks who is calling, enforces `maxHops`,
`maxSessions` and the timeout, keeps the queues, watches for deadlocks and logs everything
to a database in its volume. It is the only container with a published port, and only on
`127.0.0.1`.

**One container per agent.** Each runs `rin runtime` as the user `agent`, with its home in
a Docker volume mounted at `/home/agent` and `~/work` as the working directory. On start,
the runtime writes Rin's settings into the harness's config; then it waits for the hub. For
each message, it starts the harness once, headless, and sends its final answer back.
Your `INSTRUCTIONS.md` is mounted read-only where the harness reads its instructions.

**Bifrost**, only when an agent uses an API key, is an LLM gateway holding your provider
keys. API-key agents reach their provider through it, with a key of their own that is only
good for Bifrost. See [Subscriptions and API keys](/guides/authentication/).

## A message's trip

1. You run `rin send backend "…"`. Rin sends it to the hub with your token, as hop 1.
2. The hub checks the receiver exists, isn't the sender, and the hop count is within
   `maxHops`. It logs the message.
3. If `backend` already runs `maxSessions` sessions, the message waits in its queue.
4. The hub calls `backend`'s runtime, which starts a fresh harness session with the message
   as its prompt.
5. While working, the session may call `send_message`. That is a new message from `backend`,
   one hop further, and the session waits for its reply.
6. When the session ends, its final answer goes back through the hub, which logs it, to you.

The timeout counts from step 1: queueing, the session and any messages it sends all spend
the same time.

## Who may call whom

- Every agent, and you, have a **token**. The hub works out the caller from the token *and*
  checks that the request comes from that agent's own container. Your token is accepted only
  from outside the team's containers.
- A runtime accepts calls only from the hub, with its own agent's token.
- No container gets the Docker socket, and only the hub publishes a port.
- Tokens, virtual keys and your provider keys are kept in files readable by you only, and
  never written into `docker-compose.yml`.

Agents run their harness with its permission prompts turned off, since nobody is there to
answer them. Inside its container an agent can do anything its user can: treat the container
as a machine you handed to the agent.

## The images

`rin generate` writes a Dockerfile into the team directory; `rin up` builds from it. Every
image is Debian with common tools (`git`, `curl`, `jq`, `python3`, `ripgrep`, `vim`, …) and a
Linux build of `rin` compiled from the GitHub tag of your team's `version`. Each harness
image adds that harness at a version pinned by the Rin release, so the same `rin.yml` gets
the same tools on every machine.

## Where things live

| What | Where |
| :--- | :---- |
| Your config and instructions | Your project directory, anywhere |
| The generated team | `~/.rin/teams/<name>/` ([layout](/reference/files/)) |
| Provider keys | `~/.rin/keys.env` |
| Agent homes | Docker volumes `rin-<name>-<id>-home` |
| Message log | Docker volume `rin-<name>-hub` |
