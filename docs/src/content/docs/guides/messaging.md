---
title: Talking to agents
description: How messages, replies, sessions, queues, hops, timeouts and deadlock warnings work.
---

## From your machine

```sh
rin send <id> "<text>"
```

`rin send` delivers the message and waits for the reply, which it prints. The words after
the id are joined with spaces, so quoting is only needed for shell characters:

```sh
rin send rinlead Review the open pull requests and summarise them
```

`rin agents` lists the team: each agent's id, label, type, harness, running sessions and
queued messages.

## From inside the team

Agents message each other with two tools Rin gives their harness:

- **`send_message`** sends a message to another agent and waits for its final answer.
- **`list_agents`** lists the team and how busy each agent is, marking the caller as `you`.

pi has no MCP support, so pi agents use the same `rin send <id> "<text>"` and `rin agents`
commands you use, from their shell. Inside a container they need no setup: they act as that
agent.

You can also run `rin send` and `rin agents` in a [`rin shell`](/reference/cli/#rin-shell);
the message then comes from that agent, not from you.

## One message, one session

Every message starts a **fresh session** on the receiving agent: its harness runs once,
headless, with the message as the prompt. The session's **final answer** is the reply. When
the session ends, everything it started is stopped.

A session has no memory of earlier messages, apart from what is on disk in the agent's
home. Put what the receiver needs in the message, or have agents leave notes in files.

The message the receiver sees starts with a line naming the sender (its id and label, or
`user` / `User` for you) and saying that its final answer is sent back to them.

## Sessions and queues

`maxSessions` (default 3) caps how many sessions an agent runs at once. Further messages to
it wait in a queue, first come first served, and start as slots free up.

`rin status` and `rin agents` show `SESSIONS` as running/max and `QUEUED` as the queue length.

## Timeout

`messaging.timeout` (default `30m`) is how long one message may take **from the moment it is
sent**, waiting in the queue included. When it runs out the session is stopped and the sender
gets an error instead of a reply:

- `timeout: no reply from <id> within 30m`
- `timeout: <id> was busy for the whole 30m; the message never started`
- `timeout: <id> was busy until <left> before the 30m timeout; the message was not started`.
  A queued message that would only get a small sliver of its time left is failed rather than
  started, so it doesn't spend tokens on an answer nobody receives.

## Hops

Messages form chains: you ask the leader, the leader asks `backend`, `backend` asks
`frontend`. Each link is a **hop**. Your message is hop 1, a message sent from within that
session is hop 2, and so on. `messaging.maxHops` (default 5) is the longest chain allowed; a
message past it fails with

```
hop limit reached: this message would be hop 6, maxHops is 5
```

This stops agents from passing work around forever. An agent cannot message itself.

## Deadlocks

Agents wait for replies, so they can get stuck waiting on each other: `backend` and
`frontend` both at their `maxSessions`, each with a session waiting on a message queued at
the other. Rin watches for such cycles and reports them:

```
⚠ possible deadlock: backend ⇄ frontend (both 1/1, 2 queued for 3m)
```

The warning appears in `rin status`, `rin agents` and the log. Each cycle is logged once,
and again only if it comes back after breaking. The timeout eventually breaks it; to avoid
it, raise `maxSessions` or tell agents in their instructions not to call back the agent
that asked them.

## The log

```sh
rin log             # the whole history
rin log backend     # only messages to or from backend
rin log -f          # keep following new messages
```

Each entry shows its time (UTC), the message number, sender and receiver, and the text:

```
2026-11-17 22:31:05Z  #12  user → rinlead  (hops 1)
    Plan a todo app with a REST API and a web UI
2026-11-17 22:31:40Z  #13  rinlead → backend  (hops 2)
    Draft the REST API for todos: ...
2026-11-17 22:34:02Z  #13  backend → rinlead  reply
    Here is the API: ...
```

A message that failed shows `#<n>  error → <sender>` with the error, and deadlock warnings
show as a line of their own. The log
is kept in the hub's volume, so it survives `rin down`. `rin log -f` keeps following when the
team restarts.
