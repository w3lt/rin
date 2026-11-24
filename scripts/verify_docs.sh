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

source script/lib/step.sh

step install pnpm --dir docs install --frozen-lockfile
step build pnpm --dir docs build
step links scripts/lib/check_docs_links.mjs docs/dist
