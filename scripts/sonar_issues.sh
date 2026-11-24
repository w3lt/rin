#!/usr/bin/env bash
# Open SonarCloud issues on a pull request's head commit, from the public API
#
# scripts/sonar_issues.sh        the current branch's pull request
# scripts/sonar_issues.sh 12     pull request #12
#
# Exit 0: nothing open, 1: issues to triage, 2: error or timeout.
#
# HAPPY HACKING!

set -euo pipefail

cd "$(dirname "$0")/.." || exit 2

api=https://sonarcloud.io/api
project=${SONAR_PROJECT:-w3lt_rin}
timeout=${SONAR_TIMEOUT:-600}
interval=15

die() {
    local message="$1"
    echo "sonar_issues: $message" >&2
    exit 2
}

get() {
    local path="$1"
    curl -fsS --retry 2 "$api/$path"
}

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
waited=0
while :; do
    head="$(head_commit)" || die "cannot read pull request #$pr"
    analysed="$(analysed_commit)" || die "cannot reach the SonarCloud API"
    [[ "$analysed" = "$head" ]] && break
    if [[ "$waited" -ge "$timeout" ]]; then
        die "SonarCloud has not analysed ${head:0:7} of #$pr after ${timeout}s; check the PR's SonarCloud check"
    fi
    sleep "$interval"
    waited=$((waited + interval))
done

issues="$(get "issues/search?componentKeys=$project&pullRequest=$pr&resolved=false&ps=500")" ||
    die "cannot read issues of #$pr"

n_issues="$(jq -r .total <<<"$issues")"

echo "#$pr at ${head:0:7}: $n_issues open issue(s)"
echo "https://sonarcloud.io/project/issues?id=$project&pullRequest=$pr&resolved=false"

jq -r --arg p "$project:" '.issues[]
    | [.severity, .rule, "\(.component | ltrimstr($p)):\(.line // 0)", .message] | @tsv' <<<"$issues"

if [[ "$n_issues" -gt 500 ]]; then
    echo "only the first 500 are listed; see the link above" >&2
fi

if [[ "$n_issues" -eq 0 ]]; then
    exit 0
fi
exit 1
