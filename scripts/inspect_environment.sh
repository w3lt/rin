#!/usr/bin/env bash
#
# inspect_environment is used for detecting environment and toolchain for development machine
# including docker, rust toolchain, fmt, clippy, rust-analyzer git
#
# HAPPY HACKING!

set -u

cd "$(dirname "$0")/.." || exit 1

# Minimum requirements
rust_min=1.85
compose_min=5.6

failed=0

ok() { printf 'ok    %-9s %s\n' "$1" "$2"; }
warn() { printf 'warn  %-9s %s\n' "$1" "$2"; }
fail() {
    printf 'FAIL  %-9s %s\n' "$1" "$2"
    failed=$((failed + 1))
}

have() { command -v "$1" >/dev/null 2>&1; }

version_of() { sed 's/[^0-9]*\([0-9][0-9A-Za-z.+_-]*\).*/\1/;q'; }

ver_ge() {
    printf '%s\n%s\n' "$2" "$1" | awk -F. '
        NR == 1 { for (i = 1; i <= 3; i++) want[i] = $i + 0 }
        NR == 2 {
            for (i = 1; i <= 3; i++) {
                if ($i + 0 > want[i]) exit 0
                if ($i + 0 < want[i]) exit 1
            }
            exit 0
        }'
}

# Host
os="$(uname -s)"
ok host "$os $(uname -m)"
[[ "$os" = Linux ]] || warn linux "not Linux: cfg(target_os = \"linux\") code is skipped by local checks"

# Git
if have git; then
    ok git "$(git --version | version_of)"
    hooks="$(git config core.hooksPath 2>/dev/null || true)"
    if [[ "$hooks" = .githooks ]]; then
        ok hooks ".githooks"
    else
        warn hooks "off; enable with: git config core.hooksPath .githooks"
    fi
else
    fail git "not found"
fi

# Rust toolchain
if have rustup; then
    ok rustup "$(rustup show active-toolchain 2>/dev/null | sed 's/ .*//;q')"
else
    warn rustup "not found; the toolchain is not managed by rustup"
fi

if have rustc; then
    v="$(rustc --version | version_of)"
    if ver_ge "$v" "$rust_min"; then
        ok rustc "$v"
    else
        fail rustc "$v, need $rust_min or later (edition 2024); try: rustup update"
    fi
else
    fail rustc "not found; install from https://rust-lang.org"
fi

if have cargo; then
    ok cargo "$(cargo --version | version_of)"
    if v="$(cargo fmt --version 2>/dev/null)"; then
        ok fmt "$(printf '%s\n' "$v" | version_of)"
    else
        fail fmt "not found; try: rustup component add rustfmt"
    fi
    if v="$(cargo clippy --version 2>/dev/null)"; then
        ok clippy "$(printf '%s\n' "$v" | version_of)"
    else
        fail clippy "not found; try: rustup component add clippy"
    fi
    if v="$(rust-analyzer --version 2>/dev/null)"; then
        ok analyzer "$(printf '%s\n' "$v" | version_of)"
    else
        warn analyzer "not found; try: rustup component add rust-analyzer"
    fi
else
    fail cargo "not found; install from https://rustup.rs"
fi

if have docker; then
    ok docker "$(docker --version | version_of)"
    if docker info >/dev/null 2>&1; then
        ok daemon "reachable"
    else
        warn daemon "not reachable; start Docker"
    fi
    if v="$(docker compose version 2>/dev/null)"; then
        v="$(printf '%s\n' "$v" | version_of)"
        if ver_ge "$v" "$compose_min"; then
            ok compose "$v"
        else
            warn compose "$v, need $compose_min or later"
        fi
    else
        warn compose "plugin not found; need Compose $compose_min or later"
    fi
else
    warn docker "not found; needed for scripts/verify.sh docker and rin up"
fi

if [ "$failed" -ne 0 ]; then
    echo "$failed required tool(s) missing or too old"
    exit 1
fi
