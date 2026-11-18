# syntax=docker/dockerfile:1
# Rin images, one target per container kind: hub, claude-code, codex, pi, antigravity.
# Written out by `rin generate`; harness versions are pinned per Rin release.

# --- rin, built for Linux from the team's Rin version --------------------------
FROM rust:1.98-slim-trixie AS build
ARG RIN_GIT_REV
RUN test -n "$RIN_GIT_REV" || (echo "RIN_GIT_REV is required" >&2; exit 1)
# One `rin` for every container (features hub + agent): it is compiled once, and
# `base` below carries it into every image. No package name: the repository
# holds one package, whatever it is called on crates.io; `--bin rin` is the
# executable.
RUN cargo install --locked --root /out \
      --git https://github.com/w3lt/rin --rev "$RIN_GIT_REV" \
      --no-default-features --features hub,agent --bin rin

# --- shared base: Debian, common tools, rin, user agent (UID 1000) --------------
FROM debian:trixie-20261104-slim AS base
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      bash-completion ca-certificates curl git jq less openssh-client procps \
      python3 ripgrep unzip vim-tiny xz-utils \
 && rm -rf /var/lib/apt/lists/*
# Every harness's config folder exists in every home volume, owned by agent, so the
# read-only instruction mount never makes Docker create one as root (even after
# an agent switches harness).
RUN useradd --create-home --uid 1000 --shell /bin/bash agent \
 && install -d -o agent -g agent /home/agent/work /home/agent/.cache \
      /home/agent/.claude /home/agent/.codex /home/agent/.pi /home/agent/.pi/agent \
      /home/agent/.gemini /home/agent/.gemini/config /home/agent/.gemini/antigravity-cli
COPY --from=build /out/bin/rin /usr/local/bin/rin

# --- hub -------------------------------------------------------------------------
FROM base AS hub
RUN install -d /var/lib/rin /etc/rin
CMD ["rin", "hub"]

# --- Node.js for the npm-installed harnesses -------------------------------------
# Copied from the official image at a pinned version: no install script and no
# extra apt repository, and `base` stays free of Node for the hub and Antigravity.
# (That image can't be the base itself: its user `node` already has UID 1000.)
FROM node:24.19.0-trixie-slim AS nodejs

FROM base AS node
COPY --from=nodejs /usr/local/bin/node /usr/local/bin/node
COPY --from=nodejs /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
 && ln -s ../lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx

# --- harnesses: each runs `rin runtime` as user agent ------------------------------
FROM node AS claude-code
ARG CLAUDE_CODE_VERSION=2.1.326
RUN npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" && npm cache clean --force
ENV RIN_HARNESS=claude-code DISABLE_AUTOUPDATER=1
USER agent
WORKDIR /home/agent/work
CMD ["rin", "runtime"]

FROM node AS codex
ARG CODEX_VERSION=0.169.0
RUN npm install -g "@openai/codex@${CODEX_VERSION}" && npm cache clean --force
ENV RIN_HARNESS=codex
USER agent
WORKDIR /home/agent/work
CMD ["rin", "runtime"]

FROM node AS pi
ARG PI_VERSION=0.73.1
RUN npm install -g "@mariozechner/pi-coding-agent@${PI_VERSION}" && npm cache clean --force
ENV RIN_HARNESS=pi
USER agent
WORKDIR /home/agent/work
CMD ["rin", "runtime"]

FROM base AS antigravity
ARG AGY_VERSION=1.5.10-5253791805734912
ARG AGY_SHA512_AMD64=713ae840845d74736e2d4a96f5d84e3353d614e73a875793ddf500997ccee09595448cebfd8e94fdd90ce2a0fa97707bd1cc29763dcfe01c945d80c92234378f
ARG AGY_SHA512_ARM64=f9f5c7ebc47a8b8de536acca8116b1f5a43dca614cde94008fe5a510517b23535fb281d85416930d76e76478858bf3dc66ea6d1b0c9b6eeadcfabe4d8ef2a807
ARG TARGETARCH
RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) dir=linux-x64; file=cli_linux_x64.tar.gz; sum="$AGY_SHA512_AMD64" ;; \
      arm64) dir=linux-arm; file=cli_linux_arm64.tar.gz; sum="$AGY_SHA512_ARM64" ;; \
      *) echo "unsupported architecture: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/agy.tar.gz \
      "https://storage.googleapis.com/antigravity-public/antigravity-cli/${AGY_VERSION}/${dir}/${file}"; \
    echo "$sum  /tmp/agy.tar.gz" | sha512sum -c -; \
    tar -xzf /tmp/agy.tar.gz -C /tmp antigravity; \
    install -m 0755 /tmp/antigravity /usr/local/bin/agy; \
    rm -f /tmp/agy.tar.gz /tmp/antigravity
ENV RIN_HARNESS=antigravity AGY_CLI_DISABLE_AUTO_UPDATE=true
USER agent
WORKDIR /home/agent/work
CMD ["rin", "runtime"]
