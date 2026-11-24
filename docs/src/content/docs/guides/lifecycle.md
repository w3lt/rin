---
title: Changing and removing a team
description: Apply changes to rin.yml and instructions, add and remove agents, upgrade Rin, rotate tokens and delete a team.
---

## Apply a change

Whatever you change, in `rin.yml` or in an `INSTRUCTIONS.md`, apply it the same way:

```sh
rin generate --instructions agents     # add -f <file> if your config isn't ./rin.yml
rin up
```

`rin generate` rewrites the team's directory in `~/.rin/teams/<name>`, and `rin up`
recreates only the containers whose definition changed. It also runs the image builds, which
come straight from Docker's cache when nothing in them changed. You can run `rin up` while the team is up, but a change to `rin.yml` restarts the hub (and
any agent whose settings changed), dropping messages in flight: check `rin status` for
running sessions first. A change to an `INSTRUCTIONS.md` alone restarts nothing; the next
session reads the new file.

Generating again with nothing changed changes nothing.

## What is kept

| Kept across `rin down`, `rin up` and `rin generate` | Where                          |
| :-------------------------------------------------- | :----------------------------- |
| Each agent's home: sign-ins, `~/work`, tool history | Docker volume `rin-<name>-<id>-home` |
| The message log                                     | Docker volume `rin-<name>-hub`  |
| Bifrost's data                                      | Docker volume `rin-<name>-bifrost` |
| Tokens and Bifrost virtual keys                     | `~/.rin/teams/<name>/`          |

Rin writes three settings into each harness's own config on every start: Rin's MCP server,
its tool timeout and the default model. Everything else you set in a harness's config inside
the container stays as you left it; for those three, `rin.yml` wins.

## Add an agent

Add it to `rin.yml`, create `agents/<id>/INSTRUCTIONS.md`, then generate and `rin up`. Its
image is built and its container started. Sign it in if it uses a subscription.

## Remove an agent

Remove it from `rin.yml` **and** delete `agents/<id>` (`rin generate` refuses a directory in
`agents/` that is no agent). Generate and `rin up`: its container is stopped and removed,
and its token and files leave the team directory.

Its home volume is kept, so adding an agent with the same id later brings its home back.
To delete it for good:

```sh
docker volume rm rin-<name>-<id>-home
```

## Change an agent's harness

Change `harness` (and `model`), then generate and `rin up`. The agent gets a new image; its
home is kept. Sign in to the new tool if it uses a subscription.

## Upgrade Rin

1. Install the new `rin`.
2. Set `version:` in `rin.yml` to the new version (`rin --version` shows it). Updating the
   version in the schema line at the top of the file keeps your editor's checks current.
3. `rin generate --instructions agents` and `rin up`.

The images are rebuilt for the new version. Homes, sign-ins and the log are kept.

## Rotate tokens

Each agent, and you, have a token for the hub. To replace them all:

```sh
rm ~/.rin/teams/<name>/tokens.json
rin generate --instructions agents
rin up
```

Bifrost's virtual keys are not tokens and stay as they are.

## Delete a team

:::danger
This deletes every agent's home (including sign-ins and anything they kept in `~/work`) and
the message log. It cannot be undone.
:::

```sh
rin down --team <name>
docker compose -p rin-<name> -f ~/.rin/teams/<name>/docker-compose.yml down --volumes --rmi all
rm -rf ~/.rin/teams/<name>
```

`--rmi all` also removes the images the team used, the Bifrost image included. If the team was current, pick another with `rin use <other>`.
