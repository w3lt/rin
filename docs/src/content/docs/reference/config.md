---
title: rin.yml
description: Every field of the team config, its type, default and limits.
---

`rin.yml` describes one team. Rin checks it against
[`rin-schema.json`](https://github.com/w3lt/rin/blob/main/rin-schema.json), which is built
into the `rin` binary, so every `rin` of a version checks a config the same way. Unknown
fields are errors.

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/w3lt/rin/refs/tags/v0.1.2/rin-schema.json
rin: tohsaka
name: my-team
version: 0.1.2

messaging:
  timeout: 30m
  maxHops: 5

agents:
  - id: rin
    type: leader
    label: Chief Architect
    harness: claude-code
    authentication: subscription
    model: claude-opus-5-5
    maxSessions: 3
    resources:
      cpus: 1
      ram: 2g
```

## Top level

| Field | Type | Required | Default |
| :---- | :--- | :------- | :------ |
| [`rin`](#rin) | `tohsaka` | yes | |
| [`name`](#name) | string | yes | |
| [`version`](#version) | string | yes | |
| [`messaging`](#messaging) | object | no | see below |
| [`agents`](#agents) | list | yes | |

### `rin`

Always `tohsaka`. It marks the file as a Rin config.

### `name`

The team's name. Lowercase letters, digits and `-`, starting with a letter, 1 to 31
characters (`^[a-z][a-z0-9-]{0,30}$`).

It names the team everywhere: the directory `~/.rin/teams/<name>`, `--team <name>`,
`rin use <name>`, and the Docker project `rin-<name>`. Two configs with the same `name`
are the same team: generating one replaces the other.

### `version`

The exact Rin version this config is for, as `MAJOR.MINOR.PATCH`. It must equal
`rin --version`, or `rin generate` refuses to run. The images build `rin` from this
version's tag, so everyone using the config gets the same team.

## `messaging`

| Field | Type | Default | Limits |
| :---- | :--- | :------ | :----- |
| `timeout` | string | `30m` | Whole minutes, as `<n>m`; from `1m` to a year (`525600m`). |
| `maxHops` | integer | `5` | At least 1. |

**`timeout`** is how long one message may take from the moment it is sent, waiting in a
queue included. **`maxHops`** is the longest chain of messages allowed: your message is
hop 1, a message sent while handling it hop 2. See
[Talking to agents](/guides/messaging/).

## `agents`

A list of at least one agent, with **exactly one** `type: leader`. Ids must be unique.

| Field | Type | Required | Default |
| :---- | :--- | :------- | :------ |
| [`id`](#id) | string | yes | |
| [`type`](#type) | `leader` \| `member` | yes | |
| [`label`](#label) | string | yes | |
| [`harness`](#harness) | `claude-code` \| `codex` \| `antigravity` \| `pi` | yes | |
| [`authentication`](#authentication) | `subscription` \| `apiKey` | yes | |
| [`model`](#model) | string | yes | |
| [`maxSessions`](#maxsessions) | integer | no | `3` |
| [`resources`](#resources) | object | yes | |

### `id`

The agent's name: what you pass to `rin send`, `rin shell` and `rin log`, and the name of its
`agents/<id>/` directory and its container's hostname. Same rules as `name`
(`^[a-z][a-z0-9-]{0,30}$`). `user`, `hub`, `bifrost` and `localhost` are reserved.

### `type`

`leader` or `member`. The team has exactly one leader. The type is shown in `rin status` and
`list_agents`; it gives no extra powers. What makes the leader lead is its instructions.

### `label`

A title for the agent, such as `Backend Engineer`. Shown in `rin status`, and in the first
line of every message the agent sends, so the receiver knows who is asking.

### `harness`

The coding-agent tool the agent runs. See [Harnesses](/reference/harnesses/).

### `authentication`

- `subscription`: the agent uses its tool's own account; sign it in once with `rin shell`.
- `apiKey`: the agent uses a provider key from `~/.rin/keys.env`, through Bifrost.

`harness: pi` requires `apiKey`. See [Subscriptions and API keys](/guides/authentication/).

### `model`

The model the harness uses, passed to it as written: use the name that harness expects.
With `harness: pi` and `apiKey`, prefix the provider: `anthropic/<model>`,
`openai/<model>` or `google/<model>`.

### `maxSessions`

How many messages the agent works on at once, at least 1. Further messages wait in a queue,
first come first served.

### `resources`

The container's limits. Both fields are required.

| Field | Type | Example | Rule |
| :---- | :--- | :------ | :--- |
| `cpus` | number | `1`, `0.5`, `2` | Greater than 0. CPU cores (Compose `cpus`). |
| `ram` | string | `512m`, `4g` | A whole number from 1, then `m` or `g`, lowercase (Compose `mem_limit`). |

## Editor support

Add the schema line as the first line of the file, with your version:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/w3lt/rin/refs/tags/v0.1.2/rin-schema.json
```

Editors with a YAML language server then complete fields and flag mistakes as you type.
`rin init` writes this line for you.
