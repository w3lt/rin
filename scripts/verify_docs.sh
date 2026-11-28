#!/usr/bin/env bash
#
# Verify the docs.
# scripts/verify_docs.sh
#
# HAPPY HACKING!

set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

lines=${VERIFY_LINES:-120}
log="$(mktemp)"
trap 'rm -f "$log"' EXIT

source scripts/lib/step.sh

pnpm_pin="$(sed -n 's/.*"packageManager": *"pnpm@\([^"]*\)".*/\1/p' docs/package.json)"
pnpm_is_pinned() {
    local v
    v="$(pnpm --version)" || return 1
    [[ "$v" = "$pnpm_pin" ]] || {
        echo "warn: pnpm $v, docs/package.json pins $pnpm_pin"
    }
}

step pnpm pnpm_is_pinned
step install pnpm --dir docs install --frozen-lockfile
step build pnpm --dir docs build
step links scripts/lib/check_docs_links.mjs docs/dist
