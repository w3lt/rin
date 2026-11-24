---
title: Files and environment
description: What Rin keeps in ~/.rin, the Docker names it uses, and the environment variables it reads.
---

## `$RIN_HOME`

Rin keeps everything it generates in `$RIN_HOME`, by default `~/.rin`:

```
~/.rin/
├── current              name of the current team
├── keys.env             your provider keys (you write this; only Bifrost reads it)
└── teams/
    └── <name>/
        ├── source              path of the rin.yml it was generated from
        ├── rin.json            the config, defaults filled in (read by the hub)
        ├── docker-compose.yml  the Compose project (no secrets)
        ├── images/Dockerfile   the images' build
        ├── instructions/<id>/INSTRUCTIONS.md   copies of your instructions
        ├── tokens.json         hub tokens: one per agent, plus yours
        ├── env/<id>.env        each agent's token, and its Bifrost virtual key
        └── bifrost/            Bifrost's config and keys (API-key teams only)
            ├── config.json
            └── bifrost.env
```

`rin generate` writes the team directory; don't edit it by hand, change `rin.yml` and
generate again. Because the instructions are copied, the team keeps working if the project
directory moves.

`tokens.json`, `env/*.env`, `bifrost/bifrost.env` and `keys.env` are readable by you only.
When Rin creates `$RIN_HOME`, it makes it private too; an existing one keeps its mode, but
`teams/` is always private.

## Docker names

For a team `<name>`:

| Kind | Name |
| :--- | :--- |
| Compose project | `rin-<name>` |
| Containers | `rin-<name>-hub-1`, `rin-<name>-<id>-1`, `rin-<name>-bifrost-1` |
| Images | `rin-<name>-hub:<version>`, `rin-<name>-<id>:<version>` |
| Volumes | `rin-<name>-hub` (message log), `rin-<name>-<id>-home` (agent home), `rin-<name>-bifrost` |
| Published port | `127.0.0.1:10302` (the hub) |

To run Compose yourself on a team:

```sh
docker compose -p rin-<name> -f ~/.rin/teams/<name>/docker-compose.yml <command>
```

## Environment

### On your machine

| Variable | Read by | Effect |
| :------- | :------ | :----- |
| `RIN_HOME` | every command | Where Rin keeps its files. Default `~/.rin`. Each `RIN_HOME` has its own teams, current team and `keys.env`. |
| `RIN_GIT_REV` | `rin generate` | The Rin git revision the images build `rin` from. Default `v<version>`, the release tag. |

`RIN_GIT_REV` is for trying unreleased Rin changes in the images, for example
`RIN_GIT_REV=refs/heads/my-branch rin generate --instructions agents`. The revision must be
pushed to [github.com/w3lt/rin](https://github.com/w3lt/rin). Docker caches that build step by
its command line, so after new commits on the same branch rebuild without cache:

```sh
docker compose -p rin-<name> -f ~/.rin/teams/<name>/docker-compose.yml build --no-cache
```

or use a commit hash as the revision.

### In `keys.env`

| Variable | Provider |
| :------- | :------- |
| `ANTHROPIC_API_KEY` | Anthropic |
| `OPENAI_API_KEY` | OpenAI |
| `GEMINI_API_KEY` | Google Gemini |

One `NAME=value` per line; lines starting with `#` are comments.
