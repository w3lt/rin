# syntax=docker/dockerfile:1

# ================= BUILD IMAGE =================
FROM rust:1.100.0-slim-trixie AS build

ARG RIN_GIT_REV
RUN test -n "$RIN_GIT_REV" || (echo "RIN_GIT_REV is required" >&2; exit 1)

RUN cargo install --locked --root /out \
    --git https://github.com/w3lt/rin --rev "$RIN_GIT_REV" \
    --no-default-features --features hub,agent --bin rin


# ================= PRE-BASE IMAGE =================
FROM debian:trixie-20261123-slim AS prebase
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    bash-completion ca-certificates curl git jq less openssh-client procps \
    python3 ripgrep unzip vim-tiny xz-utils \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --uid 1000 --shell /bin/bash agent \
    && install -d -o agent -g agent /home/agent/work /home/agent/.cache \
    /home/agent/.claude /home/agent/.codex /home/agent/.pi /home/agent/.pi/agent \
    /home/agent/.gemini /home/agent/.gemini/config /home/agent/.gemini/antigravity-cli
COPY --from=build /out/bin/rin /usr/local/bin/rin


# ================= HUB IMAGE =================
FROM prebase AS hub
RUN install -d /var/lib/rin /etc/rin
USER agent
CMD ["rin", "hub"]


# ================= BASE IMAGE =================
FROM node:26.13.1-trixie-slim AS nodejs

FROM prebase AS base
COPY --from=nodejs /usr/local/bin/node /usr/local/bin/node
COPY --from=nodejs /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -s ../lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx


# ================= CLAUDE CODE IMAGE =================
FROM base AS claude-code

ARG CLAUDE_CODE_VERSION=2.1.326

RUN npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" \
    && npm cache clean --force

ENV RIN_HARNESS=claude-code \
    CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1

USER agent
WORKDIR /home/agent/work
CMD ["rin", "runtime"]


# ================= CODEX IMAGE =================
FROM base AS codex

ARG CODEX_VERSION=0.172.0

RUN npm install -g "@openai/codex@${CODEX_VERSION}" \
    && npm cache clean --force

ENV RIN_HARNESS=codex

USER agent
WORKDIR /home/agent/work
CMD ["rin", "runtime"]


# ================= PI IMAGE =================
FROM base AS pi
ARG PI_VERSION=1.7.0
RUN npm install -g --ignore-scripts "@earendil-works/pi-coding-agent@${PI_VERSION}" \
    && npm cache clean --force

ENV RIN_HARNESS=pi \
    PI_OFFLINE=1

USER agent
WORKDIR /home/agent/work
CMD ["rin", "runtime"]


# ================= ANTIGRAVITY IMAGE =================
FROM prebase AS antigravity

ARG AGY_VERSION=1.5.19-4821414830800896
ARG AGY_SHA512_AMD64=8160da6ceef2b7b62bb4a630b54b3fb4ee5283377509230736880c4ba4815e848a25cf346954a98134b647eb2f5d34e0364e77fbf74367ffd7dc29c525ddd82d
ARG AGY_SHA512_ARM64=eeecedfc2a63af281e245d73bb69f949a17c6d3ebcd3d023220056ddcc05e204a00d1d0ea77f68741b2fc15a1f66f740bc0e3bf800d734721c60938e61653010
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

ENV RIN_HARNESS=antigravity \
    AGY_CLI_DISABLE_AUTO_UPDATE=true

USER agent
WORKDIR /home/agent/work
CMD ["rin", "runtime"]
