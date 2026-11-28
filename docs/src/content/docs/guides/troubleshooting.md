---
title: Troubleshooting
description: What Rin's error messages mean and how to fix them.
---

Start with `rin status`: it shows each container's state, the hub's view of sessions and
queues, deadlock warnings and, for teams with API-key agents, whether Bifrost is up.

To see what a container itself printed, use Compose directly:

```sh
docker compose -p rin-<name> -f ~/.rin/teams/<name>/docker-compose.yml logs <id>
```

(`hub` and `bifrost` are services too.)

## Generating

**`` no current team: run `rin generate` first, or pass --team <name> ``**\
No team has been generated yet on this machine (or in this `RIN_HOME`).

**`` unknown team "x": no …/rin.json (run `rin generate`) ``**\
`--team` or `rin use` names a team that hasn't been generated. Teams are named by the
`name` in their `rin.yml`, not by a directory.

**`` version 0.1.1 does not match this rin (0.1.2); install rin 0.1.1 or change `version:` ``**\
The file was written for another Rin version. Install that version, or
[upgrade the team](/guides/lifecycle/#upgrade-rin).

**`invalid config:` followed by `/agents/1/…: …` lines**\
`rin.yml` breaks the schema. Each line gives the path of the bad value. Common causes: a
missing required field, a second `type: leader`, `harness: pi` with
`authentication: subscription`, or `ram: 2G` (lowercase `m`/`g` only). See the
[rin.yml reference](/reference/config/).

**`instructions don't match rin.yml:`**\
Every agent needs `agents/<id>/INSTRUCTIONS.md`, and every directory in `agents/` must be
an agent (directories starting with `.` are ignored). The lines below it say which is off.

**`would use the Docker container name "rin-a-b-c", which team "a-b" already uses`**\
Two teams' names would clash in Docker. Rename the team or the agent.

## Starting

**`cannot run docker (is Docker installed and running?)`**\
Start Docker Desktop or the Docker daemon. Check `docker compose version` is 5.6 or later.

**`port 10302 on 127.0.0.1 is already in use (team a is up)`**\
Only one team can be up at a time. `rin down --team a`, then `rin up`. Without a team
named, another program holds the port.

**The first `rin up` fails while building**\
The images build `rin` from GitHub at your version's tag, and download the harnesses, so
the build needs network access: Docker Hub, GitHub and crates.io (for `rin`), Debian's
package mirrors, npm, and Google's storage (for Antigravity).

## Messaging

**`receiver down: cannot reach <id>`**\
The agent's container isn't running. `rin status` shows its state; `rin up` starts it.

**`cannot reach the hub at http://127.0.0.1:10302`**\
The team isn't up, or another team is. `rin status` shows which.

**`` session failed: <id> refused the hub (not authorized: wrong token); `rin up` restarts it with the current tokens.json ``**\
Tokens were regenerated but the agent still runs with the old one. Run `rin up`.

**`session failed: claude exited with …`** (or `codex`, `agy`, `pi`)\
The harness itself failed. The last lines of its output follow. Usually the agent isn't
signed in (`rin shell <id>`, run its tool), its model name is wrong, or its API key is
missing.

**`timeout: …`**\
The message took longer than `messaging.timeout`. See
[Timeout](/guides/messaging/#timeout) for the three variants. If agents are waiting on
each other, `rin status` shows a deadlock warning.

**`hop limit reached: this message would be hop 6, maxHops is 5`**\
A chain of messages got too long. Raise `messaging.maxHops`, or tell agents to answer
rather than delegate further.

**`unknown agent: "x"`**\
No agent has that id. `rin agents` lists them.

**`message too long: … as JSON, the hub takes at most 120 KiB`**\
A message is passed to the harness as one command-line argument, which Linux limits. Put
long content in a file in the agent's home and send the path.

## API keys

**`cannot start team …: …/keys.env does not exist`** or **`… does not set …`** (from `rin up`)\
An API-key agent needs a key that `keys.env` doesn't have; the message names the variables.
See [API key](/guides/authentication/#api-key). Add them, then `rin generate` and `rin up`.

**`pi model "…" must be <provider>/<model>`** (from `rin generate`)\
Write pi's model as `<provider>/<model>`, with a built-in provider (`anthropic`, `openai`,
`google`, `ollama`) or the `id` of one of your [`llmProviders`](/reference/config/#llmproviders).

**`only pi agents can use provider "…"`** (from `rin generate`)\
Ollama and custom providers are for pi agents; other harnesses use their own provider.
