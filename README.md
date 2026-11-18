# Rin

Rin (Tohsaka) stands up a team of AI agents on your machine from one config
file: one Docker container per agent, each running Claude Code, Codex,
Antigravity or pi. One agent leads; you can talk to any of them, and they can
message each other. Design and success criteria: issue #1.

## Use

Needs Docker with Compose 2.24 or later (Docker Desktop, or Docker Engine with the Compose plugin).

```sh
cargo install --git https://github.com/w3lt/rin --tag v0.1.0 tohsaka   # installs the `rin` command

# API-key agents only: provider keys, read by Bifrost alone (rin generate keeps the file 0600)
mkdir -p -m 700 ~/.rin && touch ~/.rin/keys.env && chmod 600 ~/.rin/keys.env
cat >> ~/.rin/keys.env <<'EOF'
ANTHROPIC_API_KEY=...
EOF

cd examples/team                      # rin.yml + agents/<id>/INSTRUCTIONS.md
rin generate --instructions agents    # writes ~/.rin/teams/my-team, makes it current
rin up                                # builds images on first run
rin shell rin                         # log in once for subscription agents, run `claude`
rin send rin "Reply with exactly: pong"
rin status
rin log -f
rin down
```

Every command but `generate` acts on the current team from any directory;
`--team <name>` picks another, `rin use <name>` switches. Only one team can be
up at a time (each publishes the hub on `127.0.0.1:10302`).

Inside a container, agents message each other with the `send_message` and
`list_agents` MCP tools (Claude Code, Codex, Antigravity) or with
`rin send <id> "<text>"` and `rin agents` (pi).

## Develop

```sh
cargo test --all-features    # unit tests + hub/runtime end to end with a fake harness
cargo clippy --all-features --all-targets
```

The binary has three feature sets: `host` (default: the commands above),
`hub` (`rin hub`) and `agent` (`rin runtime`, `rin mcp`). Images build Linux
`rin` with `--no-default-features --features hub,agent`.

Images build `rin` from the `v<version>` tag. To try unreleased changes, point
them at another revision when generating, e.g.
`RIN_GIT_REV=refs/heads/my-branch rin generate --instructions agents`. Docker
caches that build step by its command line, so after pushing new commits to
the same branch rebuild with
`docker compose -p rin-<name> -f ~/.rin/teams/<name>/docker-compose.yml build --no-cache`
(or use a commit hash as the revision).
