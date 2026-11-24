#!/usr/bin/env bash
# Status checks of a pull request and the findings they annotate on its code
#
# scripts/pr_checks.sh        the current branch's pull request
# scripts/pr_checks.sh 12     pull request #12
#
# Exit 0: all checks passed with no findings, 1: something to triage, 2: error or timeout.
#
# HAPPY HACKING!

set -euo pipefail

cd "$(dirname "$0")/.." || exit 2

timeout=${PR_CHECKS_TIMEOUT:-600}
# Some checks start well after a push, so trust the set only once no check has
# started or finished for this long
settle=${PR_CHECKS_SETTLE:-60}
interval=15

die() {
    local message="$1"
    echo "pr_checks: $message" >&2
    exit 2
}

pr=${1:-}
if [[ -z "$pr" ]]; then
    pr="$(gh pr view --json number -q .number)" || die "no pull request for this branch; pass its number"
fi
[[ "$pr" =~ ^[0-9]+$ ]] || die "usage: scripts/pr_checks.sh [pull request number]"

# One line per check: state, name, link. Check runs and commit statuses both count.
checks_filter='.statusCheckRollup[]
    | if .__typename == "CheckRun" then
        [(if .status == "COMPLETED" then .conclusion else "PENDING" end), .name, .detailsUrl]
      else
        [(if .state == "EXPECTED" then "PENDING" else .state end), .context, .targetUrl]
      end
    | @tsv'
# Seconds since the last check started or finished
quiet_filter='[.statusCheckRollup[] | .completedAt, .startedAt, .createdAt
    | select(. != null and (startswith("0001-") | not)) | fromdateiso8601] | now - (max // now) | floor'

# Re-read the pull request each round, so a push during the wait is followed
while :; do
    view="$(gh pr view "$pr" --json headRefOid,statusCheckRollup)" || die "cannot read pull request #$pr"
    head="$(jq -r .headRefOid <<<"$view")"
    checks="$(jq -r "$checks_filter" <<<"$view")"
    quiet="$(jq -r "$quiet_filter" <<<"$view")" || die "cannot read the checks of #$pr"
    if [[ -n "$checks" ]] && ! grep -q '^PENDING' <<<"$checks"; then
        # At the timeout, report finished checks rather than wait for late ones
        [[ "$quiet" -ge "$settle" || "$SECONDS" -ge "$timeout" ]] && break
    elif [[ "$SECONDS" -ge "$timeout" ]]; then
        [[ -n "$checks" ]] || die "no check reported on ${head:0:7} of #$pr after ${timeout}s"
        die "checks on ${head:0:7} of #$pr unfinished after ${timeout}s; see gh pr checks $pr"
    fi
    sleep "$interval"
done

# Annotations: the findings a check left on files, one line each
runs="$(gh api --paginate "repos/{owner}/{repo}/commits/$head/check-runs?per_page=100" \
    --jq '.check_runs[] | select(.output.annotations_count > 0) | [.id, .name] | @tsv')" ||
    die "cannot list the check runs of ${head:0:7}"
findings=""
while IFS=$'\t' read -r id name; do
    [[ -n "$id" ]] || continue
    found="$(gh api --paginate "repos/{owner}/{repo}/check-runs/$id/annotations?per_page=100")" ||
        die "cannot read the annotations of $name"
    findings+="$(jq -r --arg check "$name" '.[]
        | [.annotation_level, $check, "\(.path):\(.start_line)", .title // "",
           (.message | gsub("\\s+"; " ") | ltrimstr(" "))] | @tsv' <<<"$found")"$'\n'
done <<<"$runs"
findings="$(sed '/^$/d' <<<"$findings")"

failing="$(grep -Evc '^(SUCCESS|NEUTRAL|SKIPPED)'$'\t' <<<"$checks" || true)"
n_findings="$(grep -c . <<<"$findings" || true)"

echo "#$pr at ${head:0:7}: $(grep -c . <<<"$checks") check(s), $failing failing, $n_findings finding(s)"
echo "$checks"
[[ -z "$findings" ]] || echo "$findings"

if [[ "$failing" -eq 0 && "$n_findings" -eq 0 ]]; then
    exit 0
fi
exit 1
