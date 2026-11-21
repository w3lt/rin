#!/usr/bin/env bash
# Rin's checks
#
# scripts/verify.sh
# scripts/verify.sh fast
# scripts/verify.sh matrix
# scripts/verify.sh docker   # needs Docker and HEAD pushed; see docker_smoke
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

# Docker smoke test: a throwaway team (the example plus an antigravity agent, so
# every image target builds) in a temporary RIN_HOME, `rin up`, every service
# running and the hub answering, then `down -v`. The images install rin from
# GitHub at HEAD, so HEAD must be pushed and uncommitted changes are not in them.
# The rin-verify-docker-* images stay behind as build cache.
team=verify-docker

compose() {
    docker compose -p "rin-$team" -f "$RIN_HOME/teams/$team/docker-compose.yml" "$@"
}

# On failure, the containers' logs go out before `down -v` deletes them.
cleanup() {
    local status=$?
    if [ -f "$RIN_HOME/teams/$team/docker-compose.yml" ]; then
        [ "$status" -ne 0 ] && compose logs --no-color --tail 30 2>&1
        compose down -v --remove-orphans >/dev/null 2>&1
    fi
    rm -rf "$tmp" "$log"
}

# Prints the state of every service; fails unless all run and the hub answers.
team_running() {
    local states status
    states="$(compose ps --all --format '{{.Service}} {{.State}}')" || return 1
    printf '%s\n' "$states"
    [ -n "$states" ] && ! grep -qv ' running$' <<<"$states" || return 1
    status="$(target/debug/rin --team "$team" status 2>&1)" || return 1
    printf '%s\n' "$status"
    ! grep -q '^hub: ' <<<"$status"
}

# Up within two minutes, and still up ten seconds later (no crash after start).
wait_running() {
    for _ in $(seq 60); do
        if team_running; then
            sleep 10
            team_running
            return
        fi
        sleep 2
    done
    return 1
}

docker_smoke() {
    local rev
    rev="$(git rev-parse HEAD)"
    step docker docker info
    if [ -z "$(git branch -r --contains "$rev")" ]; then
        echo "FAIL push: no remote branch has HEAD ($rev); the images build rin from GitHub at it, so push first"
        exit 1
    fi
    if [ -n "$(git status --porcelain)" ]; then
        echo "note  uncommitted changes are in the host rin only, not in the images"
    fi
    step build cargo build -q --locked

    tmp="$(mktemp -d)"
    export RIN_HOME="$tmp/home"
    trap cleanup EXIT
    mkdir -p "$RIN_HOME"
    echo 'ANTHROPIC_API_KEY=smoke-test' >"$RIN_HOME/keys.env"
    cp -R examples/team/agents "$tmp/agents"
    mkdir "$tmp/agents/gravity"
    echo 'Smoke test agent.' >"$tmp/agents/gravity/INSTRUCTIONS.md"
    {
        sed "s/^name: .*/name: $team/" examples/team/rin.yml
        cat <<'EOF'

  - id: gravity
    type: member
    label: Smoke Test
    harness: antigravity
    authentication: subscription
    model: gemini-3-pro
    resources: { cpus: 1, ram: 2g }
EOF
    } >"$tmp/rin.yml"

    step generate env RIN_GIT_REV="$rev" target/debug/rin generate -f "$tmp/rin.yml" --instructions "$tmp/agents"
    step up target/debug/rin --team "$team" up
    step running wait_running
}

case "$mode" in
fast | ci | matrix) ;;
docker)
    docker_smoke
    exit 0
    ;;
*)
    echo "usage: scripts/verify.sh [fast|ci|matrix|docker]" >&2
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
