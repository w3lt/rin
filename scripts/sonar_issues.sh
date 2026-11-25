#!/usr/bin/env bash
# Check the sonarscan issues on a pr
#
# scripts/sonar_issues.sh        the current branch's pull request
# scripts/sonar_issues.sh 12     pull request #12
#
# Exit 0: nothing open, 1: error or timeout, 255: issues to triage.
#
# HAPPY HACKING!

set -euo pipefail

cd "$(dirname "$0")/.." || exit 1

api=https://sonarcloud.io/api
project=${SONAR_PROJECT:-w3lt_rin}
timeout=${SONAR_TIMEOUT:-600}
interval=15

die() {
    local message="$1"
    echo "sonar_issues: $message" >&2
    exit 1
}

get() {
    local path="$1"
    curl -fsS \
        --connect-timeout 10 \
        --max-time 30 \
        --retry 2 \
        --retry-max-time 60 \
        "$api/$path"
}

[[ "$#" -le 1 ]] ||
    die "usage: scripts/sonar_issues.sh [pull request number]"

[[ "$timeout" =~ ^[1-9][0-9]*$ ]] ||
    die "SONAR_TIMEOUT must be a positive integer"

pr=${1:-}
if [[ -z "$pr" ]]; then
    pr="$(gh pr view --json number -q .number)" || die "no pull request for this branch; pass its number"
fi
[[ "$pr" =~ ^[0-9]+$ ]] || die "usage: scripts/sonar_issues.sh [pull request number]"

head_commit() {
    gh pr view "$pr" --json headRefOid -q .headRefOid
}

analysed_commit() {
    get "project_pull_requests/list?project=$project" |
        jq -r --arg pr "$pr" '.pullRequests[] | select(.key == $pr) | .commit.sha // empty'
}

# Re-read the head each round, so a push during the wait is followed
SECONDS=0
while :; do
    head="$(head_commit)" || die "cannot read pull request #$pr"
    analysed="$(analysed_commit)" || die "cannot reach the SonarCloud API"
    [[ "$analysed" = "$head" ]] && break
    if (( SECONDS >= timeout )); then
        die "SonarCloud has not analysed ${head:0:7} of #$pr after ${timeout}s; check the PR's SonarCloud check"
    fi
    sleep "$interval"
done

issues="$(get "issues/search?componentKeys=$project&pullRequest=$pr&resolved=false&ps=500")" ||
    die "cannot read issues of #$pr"

n_issues="$(jq -er '.total | numbers' <<<"$issues")" ||
    die "invalid issue count"

echo "#$pr at ${head:0:7}: $n_issues open issue(s)"
echo "https://sonarcloud.io/project/issues?id=$project&pullRequest=$pr&resolved=false"

jq -r --arg p "$project:" '.issues[]
    | [.severity, .rule, "\(.component | ltrimstr($p)):\(.line // 0)", .message]
    | @tsv' <<<"$issues" ||
    die "cannot parse issues"

if [[ "$n_issues" -gt 500 ]]; then
    echo "only the first 500 are listed; see the link above" >&2
fi

if [[ "$n_issues" -eq 0 ]]; then
    exit 0
fi
exit 255
