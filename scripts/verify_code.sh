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

source scripts/lib/step.sh

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
