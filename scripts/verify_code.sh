#!/usr/bin/env bash
# Rin's checks
#
# scripts/verify_code.sh
# scripts/verify_code.sh fast
# scripts/verify_code.sh matrix
#
# HAPPY HACKING!

set -euo pipefail
export CARGO_TERM_COLOR=never

cd "$(dirname "$0")/.." || exit 1

mode=${1:-ci}
lines=${VERIFY_LINES:-120}
log="$(mktemp)"
trap 'rm -f "$log"' EXIT

step() {
    local name="$1"
    shift
    if "$@" >"$log" 2>&1; then
        echo "ok    $name"
    else
        printf 'FAIL %s:' "$name"
        printf ' %q' "$@"
        echo
        local half=$((lines / 2))
        head -n "$half" "$log"
        tail -n "$half" "$log"
        local total
        total="$(wc -l <"$log" | tr -d ' ')"
        if [ "$total" -gt "$lines" ]; then
            echo "... ($((total - lines)) more lines; rerun the command above for all of it)"
        fi
        exit 1
    fi
}

case "$mode" in
fast | ci | matrix) ;;
*)
    echo "usage: scripts/verify_code.sh [fast|ci|matrix]" >&2
    exit 2
    ;;
esac

step fmt cargo fmt --all -- --check --color never
step clippy cargo clippy -q --all-targets --all-features -- -D warnings
[ "$mode" = fast ] && exit 0

step test cargo test -q --all-targets --all-features
step image-build cargo check -q --locked --no-default-features --features hub,agent

if [ "$mode" = matrix ]; then
    for f in host hub agent hub,agent host,hub host,agent ""; do
        step "clippy [${f:-none}]" cargo clippy -q --all-targets --no-default-features --features "$f" -- -D warnings
    done
fi
