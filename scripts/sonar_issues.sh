#!/usr/bin/env bash
# Open SonarCloud issues and security hotspots on a pull request, read from the
# project's public API (no token needed)
#
# scripts/sonar_issues.sh        the current branch's pull request
# scripts/sonar_issues.sh 12     pull request #12
#
# Waits until SonarCloud has analysed the pull request's head commit, then prints
# one tab-separated line per finding: severity, rule, file:line, message.
# Exit 0: nothing open, 1: findings to triage, 2: error or timeout.
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
hotspots="$(get "hotspots/search?projectKey=$project&pullRequest=$pr&status=TO_REVIEW&ps=500")" ||
    die "cannot read security hotspots of #$pr"

n_issues="$(jq -r .total <<<"$issues")"
n_hotspots="$(jq -r .paging.total <<<"$hotspots")"

echo "#$pr at ${head:0:7}: $n_issues open issue(s), $n_hotspots hotspot(s) to review"
echo "https://sonarcloud.io/project/issues?id=$project&pullRequest=$pr&resolved=false"

jq -r --arg p "$project:" '.issues[]
    | [.severity, .rule, "\(.component | ltrimstr($p)):\(.line // 0)", .message] | @tsv' <<<"$issues"
jq -r --arg p "$project:" '.hotspots[]
    | ["HOTSPOT-\(.vulnerabilityProbability)", .ruleKey, "\(.component | ltrimstr($p)):\(.line // 0)", .message] | @tsv' <<<"$hotspots"

if [[ "$n_issues" -gt 500 || "$n_hotspots" -gt 500 ]]; then
    echo "only the first 500 of each are listed; see the link above" >&2
fi

if [[ "$n_issues" -eq 0 && "$n_hotspots" -eq 0 ]]; then
    exit 0
fi
exit 1
