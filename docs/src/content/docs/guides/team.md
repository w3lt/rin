---
title: Design your team
description: Choose agents, their roles, harnesses, models and limits, and write their instructions.
---

A team is two things: `rin.yml`, which says who is on it, and one `INSTRUCTIONS.md` per
agent, which tells that agent who it is and what it owns.

## A minimal team

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/w3lt/rin/refs/tags/v0.1.2/rin-schema.json
rin: tohsaka
name: duo
version: 0.1.2

agents:
  - id: lead
    type: leader
    label: Tech Lead
    harness: claude-code
    authentication: subscription
    model: claude-opus-5-5
    resources:
      cpus: 2
      ram: 4g

  - id: dev
    type: member
    label: Engineer
    harness: codex
    authentication: subscription
    model: gpt-6-luna
    resources:
      cpus: 2
      ram: 4g
```

with `agents/lead/INSTRUCTIONS.md` and `agents/dev/INSTRUCTIONS.md` next to it.

The first line points your editor at the schema for this Rin version, so editors with a
YAML language server (VS Code with the YAML extension, for example) complete and check the
file as you type. `rin init` writes it for you.

Every field is described in the [rin.yml reference](/reference/config/).

## Leader and members

A team has **exactly one** agent with `type: leader`; the others are `type: member`.

The type is a role, not a permission: every agent can message every other agent, and you can
message any of them. What makes the leader lead is its instructions. Tell it to break work
down, delegate with `send_message`, and review what comes back. Tell members what they own
and that they may ask teammates for what they need.

## Choosing a harness

Each agent runs one [harness](/reference/harnesses/):

| `harness`     | Tool         | Messages teammates with      | `authentication`         |
| :------------ | :----------- | :--------------------------- | :----------------------- |
| `claude-code` | Claude Code  | `send_message`, `list_agents` (MCP) | `subscription` or `apiKey` |
| `codex`       | Codex        | `send_message`, `list_agents` (MCP) | `subscription` or `apiKey` |
| `antigravity` | Antigravity  | `send_message`, `list_agents` (MCP) | `subscription` or `apiKey` |
| `pi`          | pi           | `rin send`, `rin agents` (shell)    | `apiKey` only            |

`model` is passed to the harness as is, so use the name that harness expects. pi needs a
provider prefix: `anthropic/…`, `openai/…`, `google/…`, `ollama/…` (Ollama Cloud) or one of
your [`llmProviders`](/reference/config/#llmproviders). See
[Subscriptions and API keys](/guides/authentication/).

## Writing instructions

`agents/<id>/INSTRUCTIONS.md` becomes the harness's own instruction file in the container
(`CLAUDE.md`, `AGENTS.md` or `GEMINI.md`, read-only). Write it as you would for that tool
on your own machine, and add what only Rin knows:

- **Who the agent is and what it owns.** "You own APIs, data and services."
- **How to reach teammates.** For Claude Code, Codex and Antigravity: the `send_message` and
  `list_agents` tools. For pi: `rin send <id> "<text>"` and `rin agents` in the shell.
- **For the leader, how to lead.** Delegate to the right teammate, review what comes back,
  then report.

The leader of the starter team, for example:

```md
# Rin — Chief Architect

You lead this team.
Break work down and delegate to the right teammate with the `send_message` tool; `list_agents` shows who is on the team and how busy they are.
Review what comes back before you report to the user.
```

Each message an agent receives starts with a line saying who it is from and that its final
answer goes back to them, so instructions don't need to explain that.

Rin copies the files into the team when you run `rin generate`. After editing one, run
`rin generate` and `rin up` again.

## Sessions, timeouts and hops

Three numbers shape how busy the team can get:

- **`maxSessions`** (per agent, default 3): how many messages an agent works on at once.
  More wait in a queue, first come first served.
- **`messaging.timeout`** (default `30m`): how long a message may take, waiting in the queue
  included.
- **`messaging.maxHops`** (default 5): how long a chain of messages may get. Your message is
  hop 1; a message an agent sends while working on it is hop 2, and so on.

[Talking to agents](/guides/messaging/) explains how they interact.

## Resources

`resources.cpus` and `resources.ram` become the container's CPU and memory limits. Coding
agents run builds and tests, so give them room: `2` CPUs and `4g` is a comfortable start for
an engineer, `1` and `2g` is the starter team's default.

## Names

`name` and every agent `id` are lowercase letters, digits and `-`, starting with a letter,
at most 31 characters. They become Docker names (`rin-<name>-<id>`), so two teams must not
produce the same one: `rin generate` refuses a team whose names clash with another team's.
The ids `user`, `hub`, `bifrost` and `localhost` are reserved.
