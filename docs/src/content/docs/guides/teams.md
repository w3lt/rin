---
title: Running several teams
description: Keep several teams, switch the current one, and act on another with --team.
---

You can keep as many teams as you like; each is a directory in `~/.rin/teams/`, named after
its `rin.yml`'s `name`.

## The current team

`rin generate` makes the team it writes the **current team**. Every command except `init`,
`generate`, `use` and `completions` acts on the current team, from any directory: you don't need to be
in the project to run `rin up` or `rin send`.

Switch it with:

```sh
rin use other-team
```

```
current team: other-team
```

or act on another team for one command with `--team`:

```sh
rin status --team other-team
```

`--team` takes the team's `name`, not a path. The team must have been generated first.

## One team up at a time

Every team publishes its hub on `127.0.0.1:10302`, so only one team can be up at once.
`rin up` refuses to start a second one and names the team that is up:

```
cannot start team b: port 10302 on 127.0.0.1 is already in use (team a is up). Only one Rin team can be up at a time: run `rin down --team <name>` first
```

Stop the other team first:

```sh
rin down --team a
rin up
```

## Teams are independent

Each team has its own containers, images, home volumes, tokens and log. Two teams can have
agents with the same id: their Docker names differ (`rin-a-backend`, `rin-b-backend`). They
share only `~/.rin/keys.env`.

`rin generate` refuses a team whose Docker names would collide with another team's (team
`a` with agent `b-c` and team `a-b` with agent `c` would both be `rin-a-b-c`). Rename the
team or the agent.
