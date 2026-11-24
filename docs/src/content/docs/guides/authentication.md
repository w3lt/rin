---
title: Subscriptions and API keys
description: Sign agents in with their tool's subscription, or give them API keys through Bifrost without the keys entering their containers.
---

Each agent has `authentication: subscription` or `authentication: apiKey`. You can mix them
in one team.

## Subscription

The agent uses its tool's own account (a Claude, ChatGPT or Google plan). Sign it in once
after the first `rin up`:

```sh
rin shell <id>
```

then run the agent's tool (`claude`, `codex` or `agy`) and follow its sign-in. The sign-in
is kept in the agent's home volume, so it survives `rin down`, `rin up` and new versions of
Rin. Each agent signs in separately, even when two use the same tool.

pi has no subscription sign-in: pi agents always use `apiKey`.

## API key

Your provider keys live in one file on your machine, **`~/.rin/keys.env`**
(`$RIN_HOME/keys.env` if you set [`RIN_HOME`](/reference/files/#environment)):

```sh
mkdir -p -m 700 ~/.rin
touch ~/.rin/keys.env && chmod 600 ~/.rin/keys.env
```

```sh title="~/.rin/keys.env"
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
GEMINI_API_KEY=...
```

Set only the providers you use. Then generate and start the team:

```sh
rin generate --instructions agents
rin up
```

Run both again whenever you add or change a key.

### What the agents get

The keys never enter an agent's container. When a team has at least one API-key agent, Rin
adds **Bifrost**, an LLM gateway, as one more container:

- Bifrost is the only container that reads `keys.env`.
- Each API-key agent gets its own **virtual key** for Bifrost (`sk-bf-…`) instead of your
  provider key, and its harness is pointed at Bifrost instead of the provider.
- Subscription agents don't use Bifrost at all, and a team with no API-key agent doesn't
  run it.

`rin generate` only reads the variable *names* in `keys.env`, to know which providers to
set up; it never copies the values. It also makes the file readable by you only.

### Which key each agent needs

| `harness`     | Key in `keys.env`                     |
| :------------ | :------------------------------------ |
| `claude-code` | `ANTHROPIC_API_KEY`                   |
| `codex`       | `OPENAI_API_KEY`                      |
| `antigravity` | `GEMINI_API_KEY`                      |
| `pi`          | depends on the `model` prefix, below  |

pi's `model` must name its provider, so Rin can route it through Bifrost:

| `model`               | Key                 |
| :-------------------- | :------------------ |
| `anthropic/<model>`   | `ANTHROPIC_API_KEY` |
| `openai/<model>`      | `OPENAI_API_KEY`    |
| `google/<model>`      | `GEMINI_API_KEY`    |

A pi model without one of these prefixes gets no route through Bifrost, and the agent logs
a warning when it starts.

### Warnings you may see

- `keys.env does not exist` (from `rin generate`) or `keys.env is missing: API-key agents
  will fail` (from `rin up`): create the file, then run `rin generate` and `rin up`.
- `keys.env sets none of ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY`: the file
  exists but has no key Rin knows. Add one and generate again.

`rin status` shows whether Bifrost is up on its last line (`bifrost: running`) for teams
that use it.

## Switching an agent

Change `authentication` in `rin.yml`, then `rin generate` and `rin up`. An agent moving from
`apiKey` to `subscription` stops using Bifrost and needs to be signed in; one moving the
other way uses its virtual key from then on.
