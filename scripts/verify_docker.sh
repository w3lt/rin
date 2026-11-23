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
export RIN_HOME
team=verify-docker

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

compose() {
    docker compose -p "rin-$team" -f "$RIN_HOME/teams/$team/docker-compose.yml" "$@"
}

cleanup() {
    local status=$?
    if [[ -f "$RIN_HOME/teams/$team/docker-compose.yml" ]]; then
        [[ "$status" -ne 0 ]] && compose logs --no-color --tail 30 2>&1
        compose down -v --remove-orphans >/dev/null 2>&1
    fi
    rm -rf "$tmp" "$log"
}

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
    local rev="$(git rev-parse HEAD)"
    step docker docker info
    if [[ -n "$(docker ps -a -q --filter "name=rin-$team")$(docker volume ls -q --filter "name=rin-$team-")" ]]; then
        echo "FAIL leaftovers: containers or volumes named rin-$team* exist; remove them first (docker compose -p rin-$team down -v)"
        exit 1
    fi

    if [[ -z "$(git branch -r --contains "$rev")" ]]; then
        echo "FAIL push: no remote branch has HEAD ($rev); the image build rin from Github at it, so push first"
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
