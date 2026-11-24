---
title: Harnesses
description: The coding-agent tools Rin can run, how each is started, where its instructions go and how it reaches teammates.
---

A harness is the coding-agent program an agent runs. Rin installs it in the agent's image,
at a version pinned by the Rin release, and starts it once per message, headless, with its
permission prompts turned off.

| `harness` | Tool | Command in the container | Instructions mounted at |
| :-------- | :--- | :----------------------- | :---------------------- |
| `claude-code` | [Claude Code](https://github.com/anthropics/claude-code) | `claude` | `~/.claude/CLAUDE.md` |
| `codex` | [Codex CLI](https://github.com/openai/codex) | `codex` | `~/.codex/AGENTS.md` |
| `antigravity` | Antigravity CLI | `agy` | `~/.gemini/GEMINI.md` |
| `pi` | [pi](https://github.com/badlogic/pi-mono) | `pi` | `~/.pi/agent/AGENTS.md` |

`~` is `/home/agent`, the agent's home volume. The instructions file is your
`agents/<id>/INSTRUCTIONS.md`, mounted read-only.

## Authentication and models

| `harness` | `subscription` | `apiKey`: key used | `model` |
| :-------- | :------------- | :----------------- | :------ |
| `claude-code` | yes: sign in with `claude` | `ANTHROPIC_API_KEY` | A Claude model, e.g. `claude-opus-5-5` |
| `codex` | yes: sign in with `codex` | `OPENAI_API_KEY` | An OpenAI model, e.g. `gpt-6-luna` |
| `antigravity` | yes: sign in with `agy` | `GEMINI_API_KEY` | A Gemini model, e.g. `gemini-3.8-flash` |
| `pi` | no | by prefix | `anthropic/…`, `openai/…`, `google/…`, `ollama/…` or `<custom provider>/…` |

## Messaging teammates

| `harness` | How it messages teammates |
| :-------- | :------------------------ |
| `claude-code`, `codex`, `antigravity` | Rin's MCP server, `rin`, with two tools: `send_message` and `list_agents` |
| `pi` | The `rin send <id> "<text>"` and `rin agents` commands, from its shell |

Mention the right one in the agent's `INSTRUCTIONS.md`.

The MCP tools take only what they need (`send_message` takes `to` and `text`): the sender
and the hop count are filled in by Rin and cannot be changed by the agent. Their timeout is
set a little above `messaging.timeout`, so a slow reply ends with Rin's own timeout error.

## Settings Rin manages

Each time the agent's container starts, Rin writes three settings into the harness's own
config in the home volume, and touches nothing else:

- the `rin` MCP server (not for pi),
- the MCP tool timeout,
- the default model from `rin.yml`,

and, for API-key agents, the route to Bifrost. Your other settings stay as you left them.
For Codex, `~/.codex/config.toml` is written out again as a whole when one of these changes:
other keys keep their values, but not their comments or layout.
