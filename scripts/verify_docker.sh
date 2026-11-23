#!/usr/bin/env bash
#
# Verify the Docker-relevant stuff of Rin
#
# HAPPY HACKING!

set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

# Set RIN_HOME to a temp dir
tmp="$(mktemp -d "${TMPDIR:-/tmp}/rin-verify-docker.XXXXXX")"
RIN_HOME="$tmp/home"
mkdir -p "$RIN_HOME"
export RIN_HOME
team=verify-docker

lines=${VERIFY_LINES:-120}
log="$tmp/step.log"
trap 'rm -f "$log"' EXIT

source scripts/lib/step.sh

compose() {
    docker compose -p "rin-$team" -f "$RIN_HOME/teams/$team/docker-compose.yml" "$@"
}

cleanup() {
    local status=$?
    if [[ -f "$RIN_HOME/teams/$team/docker-compose.yml" ]]; then
        if [[ "$status" -ne 0 ]]; then
            compose logs --no-color --tail 30 2>&1 || true
        fi
        compose down -v --remove-orphans >/dev/null 2>&1 \
            || echo "WARN cleanup: compose down failed; run: docker compose -p rin-$team down -v" >&2
    fi
    rm -rf "$tmp"
    exit "$status"
}

trap cleanup EXIT

team_running() {
    local states status
    states="$(compose ps --all --format '{{.Service}} {{.State}}')" || return 1
    printf '%s\n' "$states"
    [[ -n "$states" ]] && ! grep -qv ' running$' <<<"$states" || return 1
    status="$(target/debug/rin --team "$team" status 2>&1)" || return 1
    printf '%s\n' "$status"
    ! grep -q '^hub: ' <<<"$status"
}

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
    if [[ -n "$(docker ps -a -q --filter "name=rin-$team")$(docker volume ls -q --filter "name=rin-$team-")" ]]; then
        echo "FAIL leftovers: containers or volumes named rin-$team* exist; remove them first (docker compose -p rin-$team down -v)"
        exit 1
    fi

    if [[ -z "$(git branch -r --contains "$rev")" ]]; then
        echo "FAIL push: no remote branch has HEAD ($rev); the images build rin from Github at it, so push first"
        exit 1
    fi

    if [[ -n "$(git status --porcelain)" ]]; then
        echo "note uncommitted changes are in the host rin only, not in the images"
    fi

    step build cargo build -q --locked
    echo 'ANTHROPIC_API_KEY=smoke-test' >"$RIN_HOME/keys.env"
    cp -R examples/team/agents "$tmp/agents"
    sed "s/^name: my-team$/name: $team/" examples/team/rin.yml >"$tmp/rin.yml"
    grep -q "^name: $team$" "$tmp/rin.yml" || { echo "FAIL rename: no 'name: my-team' in examples/team/rin.yml"; exit 1; }

    step generate env RIN_GIT_REV="$rev" target/debug/rin generate -f "$tmp/rin.yml" --instructions "$tmp/agents"
    step up target/debug/rin up
    step running wait_running
}

docker_smoke
exit 0
